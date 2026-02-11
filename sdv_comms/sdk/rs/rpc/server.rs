// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! SDV RPC Server APIs
mod authorizer;
mod server_sd;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use crypto_rpc::cert::verify_identity;
use futures::stream::{Stream, StreamExt};
use futures::TryStreamExt;
use tokio::net::TcpListener as TokioTcpListener;
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use tonic::body::Body;
use tonic::transport::server::TcpIncoming as TonicTcpIncoming;

use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};

use crate::id::ServiceFqin;
use crate::rpc::certificate_provider::ProdTlsProvider;
use crate::rpc::streams::ServerSslStream;
use crate::rpc::streams::SslStreamConnectInfo;
use crate::sd::{UnitRegistrationArgs, UnitType};
use crate::ContextRef;

use super::codec::NoOpCodec;
use super::tls::{init_ssl_stream_server, verify_issuer_vm_matches_fqin, TlsProvider};
use super::util;

use authorizer::{CachedAuthorizer, SdvAuthorizer};
use server_sd::SdvServiceRegistrar;
use tonic::Streaming;

use openssl::x509::X509;

/// Implementation of this trait should provide functionality
/// for registering a service instance.
#[async_trait]
pub(crate) trait ServiceRegistrar: Send {
    async fn register(&mut self) -> SdvResult<TokioTcpListener>;
}

/// Implementation of this trait should answer the question if a caller is
/// authorized to have access to the callee.
#[async_trait]
pub(crate) trait Authorizer {
    async fn is_authorized(&self, subject_fqin: &ServiceFqin) -> SdvResult<bool>;
}

/// Configurable options for the RPC server.
#[derive(Default, Clone)]
pub struct ServerOptions {
    max_incoming_message_size_bytes: Option<usize>,
}

impl ServerOptions {
    /// Create new instance of the options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets maximum size of the incoming message in bytes.
    pub fn set_max_incoming_message_size_bytes(&mut self, size: usize) {
        self.max_incoming_message_size_bytes = Some(size);
    }
}

/// A unary RPC method.
pub type UnaryRpcMethod = Arc<
    dyn Fn(ServiceFqin, &[u8]) -> Pin<Box<dyn Future<Output = SdvResult<Vec<u8>>> + Send>>
        + Send
        + Sync,
>;
/// A server-streaming RPC method.
pub type ServerStreamingRpcMethod = Arc<
    dyn Fn(
            ServiceFqin,
            &[u8],
        ) -> Pin<
            Box<
                dyn Future<
                        Output = SdvResult<Pin<Box<dyn Stream<Item = SdvResult<Vec<u8>>> + Send>>>,
                    > + Send,
            >,
        > + Send
        + Sync,
>;

/// A client-streaming RPC method.
pub type ClientStreamingRpcMethod = Arc<
    dyn Fn(
            ServiceFqin,
            Streaming<bytes::BytesMut>,
        ) -> Pin<Box<dyn Future<Output = SdvResult<Vec<u8>>> + Send>>
        + Send
        + Sync,
>;

#[derive(Clone)]
/// RPC method
pub enum RpcMethod {
    /// Unary RPC method.
    Unary(UnaryRpcMethod),

    /// Server-streaming RPC method
    ServerStreaming(ServerStreamingRpcMethod),

    /// Client-streaming RPC method
    ClientStreaming(ClientStreamingRpcMethod),
}

#[derive(Clone)]
struct TransportService<A: Authorizer + Clone + Send + 'static> {
    request_path_prefix: String,
    rpc_method_map: HashMap<String, RpcMethod>,
    authorizer: A,
    server_options: ServerOptions,
}

impl<A: Authorizer + Clone + Send + 'static> tonic::server::NamedService for TransportService<A> {
    // Tonic requires a static string for the service name, but we need to dynamically
    // determine the fully-qualified proto name. To work around this limitation,
    // we use a capture service name. This name is used by the Tonic library to
    // add the service to the Axum router.
    const NAME: &'static str = "{service}";
}

type BoxFuture<T, E> = Pin<Box<dyn self::Future<Output = Result<T, E>> + Send + 'static>>;

fn apply_server_options<T>(
    server: tonic::server::Grpc<T>,
    server_options: ServerOptions,
) -> tonic::server::Grpc<T>
where
    T: tonic::codec::Codec,
{
    match server_options.max_incoming_message_size_bytes {
        Some(max_incoming_message_size) => {
            server.max_decoding_message_size(max_incoming_message_size)
        }
        None => server,
    }
}

async fn handle_unary_call(
    method: UnaryRpcMethod,
    req: http::Request<tonic::body::Body>,
    caller_fqin: ServiceFqin,
    server_options: ServerOptions,
) -> http::Response<Body> {
    struct Transport {
        method: UnaryRpcMethod,
        caller_fqin: ServiceFqin,
    }

    impl tonic::server::UnaryService<bytes::BytesMut> for Transport {
        type Response = bytes::BytesMut;
        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

        fn call(&mut self, req: tonic::Request<bytes::BytesMut>) -> Self::Future {
            let f = self.method.clone();
            let fqin = self.caller_fqin.clone();

            Box::pin(async move {
                verify_header_fqin(&fqin, &req)?;
                match f(fqin, req.get_ref()).await {
                    Ok(bytes) => Ok(tonic::Response::new(bytes::BytesMut::from(bytes.as_slice()))),
                    Err(status) => {
                        log::warn!("Could not process the request: {}", &status);
                        Err(sdv_status_to_tonic_status(&status))
                    }
                }
            })
        }
    }

    let server = tonic::server::Grpc::new(NoOpCodec::default());
    let mut server = apply_server_options(server, server_options);
    server.unary(Transport { method, caller_fqin }, req).await
}

async fn handle_server_streaming_call(
    method: ServerStreamingRpcMethod,
    req: http::Request<tonic::body::Body>,
    caller_fqin: ServiceFqin,
    server_options: ServerOptions,
) -> http::Response<Body> {
    struct StreamingTransport {
        method: ServerStreamingRpcMethod,
        caller_fqin: ServiceFqin,
    }

    impl tonic::server::ServerStreamingService<bytes::BytesMut> for StreamingTransport {
        type Response = bytes::BytesMut;
        type ResponseStream =
            Pin<Box<dyn Stream<Item = Result<Self::Response, tonic::Status>> + Send>>;
        type Future = BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;

        fn call(&mut self, req: tonic::Request<bytes::BytesMut>) -> Self::Future {
            let f = self.method.clone();
            let fqin = self.caller_fqin.clone();

            Box::pin(async move {
                verify_header_fqin(&fqin, &req)?;
                match f(fqin, req.get_ref()).await {
                    Ok(stream) => Ok(tonic::Response::new(
                        stream
                            .map(|item| {
                                item.map(|bytes| bytes::BytesMut::from(bytes.as_slice()))
                                    .map_err(|e| tonic::Status::unknown(e.to_string()))
                            })
                            .boxed(),
                    )),
                    Err(status) => {
                        log::warn!("Could not process the request: {}", &status);
                        Err(sdv_status_to_tonic_status(&status))
                    }
                }
            })
        }
    }

    let server = tonic::server::Grpc::new(NoOpCodec::default());
    let mut server = apply_server_options(server, server_options);
    server.server_streaming(StreamingTransport { method, caller_fqin }, req).await
}

async fn handle_client_streaming_call(
    method: ClientStreamingRpcMethod,
    req: http::Request<tonic::body::Body>,
    caller_fqin: ServiceFqin,
    server_options: ServerOptions,
) -> http::Response<Body> {
    struct ClientStreamingTransport {
        method: ClientStreamingRpcMethod,
        caller_fqin: ServiceFqin,
    }

    impl tonic::server::ClientStreamingService<bytes::BytesMut> for ClientStreamingTransport {
        type Response = bytes::BytesMut;
        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

        fn call(&mut self, req: tonic::Request<Streaming<bytes::BytesMut>>) -> Self::Future {
            let f = self.method.clone();
            let fqin = self.caller_fqin.clone();

            Box::pin(async move {
                verify_header_fqin(&fqin, &req)?;
                match f(fqin, req.into_inner()).await {
                    Ok(bytes) => Ok(tonic::Response::new(bytes::BytesMut::from(bytes.as_slice()))),
                    Err(status) => {
                        log::warn!("Could not process the request {}", &status);
                        Err(sdv_status_to_tonic_status(&status))
                    }
                }
            })
        }
    }

    let server = tonic::server::Grpc::new(NoOpCodec::default());
    let mut server = apply_server_options(server, server_options);
    server.client_streaming(ClientStreamingTransport { method, caller_fqin }, req).await
}

#[allow(clippy::result_large_err)]
fn verify_header_fqin<T>(fqin: &ServiceFqin, req: &tonic::Request<T>) -> Result<(), tonic::Status> {
    let conn_info = req.extensions().get::<SslStreamConnectInfo>();
    let conn_info = match conn_info {
        Some(conn_info) => conn_info,
        None => {
            // We are using insecure channel
            return Ok(());
        }
    };

    let peer_cert: &X509 = match conn_info.peer_cert.as_ref() {
        Ok(peer_cert) => peer_cert,
        Err(_e) => {
            return Err(tonic::Status::unauthenticated("No FQIN in the certificate"));
        }
    };

    let client_verification_passed: bool = verify_identity(peer_cert, &fqin.to_string())
        .map_err(|err| tonic::Status::internal(err.to_string()))?;

    if !client_verification_passed {
        let error_msg = format!("Expected identity {fqin} doesn't match certificate.");
        return Err(tonic::Status::unauthenticated(error_msg));
    }

    verify_issuer_vm_matches_fqin(peer_cert, fqin)
        .map_err(|err| sdv_status_to_tonic_status(&err))?;

    Ok(())
}

fn get_fqin(headers: &http::HeaderMap<http::HeaderValue>) -> SdvResult<ServiceFqin> {
    let header_value = match headers.get("x-sdv-fqin") {
        Some(header_value) => header_value,
        None => {
            return Err(SdvStatus::with_message(
                SdvStatusCode::Unauthenticated,
                "FQIN header is not provided",
            ));
        }
    };

    let fqin_str = match header_value.to_str() {
        Ok(fqin_str) => fqin_str,
        Err(_) => {
            return Err(SdvStatus::with_message(
                SdvStatusCode::Unauthenticated,
                "FQIN header can't be converted to string",
            ));
        }
    };

    let fqin = match ServiceFqin::from_str(fqin_str) {
        Ok(fqin) => fqin,
        Err(_) => {
            return Err(SdvStatus::with_message(
                SdvStatusCode::Unauthenticated,
                "FQIN header can't be parsed",
            ));
        }
    };

    Ok(fqin)
}

fn build_error_response(error_code: tonic::Code, msg: impl Into<String>) -> http::Response<Body> {
    tonic::Status::new(error_code, msg).into_http()
}

impl<A: Authorizer + Clone + Send + 'static> TransportService<A> {
    fn extract_method_name(&self, path: &str) -> String {
        path.strip_prefix(&self.request_path_prefix)
            .unwrap_or_else(|| {
                log::error!(
                    "Request path {} didn't match the expected request prefix: {}",
                    path,
                    self.request_path_prefix
                );
                "default"
            })
            .to_string()
    }
}

impl<A: Authorizer + Clone + Send + 'static> tower::Service<http::Request<tonic::body::Body>>
    for TransportService<A>
{
    type Response = http::Response<Body>;
    type Error = std::convert::Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<tonic::body::Body>) -> Self::Future {
        let path = req.uri().path();
        let method_name = self.extract_method_name(path);

        let caller_fqin = match get_fqin(req.headers()) {
            Ok(fqin) => fqin,
            Err(err) => {
                log::warn!("Could not obtain caller FQIN from the header");
                let error_response = async move {
                    Ok(build_error_response(tonic::Code::Unauthenticated, err.to_string()))
                };

                return Box::pin(error_response);
            }
        };

        let handler_authorizer = self.authorizer.clone();
        let handler_method: Option<RpcMethod> = self.rpc_method_map.get(&method_name).cloned();
        let server_options = self.server_options.clone();

        let fut = async move {
            let authorized = { handler_authorizer.is_authorized(&caller_fqin).await };

            match authorized {
                Ok(true) => {
                    // Authorization successful, continue with the request logic.
                }
                Ok(false) => {
                    return Ok(build_error_response(
                        tonic::Code::PermissionDenied,
                        "ACL check didn't allow this operation",
                    ));
                }
                Err(err) => {
                    return Ok(build_error_response(
                        tonic::Code::PermissionDenied,
                        format!("Authorization check failed: {err}"),
                    ));
                }
            }

            match handler_method {
                Some(RpcMethod::Unary(f)) => {
                    log::debug!("Passing request to unary handler {}", &method_name);
                    Ok(handle_unary_call(f, req, caller_fqin, server_options).await)
                }
                Some(RpcMethod::ServerStreaming(f)) => {
                    log::debug!("Passing request to server streaming handler {}", &method_name);
                    Ok(handle_server_streaming_call(f, req, caller_fqin, server_options).await)
                }
                Some(RpcMethod::ClientStreaming(f)) => {
                    log::debug!("Passing request to client streaming handler {}", &method_name);
                    Ok(handle_client_streaming_call(f, req, caller_fqin, server_options).await)
                }
                None => {
                    log::warn!("No matching handler found for the path {}", &req.uri().path());
                    Ok(build_error_response(
                        tonic::Code::Unimplemented,
                        "Could not find the handler for the operation",
                    ))
                }
            }
        };

        Box::pin(fut)
    }
}

/// A type that represents an instance of RPC server.
pub struct Server {
    // Connection handle is used to stop receiving new connections.
    // It will be part of graceful shutdown API.
    #[allow(dead_code)]
    shutdown_tx: Option<oneshot::Sender<()>>,
    // This is used to unregister server from SD in drop()
    _service_registrar: Arc<Mutex<dyn ServiceRegistrar>>,
}

impl Drop for Server {
    // This only initiates server shutdown. The server will be
    // completely shutdown some time later.
    fn drop(&mut self) {
        // Trigger the server shutdown.
        if let Some(sender) = self.shutdown_tx.take() {
            if sender.send(()).is_err() {
                log::warn!("Failed to trigger server shutdown");
            }
        }
    }
}

impl Server {
    /// Non blocking.
    ///
    /// Creates a running and listening RPC server asynchronously.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_reg_args: Unit registration arguments
    ///    [IN] unary_methods_map: map between the unary methods name and the corresponding handler;
    ///    [IN] server_options: options for the RPC server.
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains a connected RPC
    ///   server instance, or a non-Ok SdvStatus to indicate any error.
    pub async fn create(
        _context: ContextRef,
        unit_reg_args: UnitRegistrationArgs,
        unary_methods_map: HashMap<String, UnaryRpcMethod>,
        server_options: &ServerOptions,
    ) -> SdvResult<Server> {
        let methods_map =
            unary_methods_map.into_iter().map(|(k, v)| (k, RpcMethod::Unary(v))).collect();
        Self::create_with_generic_handlers(_context, unit_reg_args, methods_map, server_options)
            .await
    }

    /// Non blocking.
    ///
    /// Creates a running and listening RPC server asynchronously, accepts both unary and
    /// server-streaming handlers.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_reg_args: Unit registration arguments
    ///    [IN] methods_map: map between the methods name and the corresponding handler;
    ///    [IN] server_options: options for the RPC server.
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains a connected RPC
    ///   server instance, or a non-Ok SdvStatus to indicate any error.
    pub async fn create_with_generic_handlers(
        context: ContextRef,
        unit_reg_args: UnitRegistrationArgs,
        methods_map: HashMap<String, RpcMethod>,
        server_options: &ServerOptions,
    ) -> SdvResult<Server> {
        let sdv_authorizer =
            SdvAuthorizer::new(&context.get_self_fqin(), &unit_reg_args.get_unit_name()).await;

        let unit_type: UnitType = unit_reg_args.get_unit_type();
        let service_registrar = Arc::new(Mutex::new(SdvServiceRegistrar::new(unit_reg_args)));
        let tls_provider = ProdTlsProvider::new(&context.get_self_fqin().to_string()).await?;

        Self::create_impl(
            unit_type,
            methods_map,
            service_registrar,
            tls_provider,
            sdv_authorizer,
            server_options.clone(),
        )
        .await
    }

    // This function creates a new gRPC server.
    pub(crate) async fn create_impl<A: Authorizer + Clone + Send + Sync + 'static>(
        unit_type: UnitType,
        methods_map: HashMap<String, RpcMethod>,
        service_registrar: Arc<Mutex<dyn ServiceRegistrar>>,
        tls_provider: Option<impl TlsProvider + 'static>,
        authorizer: A,
        server_options: ServerOptions,
    ) -> SdvResult<Server> {
        let tcp_listener: TokioTcpListener = service_registrar.lock().await.register().await?;

        let incoming_stream = TonicTcpIncoming::from(tcp_listener).with_nodelay(Some(true));
        let service_name = util::get_service_name_from_unit_type(&unit_type);
        let request_path_prefix = format!("/{service_name}/");

        log::info!("Setting up the RPC server with a path {}", &request_path_prefix);

        let transport_service = TransportService {
            request_path_prefix,
            rpc_method_map: methods_map,
            authorizer: CachedAuthorizer::new(authorizer),
            server_options,
        };

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let shutdown_future = async {
            if shutdown_rx.await.is_err() {
                log::warn!("The shutdown signal sender was dropped");
            }
        };

        match tls_provider {
            Some(tls_provider) => {
                let encrypted_stream = incoming_stream.and_then(move |stream| {
                    let tls_data = tls_provider.get_tls_data();
                    async move {
                        let incoming_ssl_stream = init_ssl_stream_server(stream, tls_data).await;

                        let connection = ServerSslStream(incoming_ssl_stream);
                        Ok(connection)
                    }
                });

                let fut = async {
                    let _ = tonic::transport::Server::builder()
                        .add_service(transport_service)
                        .serve_with_incoming_shutdown(encrypted_stream, shutdown_future)
                        .await;
                };
                tokio::task::spawn(fut); // server is running asynchronously
            }
            None => {
                let fut = async {
                    let _ = tonic::transport::Server::builder()
                        .add_service(transport_service)
                        .serve_with_incoming_shutdown(incoming_stream, shutdown_future)
                        .await;
                };
                tokio::task::spawn(fut); // server is running asynchronously
            }
        }

        Ok(Server { shutdown_tx: Some(shutdown_tx), _service_registrar: service_registrar })
    }
}

fn sdv_status_to_tonic_status(sdv_status: &SdvStatus) -> tonic::Status {
    if sdv_status.is_ok() {
        return tonic::Status::ok("Ok");
    }

    // SDV Status and Tonic Status are the same errors ordered in the same way.
    // So we can just cast one enum to another. Also SdvStatus doesn't allow
    // to obtain the error message. Let's just convert it into string instead.
    tonic::Status::new((sdv_status.status_code() as i32).into(), sdv_status.to_string())
}

// Unit tests that don't depend on the major SDV components, e.g. there is no
// dependency from Service Discovery, ACL Authz Agent, RPC Agent, etc.
// Please note that there is still some dependency on SDV due to the
// implementation of many SDV types in shared library behind FFI layer.
#[cfg(test)]
mod tests {
    use crate::id::ServiceFqin;
    use crate::rpc::server::get_fqin;
    use crate::rpc::server::sdv_status_to_tonic_status;
    use crate::rpc::testing::fake_authorizer::Access;
    use crate::rpc::testing::fake_certificate_provider::{
        FakeCertificateAuthority, FakeTlsProvider,
    };
    use crate::rpc::testing::fake_connector::RegistrationRecord;
    use crate::rpc::{
        ClientStreamingRpcMethod, RpcMethod, ServerStreamingRpcMethod, UnaryRpcMethod,
    };
    use sdv_status_rs::SdvStatus;
    use sdv_status_rs::SdvStatusCode;

    use http::HeaderMap;
    use http::HeaderValue;
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::sync::Arc;
    use std::sync::Mutex;

    static SERVER_FQIN_STR: &str = "vm-cdc:com.android.sdv.rpc.Server/default";
    static CLIENT_FQIN_STR: &str = "vm-cdc:com.android.sdv.rpc.Client/default";
    static ANOTHER_SERVER_FQIN_STR: &str = "vm-cdc:com.google.sdv.another.rpc.Server/default";
    static ANOTHER_CLIENT_FQIN_STR: &str = "vm-cdc:com.google.sdv.another.rpc.Client/default";
    static ANOTHER_VM_SERVER_FQIN_STR: &str = "vm2:com.android.sdv.rpc.Server/default";
    static ANOTHER_VM_CLIENT_FQIN_STR: &str = "vm2:com.android.sdv.rpc.Client/default";

    // This module contains functions that return the default and most used
    // values in the unit tests.
    mod test_default {
        use crate::id::ServiceFqin;
        use crate::rpc::client::internal::TargetServiceInfo;
        use crate::rpc::server::tests::CLIENT_FQIN_STR;
        use crate::rpc::server::tests::SERVER_FQIN_STR;
        use crate::rpc::testing::fake_authorizer::Access;
        use crate::rpc::testing::fake_authorizer::FakeAuthorizer;
        use crate::rpc::testing::fake_certificate_provider::FakeTlsProvider;
        use crate::rpc::testing::fake_connector::FakeConnector;
        use crate::rpc::testing::fake_connector::FakeServiceRegistrar;
        use crate::rpc::testing::fake_connector::RegistrationRecord;
        use crate::rpc::Client;
        use crate::rpc::ClientOptions;
        use crate::rpc::RpcMethod;
        use crate::rpc::Server;
        use crate::rpc::ServerOptions;
        use crate::rpc::UnaryRpcMethod;
        use crate::sd::UnitRegistrationArgs;
        use crate::sd::UnitType;
        use sdv_status_rs::SdvResult;
        use std::collections::HashMap;
        use std::str::FromStr;
        use std::sync::Arc;
        use std::sync::Mutex;

        static UNIT_NAME: &str = "unitname";
        pub(super) static METHOD_NAME: &str = "SayHello";

        pub(super) fn get_server_target_info() -> TargetServiceInfo {
            TargetServiceInfo {
                service_unit_name: UNIT_NAME.to_string(),
                unit_type: get_server_unit_type(),
                fqin: get_server_fqin(),
            }
        }

        pub(super) fn get_server_unit_type() -> UnitType {
            UnitType::builder()
                .set_sdv_package_name("com.example.unit.type")
                .set_service_bundle_name("UnitTypeBundleName")
                .set_unit_type_name("Typename")
                .build()
                .expect("Can't create unit type")
        }

        pub(super) fn get_server_reg_arguments() -> UnitRegistrationArgs {
            let mut unit_registration_args = UnitRegistrationArgs::new();
            unit_registration_args.set_unit_name(UNIT_NAME);

            let ut = get_server_unit_type();

            unit_registration_args.set_unit_type(&ut);
            unit_registration_args
        }

        pub(super) fn get_client_fqin() -> ServiceFqin {
            ServiceFqin::from_str(CLIENT_FQIN_STR).expect("Valid ServiceFqin is expected")
        }

        pub(super) fn get_server_fqin() -> ServiceFqin {
            ServiceFqin::from_str(SERVER_FQIN_STR).expect("Valid ServiceFqin is expected")
        }

        pub(super) fn create_default_methods_map() -> HashMap<String, RpcMethod> {
            let callback: UnaryRpcMethod =
                Arc::new(move |_caller_fqin: ServiceFqin, data: &[u8]| {
                    let message = String::from_utf8_lossy(data);
                    let response = format!("Hello! You said: {message}");
                    Box::pin(async move { Ok(response.into_bytes()) })
                });

            super::to_methods_map(METHOD_NAME, callback)
        }

        // Creates an RPC server with most popular configuration.
        pub(super) async fn create_server(
            service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>>,
            tls_provider: Option<FakeTlsProvider>,
        ) -> crate::rpc::Server {
            let methods_map: HashMap<String, RpcMethod> = create_default_methods_map();
            create_custom_server(methods_map, Access::Allow, service_db, tls_provider).await
        }

        pub(super) async fn create_custom_server(
            methods_map: HashMap<String, RpcMethod>,
            access: Access,
            service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>>,
            tls_provider: Option<FakeTlsProvider>,
        ) -> crate::rpc::Server {
            let reg_args = get_server_reg_arguments();
            let registrar = Arc::new(tokio::sync::Mutex::new(FakeServiceRegistrar::new(
                service_db,
                reg_args.get_unit_name(),
            )));

            Server::create_impl(
                reg_args.get_unit_type(),
                methods_map,
                registrar,
                tls_provider,
                FakeAuthorizer::new(access),
                ServerOptions::default(),
            )
            .await
            .expect("Could not create RPC server")
        }

        pub(super) async fn create_client(
            service_db: Arc<std::sync::Mutex<HashMap<String, RegistrationRecord>>>,
            tls_provider: Option<FakeTlsProvider>,
        ) -> Client {
            try_create_client(service_db, tls_provider).await.expect("Could not create RPC client")
        }

        pub(super) async fn create_client_with_fqin(
            service_db: Arc<std::sync::Mutex<HashMap<String, RegistrationRecord>>>,
            tls_provider: Option<FakeTlsProvider>,
            fqin: &str,
        ) -> Client {
            Client::create_impl(
                ServiceFqin::from_str(fqin).unwrap(),
                get_server_target_info(),
                FakeConnector::new(service_db),
                tls_provider,
                &ClientOptions::default(),
            )
            .await
            .expect("Could not create RPC client with fqin")
        }

        pub(super) async fn try_create_client(
            service_db: Arc<std::sync::Mutex<HashMap<String, RegistrationRecord>>>,
            tls_provider: Option<FakeTlsProvider>,
        ) -> SdvResult<Client> {
            Client::create_impl(
                get_client_fqin(),
                get_server_target_info(),
                FakeConnector::new(service_db),
                tls_provider,
                &ClientOptions::default(),
            )
            .await
        }
    }

    fn get_vm_name_of_fqin(fqin: &str) -> String {
        let fqin = ServiceFqin::from_str(fqin).expect("Valid ServiceFqin is expected");
        fqin.get_sdv_vm_name().to_owned()
    }

    fn to_methods_map(method_name: &str, callback: UnaryRpcMethod) -> HashMap<String, RpcMethod> {
        [(method_name.to_string(), RpcMethod::Unary(callback))].iter().cloned().collect()
    }

    // Unit test of the simple RPC call.
    #[tokio::test]
    async fn unit_test_server_call() {
        let service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>> = Default::default();
        let _server = test_default::create_server(service_db.clone(), None).await;

        let client = test_default::create_client(service_db.clone(), None).await;

        let request = b"This is a test message";
        let response = client.unary_call(test_default::METHOD_NAME, request).await;
        assert!(response.is_ok());
        assert_eq!(response.unwrap(), b"Hello! You said: This is a test message");
    }

    // This test verifies that a secure server correctly rejects connection attempts
    // from insecure clients, while still allowing subsequent connections from secure clients.
    #[tokio::test]
    async fn secure_server_rejects_insecure_client() {
        let service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>> = Default::default();
        let fake_ca = FakeCertificateAuthority::new(&get_vm_name_of_fqin(SERVER_FQIN_STR));
        let cert_provider_server = FakeTlsProvider::new(&fake_ca, SERVER_FQIN_STR);
        let cert_provider_client = FakeTlsProvider::new(&fake_ca, CLIENT_FQIN_STR);
        let _server =
            test_default::create_server(service_db.clone(), Some(cert_provider_server)).await;

        let insecure_client = test_default::create_client(service_db.clone(), None).await;

        let request = b"This is a test message";
        let response = insecure_client.unary_call(test_default::METHOD_NAME, request).await;
        assert!(response.is_err(), "Insecure clients can't access secure servers");

        // This connection failure should not affect the server, new client should connect successfully.
        let client =
            test_default::create_client(service_db.clone(), Some(cert_provider_client)).await;

        let response = client.unary_call(test_default::METHOD_NAME, request).await;
        assert!(response.is_ok());
        assert_eq!(response.unwrap(), b"Hello! You said: This is a test message");
    }

    // Unit test of the simple RPC call with TLS.
    // TODO(b/378853443): Use parameterized tests instead.
    #[tokio::test]
    async fn unit_test_server_call_tls() {
        let service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>> = Default::default();
        let fake_ca = FakeCertificateAuthority::new(&get_vm_name_of_fqin(SERVER_FQIN_STR));
        let cert_provider_server = FakeTlsProvider::new(&fake_ca, SERVER_FQIN_STR);
        let cert_provider_client = FakeTlsProvider::new(&fake_ca, CLIENT_FQIN_STR);
        let _server =
            test_default::create_server(service_db.clone(), Some(cert_provider_server)).await;

        let client =
            test_default::create_client(service_db.clone(), Some(cert_provider_client)).await;

        let request = b"This is a test message";
        let response = client.unary_call(test_default::METHOD_NAME, request).await;
        assert!(response.is_ok());
        assert_eq!(response.unwrap(), b"Hello! You said: This is a test message");
    }

    // Unit test of ACL access deny.
    #[tokio::test]
    async fn unit_test_acl_deny() {
        let callback: UnaryRpcMethod = Arc::new(move |_caller_fqin: ServiceFqin, _data: &[u8]| {
            // If access is denied the handler should not be called.
            panic!();
        });
        let method_name = "Panic";
        let methods_map: HashMap<String, RpcMethod> = to_methods_map(method_name, callback);

        let service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>> = Default::default();

        let _server =
            test_default::create_custom_server(methods_map, Access::Deny, service_db.clone(), None)
                .await;
        let client = test_default::create_client(service_db.clone(), None).await;

        let request = b"This should not be read";
        let response = client.unary_call(method_name, request).await;
        assert!(response.is_err());
        assert_eq!(response.unwrap_err().status_code(), SdvStatusCode::PermissionDenied);
    }

    // Unit test of ACL access deny with TLS.
    // TODO(b/378853443): Use parameterized tests instead.
    #[tokio::test]
    async fn unit_test_acl_deny_tls() {
        let callback: UnaryRpcMethod = Arc::new(move |_caller_fqin: ServiceFqin, _data: &[u8]| {
            // If access is denied the handler should not be called.
            panic!();
        });
        let method_name = "Panic";
        let methods_map: HashMap<String, RpcMethod> = to_methods_map(method_name, callback);

        let service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>> = Default::default();
        let fake_ca = FakeCertificateAuthority::new(&get_vm_name_of_fqin(SERVER_FQIN_STR));
        let cert_provider_server = FakeTlsProvider::new(&fake_ca, SERVER_FQIN_STR);
        let cert_provider_client = FakeTlsProvider::new(&fake_ca, CLIENT_FQIN_STR);

        let _server = test_default::create_custom_server(
            methods_map,
            Access::Deny,
            service_db.clone(),
            Some(cert_provider_server),
        )
        .await;
        let client =
            test_default::create_client(service_db.clone(), Some(cert_provider_client)).await;

        let request = b"This should not be read";
        let response = client.unary_call(method_name, request).await;
        assert!(response.is_err());
        assert_eq!(response.unwrap_err().status_code(), SdvStatusCode::PermissionDenied);
    }

    // Unit test of client streaming ACL access deny with TLS.
    #[tokio::test]
    async fn unit_test_client_streaming_acl_deny_tls() {
        use futures::Stream;
        use std::pin::Pin;

        let callback: ClientStreamingRpcMethod = Arc::new(
            move |_caller_fqin: ServiceFqin, _request: tonic::Streaming<bytes::BytesMut>| {
                // If access is denied the handler should not be called.
                panic!();
            },
        );
        let method_name = "Panic";
        let methods_map: HashMap<String, RpcMethod> =
            [(method_name.to_string(), RpcMethod::ClientStreaming(callback))]
                .iter()
                .cloned()
                .collect();

        let service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>> = Default::default();
        let fake_ca = FakeCertificateAuthority::new(&get_vm_name_of_fqin(SERVER_FQIN_STR));
        let cert_provider_server = FakeTlsProvider::new(&fake_ca, SERVER_FQIN_STR);
        let cert_provider_client = FakeTlsProvider::new(&fake_ca, CLIENT_FQIN_STR);

        let _server = test_default::create_custom_server(
            methods_map,
            Access::Deny,
            service_db.clone(),
            Some(cert_provider_server),
        )
        .await;
        let client =
            test_default::create_client(service_db.clone(), Some(cert_provider_client)).await;

        let client_stream_request = {
            let messages = tokio_stream::iter(std::iter::empty::<Vec<u8>>());
            Box::pin(messages) as Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>
        };

        let response = client.client_streaming_call(method_name, client_stream_request).await;
        assert!(response.is_err());
        assert_eq!(response.unwrap_err().status_code(), SdvStatusCode::PermissionDenied);
    }

    // Unit test of server streaming ACL access deny with TLS.
    #[tokio::test]
    async fn unit_test_server_streaming_acl_deny_tls() {
        let callback: ServerStreamingRpcMethod =
            Arc::new(move |_caller_fqin: ServiceFqin, _req: &[u8]| {
                // If access is denied the handler should not be called.
                panic!();
            });
        let method_name = "Panic";
        let methods_map: HashMap<String, RpcMethod> =
            [(method_name.to_string(), RpcMethod::ServerStreaming(callback))]
                .iter()
                .cloned()
                .collect();

        let service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>> = Default::default();
        let fake_ca = FakeCertificateAuthority::new(&get_vm_name_of_fqin(SERVER_FQIN_STR));
        let cert_provider_server = FakeTlsProvider::new(&fake_ca, SERVER_FQIN_STR);
        let cert_provider_client = FakeTlsProvider::new(&fake_ca, CLIENT_FQIN_STR);

        let _server = test_default::create_custom_server(
            methods_map,
            Access::Deny,
            service_db.clone(),
            Some(cert_provider_server),
        )
        .await;
        let client =
            test_default::create_client(service_db.clone(), Some(cert_provider_client)).await;

        let request = b"This should not be read";
        let response = client.server_streaming_call(method_name, request).await;

        // This workaroud is here because unwrap_err() will not work due
        // mo missing fmt::Debug implementation.
        let err = match response {
            Ok(_) => panic!("The error is expected"),
            Err(e) => e,
        };
        assert_eq!(err.status_code(), SdvStatusCode::PermissionDenied);
    }

    // Tests that calling unimplemented method returns an error with
    // associated error description.
    #[tokio::test]
    async fn unit_test_handler_unimplemented() {
        let service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>> = Default::default();
        let _server = test_default::create_server(service_db.clone(), None).await;

        let client = test_default::create_client(service_db.clone(), None).await;

        let request = b"This is a test message";
        let response = client.unary_call("WrongName", request).await;
        assert!(response.is_err());
        let err = response.unwrap_err();
        assert_eq!(err.status_code(), SdvStatusCode::Unimplemented);
        assert!(format!("{err}").contains("Could not find the handler"));
    }

    #[tokio::test]
    async fn unit_test_clients_fqin_doesnt_match_cert() {
        let service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>> = Default::default();

        let fake_ca = FakeCertificateAuthority::new(&get_vm_name_of_fqin(SERVER_FQIN_STR));
        let cert_provider_server = FakeTlsProvider::new(&fake_ca, SERVER_FQIN_STR);
        let cert_provider_client = FakeTlsProvider::new(&fake_ca, ANOTHER_CLIENT_FQIN_STR);
        let _server =
            test_default::create_server(service_db.clone(), Some(cert_provider_server)).await;

        let client =
            test_default::create_client(service_db.clone(), Some(cert_provider_client)).await;

        let request = b"This is a test message";
        let response = client.unary_call(test_default::METHOD_NAME, request).await;
        assert!(response.is_err());
        let err = response.unwrap_err();
        assert_eq!(err.status_code(), SdvStatusCode::Unauthenticated);
        assert!(format!("{err}").contains("doesn't match certificate"));
    }

    #[tokio::test]
    async fn unit_test_server_fqin_doesnt_match_cert() {
        let service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>> = Default::default();

        let fake_ca = FakeCertificateAuthority::new(&get_vm_name_of_fqin(CLIENT_FQIN_STR));
        let cert_provider_server = FakeTlsProvider::new(&fake_ca, ANOTHER_SERVER_FQIN_STR);
        let cert_provider_client = FakeTlsProvider::new(&fake_ca, CLIENT_FQIN_STR);

        let _server =
            test_default::create_server(service_db.clone(), Some(cert_provider_server)).await;

        let client =
            test_default::try_create_client(service_db.clone(), Some(cert_provider_client)).await;
        assert!(client.is_err());
    }

    #[tokio::test]
    async fn unit_test_clients_cert_fqin_doesnt_match_issuer_vm() {
        let service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>> = Default::default();

        // Set the root CA name to a value different than the VM specified in client Fqin.
        // This should trigger an error, since CA should not be able to issue certificates for
        // services on another VM.
        let fake_ca = FakeCertificateAuthority::new(&get_vm_name_of_fqin(SERVER_FQIN_STR));
        let cert_provider_server = FakeTlsProvider::new(&fake_ca, SERVER_FQIN_STR);
        let cert_provider_client = FakeTlsProvider::new(&fake_ca, ANOTHER_VM_CLIENT_FQIN_STR);
        let _server =
            test_default::create_server(service_db.clone(), Some(cert_provider_server)).await;

        let client =
            test_default::create_client(service_db.clone(), Some(cert_provider_client)).await;

        let request = b"This is a test message";
        let response = client.unary_call(test_default::METHOD_NAME, request).await;
        assert!(response.is_err());
        let err = response.unwrap_err();
        assert_eq!(err.status_code(), SdvStatusCode::Unauthenticated);
        assert!(
            err.to_string().contains("doesn't match certificate"),
            "err message {err} is not as expected"
        );
    }

    #[tokio::test]
    async fn unit_test_servers_cert_fqin_doesnt_match_issuer_vm() {
        let service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>> = Default::default();

        // Set the root CA name to a value different than the VM specified in server Fqin.
        // This should trigger an error, since CA should not be able to issue certificates for
        // services on another VM.
        let fake_ca = FakeCertificateAuthority::new(&get_vm_name_of_fqin(CLIENT_FQIN_STR));
        let cert_provider_server = FakeTlsProvider::new(&fake_ca, ANOTHER_VM_SERVER_FQIN_STR);
        let cert_provider_client = FakeTlsProvider::new(&fake_ca, CLIENT_FQIN_STR);

        let _server =
            test_default::create_server(service_db.clone(), Some(cert_provider_server)).await;

        let client =
            test_default::try_create_client(service_db.clone(), Some(cert_provider_client)).await;
        assert!(client.is_err());
    }

    #[test]
    fn test_get_fqin_success() {
        let mut headers = HeaderMap::new();
        let fqin_str = "vm-cdc:com.google.sdv.TestService/default";
        headers.insert("x-sdv-fqin", fqin_str.parse().unwrap());
        let result = get_fqin(&headers);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), fqin_str);
    }

    #[test]
    fn test_get_fqin_header_missing() {
        let headers = HeaderMap::new();
        let result = get_fqin(&headers);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), SdvStatusCode::Unauthenticated);
        assert!(err.to_string().contains("FQIN header is not provided"));
    }

    #[test]
    fn test_get_fqin_header_not_string() {
        let mut headers = HeaderMap::new();
        // Insert bytes that are not valid UTF-8
        headers.insert("x-sdv-fqin", HeaderValue::from_bytes(&[0xC3, 0x28]).unwrap());
        let result = get_fqin(&headers);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), SdvStatusCode::Unauthenticated);
        assert!(err.to_string().contains("FQIN header can't be converted to string"));
    }

    #[test]
    fn test_get_fqin_header_cannot_parse() {
        let mut headers = HeaderMap::new();
        let invalid_fqin_str = "invalid-fqin-format";
        headers.insert("x-sdv-fqin", invalid_fqin_str.parse().unwrap());
        let result = get_fqin(&headers);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), SdvStatusCode::Unauthenticated);
        assert!(err.to_string().contains("FQIN header can't be parsed"));
    }

    #[test]
    fn test_get_fqin_header_empty_string() {
        let mut headers = HeaderMap::new();
        headers.insert("x-sdv-fqin", "".parse().unwrap());
        let result = get_fqin(&headers);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), SdvStatusCode::Unauthenticated);
        assert!(err.to_string().contains("FQIN header can't be parsed"));
    }

    #[test]
    fn test_sdv_status_to_tonic_status_ok() {
        let sdv_status = SdvStatus::new(SdvStatusCode::Ok);
        let tonic_status = sdv_status_to_tonic_status(&sdv_status);
        assert_eq!(tonic_status.code(), tonic::Code::Ok);
        assert_eq!(tonic_status.message(), "Ok");
    }

    #[test]
    fn test_sdv_status_to_tonic_status_error() {
        let sdv_status =
            SdvStatus::with_message(SdvStatusCode::Cancelled, "Operation was cancelled");
        let tonic_status = sdv_status_to_tonic_status(&sdv_status);
        assert_eq!(tonic_status.code(), tonic::Code::Cancelled);
        assert_eq!(tonic_status.message(), "SdvStatusCode::Cancelled(1): Operation was cancelled");
    }

    #[test]
    fn test_sdv_status_to_tonic_status_error_no_message() {
        let sdv_status = SdvStatus::new(SdvStatusCode::Unknown);
        let tonic_status = sdv_status_to_tonic_status(&sdv_status);
        assert_eq!(tonic_status.code(), tonic::Code::Unknown);
        assert_eq!(tonic_status.message(), "SdvStatusCode::Unknown(2)");
    }

    #[tokio::test]
    async fn test_multiple_clients_concurrent_calls() {
        let service_db: Arc<Mutex<HashMap<String, RegistrationRecord>>> = Default::default();

        let fake_ca = FakeCertificateAuthority::new(&get_vm_name_of_fqin(SERVER_FQIN_STR));
        let cert_provider_server = FakeTlsProvider::new(&fake_ca, SERVER_FQIN_STR);
        let cert_provider_client1 = FakeTlsProvider::new(&fake_ca, CLIENT_FQIN_STR);
        let cert_provider_client2 = FakeTlsProvider::new(&fake_ca, ANOTHER_CLIENT_FQIN_STR);

        // Start the server
        let _server =
            test_default::create_server(service_db.clone(), Some(cert_provider_server)).await;

        // Create two independent clients.
        let client1 = test_default::create_client_with_fqin(
            service_db.clone(),
            Some(cert_provider_client1),
            CLIENT_FQIN_STR,
        )
        .await;
        let client2 = test_default::create_client_with_fqin(
            service_db.clone(),
            Some(cert_provider_client2),
            ANOTHER_CLIENT_FQIN_STR,
        )
        .await;
        let client1 = Arc::new(client1);
        let client2 = Arc::new(client2);

        let num_tasks_per_client = 5; // Number of concurrent calls per client
        let mut tasks = Vec::new();

        // Spawn tasks for client 1.
        for i in 0..num_tasks_per_client {
            let client_clone = Arc::clone(&client1);
            let request_message = format!("Client1 Message {i}");
            let request_bytes = request_message.as_bytes().to_vec();

            tasks.push(tokio::spawn(async move {
                client_clone.unary_call(test_default::METHOD_NAME, &request_bytes).await
            }));
        }

        // Spawn tasks for client 2.
        for i in 0..num_tasks_per_client {
            let client_clone = Arc::clone(&client2);
            let request_message = format!("Client2 Message {i}");
            let request_bytes = request_message.as_bytes().to_vec();

            tasks.push(tokio::spawn(async move {
                client_clone.unary_call(test_default::METHOD_NAME, &request_bytes).await
            }));
        }

        // Await all tasks and verify responses.
        for (i, task) in tasks.into_iter().enumerate() {
            let response = task.await.expect("Task panicked or failed to complete");
            assert!(response.is_ok(), "Call {} failed: {:?}", i, response.err());
            let received_response = response.unwrap();

            // Determine which client and message this response corresponds to.
            let client_idx = if i < num_tasks_per_client { 1 } else { 2 };
            let msg_idx = i % num_tasks_per_client; // Get the original message index for client

            let expected_message = format!("Client{client_idx} Message {msg_idx}");
            let expected_response = format!("Hello! You said: {expected_message}");

            assert_eq!(received_response, expected_response.as_bytes(), "Mismatch for call {i}");
        }
    }
}
