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

//! SDV RPC Client APIs
mod client_sd;

use std::pin::Pin;
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream::Stream;
use futures::stream::StreamExt;
use hyper_util::rt::tokio::TokioIo;
use tokio::net::TcpStream as TokioTcpStream;
use tonic::client::Grpc;
use tonic::transport::{Channel, Endpoint};
use tonic::{IntoRequest, Response};
use tower::service_fn;

use crypto_rpc::cert::verify_identity;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};

use crate::id::ServiceFqin;
use crate::id::UnitIdentifier;
use crate::rpc::certificate_provider::ProdTlsProvider;
use crate::rpc::client::client_sd::SdvServiceDiscoverer;
use crate::rpc::streams::extract_cert;
use crate::rpc::tls::TlsProvider;
use crate::rpc::util;
use crate::sd::UnitType;
use crate::ContextRef;

use super::codec::NoOpCodec;
use super::tls::{init_ssl_stream_client, verify_issuer_vm_matches_fqin};

/// Implementations of this trait should provide functionality
/// for connecting to the RPC server.
#[async_trait]
pub(crate) trait Connector {
    async fn connect(&self, fqin: &ServiceFqin, unit_name: &str) -> SdvResult<TokioTcpStream>;
}

/// Configurable options for the RPC client.
#[derive(Default)]
pub struct ClientOptions {
    max_incoming_message_size_bytes: Option<usize>,
    timeout: Option<std::time::Duration>,
}

impl ClientOptions {
    /// Create new instance of the options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets maximum size of the incoming message in bytes.
    pub fn set_max_incoming_message_size_bytes(&mut self, size: usize) {
        self.max_incoming_message_size_bytes = Some(size);
    }

    /// Sets the max duration the request.
    /// It adds the `grpc-timeout` header to each RPC request.
    /// More details are here:
    /// https://docs.rs/tonic/latest/tonic/struct.Request.html#method.set_timeout
    pub fn set_timeout(&mut self, timeout: std::time::Duration) {
        self.timeout = Some(timeout);
    }
}

fn set_headers<T>(
    tonic_request: &mut tonic::Request<T>,
    fqin_str: &str,
    timeout: &Option<Duration>,
) {
    tonic_request
        .metadata_mut()
        .insert("x-sdv-fqin", tonic::metadata::MetadataValue::from_str(fqin_str).unwrap());

    // This sets grpc-timeout header.
    if let Some(timeout_value) = timeout {
        tonic_request.set_timeout(*timeout_value);
    }
}

fn prepare_unary_request(
    request: &[u8],
    fqin_str: &str,
    timeout: &Option<Duration>,
) -> tonic::Request<bytes::BytesMut> {
    let rpc_request = bytes::BytesMut::from(request);
    let mut tonic_request: tonic::Request<bytes::BytesMut> = rpc_request.into_request();

    set_headers(&mut tonic_request, fqin_str, timeout);

    tonic_request
}

fn prepare_streaming_request(
    stream: Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>,
    fqin_str: &str,
    timeout: &Option<Duration>,
) -> tonic::Request<Pin<Box<dyn Stream<Item = bytes::BytesMut> + Send>>> {
    let mut tonic_request =
        tonic::Request::new(stream.map(|bytes| bytes::BytesMut::from(bytes.as_slice())).boxed());

    set_headers(&mut tonic_request, fqin_str, timeout);

    tonic_request
}

fn build_request_path_prefix(unit_type: &UnitType) -> String {
    let service_name = util::get_service_name_from_unit_type(unit_type);
    let request_path_prefix = format!("/{service_name}/");
    request_path_prefix
}

/// A connected SDV client.
pub struct Client {
    client: Grpc<Channel>,
    fqin_str: String,
    request_path_prefix: String,
    request_timeout: Option<Duration>,
}

/// Internal module for the RPC client.
///
/// This module should be used only for internal testing purposes.
pub mod internal {
    use super::*;

    /// Contains information needed for the RPC client to connect to a target
    /// RPC server.
    ///
    /// This struct encapsulates the necessary details for establishing a connection
    /// to a specific service instance. It includes the name of the service unit,
    /// the type of the unit, and the fully qualified instance name (FQIN).
    #[derive(Clone, Debug)]
    pub struct TargetServiceInfo {
        /// The name of the service unit.
        ///
        /// This is a human-readable name that identifies the specific service unit
        /// within the target service.
        pub service_unit_name: String,
        /// The type of the service unit.
        ///
        /// This provides information about the capabilities and interface of the
        /// service unit.
        pub unit_type: UnitType,
        /// The Fully Qualified Instance Name (FQIN) of the target service.
        ///
        /// This is a globally unique identifier that specifies the exact instance
        /// of the service to connect to.
        pub fqin: ServiceFqin,
    }

    /// Creates a connected RPC client asynchronously.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_identifier: Unit identifier for the Server this client should connect to
    ///    [IN] client_options: options for the RPC client
    ///    [IN] target_service_info: RPC server identities
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains a connected RPC
    ///   client instance, or a non-Ok SdvStatus to indicate any error.
    #[allow(dead_code)]
    pub async fn create(
        context: ContextRef,
        target_service_info: TargetServiceInfo,
        client_options: &ClientOptions,
    ) -> SdvResult<Client> {
        let discoverer = SdvServiceDiscoverer;

        let tls_provider = ProdTlsProvider::new(&context.get_self_fqin().to_string()).await?;

        Client::create_impl(
            context.get_self_fqin(),
            target_service_info,
            discoverer,
            tls_provider,
            client_options,
        )
        .await
    }
}

impl Client {
    /// Creates a connected RPC client asynchronously.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_identifier: Unit identifier for the Server this client should connect to
    ///    [IN] client_options: options for the RPC client
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains a connected RPC
    ///   client instance, or a non-Ok SdvStatus to indicate any error.
    pub async fn create(
        context: ContextRef,
        unit_identifier: UnitIdentifier,
        client_options: &ClientOptions,
    ) -> SdvResult<Client> {
        let connector = SdvServiceDiscoverer;

        let tls_provider = ProdTlsProvider::new(&context.get_self_fqin().to_string()).await?;

        let target_service_info = client_sd::get_service_unit_type(
            unit_identifier.get_fqin(),
            unit_identifier.get_unit_name(),
        )
        .await?;

        Self::create_impl(
            context.get_self_fqin(),
            target_service_info,
            connector,
            tls_provider,
            client_options,
        )
        .await
    }

    // This function creates a new gRPC client.
    //
    // If `cert_provider` is provided the TLS connection to the server will be established using
    // the data obtained from `cert_provider`. Otherwise the connection will be unencrypted.
    pub(crate) async fn create_impl<C: Connector + Sync + Send + Clone + 'static>(
        self_fqin: ServiceFqin,
        target_service_info: internal::TargetServiceInfo,
        connector: C,
        tls_provider: Option<impl TlsProvider + 'static>,
        client_options: &ClientOptions,
    ) -> SdvResult<Client> {
        let request_path_prefix = build_request_path_prefix(&target_service_info.unit_type);
        log::debug!("Client's request path was set to: {request_path_prefix}");
        // Workaround: Channel creation currently requires an Endpoint,
        // even though the URI is unused in our connector.
        // See similar usage in Tonic's connect_with_connector example:
        // https://github.com/hyperium/tonic/blob/master/examples/src/uds/client.rs#L21
        let unused_addr = "http://[::]:50051";
        let endpoint = Endpoint::from_str(unused_addr).unwrap();
        let channel = match tls_provider {
            Some(tls_provider) => {
                endpoint
                    .connect_with_connector(service_fn(move |_: tonic::transport::Uri| {
                        let unit_name = target_service_info.service_unit_name.clone();
                        let connector = connector.clone();
                        let tls_data = tls_provider.get_tls_data();
                        let server_fqin = target_service_info.fqin.clone();
                        async move {
                            let stream = connector.connect(&server_fqin, &unit_name).await?;

                            let ssl_stream = init_ssl_stream_client(stream, tls_data).await?;
                            verify_server_fqin(&server_fqin, &ssl_stream)?;
                            SdvResult::Ok(TokioIo::new(ssl_stream))
                        }
                    }))
                    .await
            }
            None => {
                endpoint
                    .connect_with_connector(service_fn(move |_: tonic::transport::Uri| {
                        let unit_name = target_service_info.service_unit_name.clone();
                        let server_fqin = target_service_info.fqin.clone();
                        let connector = connector.clone();
                        async move {
                            SdvResult::Ok(TokioIo::new(
                                connector.connect(&server_fqin, &unit_name).await?,
                            ))
                        }
                    }))
                    .await
            }
        };
        match channel {
            Ok(ch) => {
                let client = Grpc::new(ch);
                let request_timeout = client_options.timeout;
                let client = match client_options.max_incoming_message_size_bytes {
                    Some(max_incoming_message_size) => {
                        client.max_decoding_message_size(max_incoming_message_size)
                    }
                    None => client,
                };

                Ok(Self {
                    client,
                    fqin_str: self_fqin.to_string(),
                    request_path_prefix,
                    request_timeout,
                })
            }
            Err(e) => {
                log::error!("Could not get channel");
                Err(SdvStatus::with_cause_and_message(
                    SdvStatusCode::Unavailable,
                    e,
                    "Could not get channel",
                ))
            }
        }
    }

    /// Executes a unary RPC call asynchronously.
    ///
    /// Parameters:
    ///    [IN] client: connected SDV RPC client;
    ///    [IN] method_name: name of the remote RPC method;
    ///    [IN] request: buffer where the RPC request bytes are stored.
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains RPC response bytes,
    ///   or a non-Ok SdvStatus to indicate any error.
    pub async fn unary_call(&self, method_name: &str, request: &[u8]) -> SdvResult<Vec<u8>> {
        let mut client = self.client.clone();

        if let Err(e) = client.ready().await {
            log::error!("Inner GrpcService is not able to accept a new request");
            return Err(SdvStatus::with_cause_and_message(
                SdvStatusCode::Unavailable,
                e,
                "Inner GrpcService is not able to accept a new request",
            ));
        }

        let request_path = format!("{}{}", self.request_path_prefix, method_name);
        let request_path =
            http::uri::PathAndQuery::from_maybe_shared(request_path).expect("Valid path");
        let tonic_request =
            prepare_unary_request(request, self.fqin_str.as_str(), &self.request_timeout);
        let maybe_response: Result<Response<bytes::BytesMut>, tonic::Status> =
            client.unary(tonic_request, request_path, NoOpCodec::default()).await;

        match maybe_response {
            Ok(mut response) => {
                let response_buf = response.get_mut();
                Ok(response_buf.to_vec())
            }
            Err(e) => {
                log::error!("Could not obtain unary response: {}", e.message());
                Err(SdvStatus::with_cause_and_message(
                    SdvStatusCode::from(e.code() as i32),
                    e,
                    "Could not obtain unary response",
                ))
            }
        }
    }

    /// Executes a client-streaming RPC call asynchronously.
    ///
    /// Parameters:
    ///    [IN] client: connected SDV RPC client;
    ///    [IN] method_name: name of the remote RPC method;
    ///    [IN] request: stream of the data that should be sent to the server.
    ///
    /// Return value:
    ///   Returns RPC response bytes.
    pub async fn client_streaming_call(
        &self,
        method_name: &str,
        request: Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>,
    ) -> SdvResult<Vec<u8>> {
        let mut client = self.client.clone();

        if let Err(e) = client.ready().await {
            log::error!("Inner GrpcService is not able to accept a new request");
            return Err(SdvStatus::with_cause_and_message(
                SdvStatusCode::Unavailable,
                e,
                "Inner GrpcService is not able to accept a new request",
            ));
        }

        let request_path = format!("{}{}", self.request_path_prefix, method_name);
        let request_path =
            http::uri::PathAndQuery::from_maybe_shared(request_path).expect("Valid path");

        let tonic_request =
            prepare_streaming_request(request, self.fqin_str.as_str(), &self.request_timeout);

        // In general we may use a codec that would support encoding of the
        // Result of bytes instead of just bytes. That would allow us to report
        // streaming error to the server. But we don't do that as it may cause
        // trouble with C++/Java compatibility.
        let maybe_response: Result<Response<bytes::BytesMut>, tonic::Status> =
            client.client_streaming(tonic_request, request_path, NoOpCodec::default()).await;

        match maybe_response {
            Ok(mut response) => {
                let response_buf = response.get_mut();
                Ok(response_buf.to_vec())
            }
            Err(e) => {
                log::error!("Could not obtain client streaming response: {}", e.message());
                Err(SdvStatus::with_cause_and_message(
                    SdvStatusCode::from(e.code() as i32),
                    e,
                    "Could not obtain client streaming response",
                ))
            }
        }
    }

    /// Executes a server-streaming RPC call asynchronously.
    ///
    /// Parameters:
    ///    [IN] client: connected SDV RPC client;
    ///    [IN] method_name: name of the remote RPC method;
    ///    [IN] request: buffer where the RPC request bytes are stored
    ///
    /// Return value:
    ///   Returns a Stream of RPC responses.
    pub async fn server_streaming_call(
        &self,
        method_name: &str,
        request: &[u8],
    ) -> SdvResult<Pin<Box<dyn Stream<Item = SdvResult<Vec<u8>>> + Send>>> {
        let mut client = self.client.clone();

        if let Err(e) = client.ready().await {
            log::error!("Client is not ready for streaming call");
            return Err(SdvStatus::with_cause_and_message(
                SdvStatusCode::Unavailable,
                e,
                "Client is not ready for streaming call",
            ));
        }

        let request_path = format!("{}{}", self.request_path_prefix, method_name);
        let request_path =
            http::uri::PathAndQuery::from_maybe_shared(request_path).expect("Valid path");
        let tonic_request =
            prepare_unary_request(request, self.fqin_str.as_str(), &self.request_timeout);
        let maybe_response: Result<Response<tonic::Streaming<bytes::BytesMut>>, tonic::Status> =
            client.server_streaming(tonic_request, request_path, NoOpCodec::default()).await;

        match maybe_response {
            Ok(response) => {
                let stream = response.into_inner();
                let mapped_stream = stream.map(|item| {
                    item.map(|bytes_mut| bytes_mut.to_vec())
                        .map_err(|e| SdvStatus::new(SdvStatusCode::from(e.code() as i32)))
                });
                Ok(Box::pin(mapped_stream))
            }
            Err(e) => {
                log::error!("Could not obtain server streaming response");
                Err(SdvStatus::with_cause_and_message(
                    SdvStatusCode::from(e.code() as i32),
                    e,
                    "Could not obtain server streaming response",
                ))
            }
        }
    }
}

fn verify_server_fqin(
    expected_server_fqin: &ServiceFqin,
    ssl_stream: &tokio_openssl::SslStream<TokioTcpStream>,
) -> SdvResult<()> {
    let peer_cert = extract_cert(ssl_stream)?;

    let server_verification_passed: bool =
        verify_identity(&peer_cert, &expected_server_fqin.to_string()).map_err(|err| {
            SdvStatus::with_message(sdv_status_rs::SdvStatusCode::Internal, err.to_string())
        })?;

    if !server_verification_passed {
        let error_msg =
            format!("Actual FQIN is not equal to expected FQIN {}", *expected_server_fqin);
        return Err(SdvStatus::with_message(SdvStatusCode::Unauthenticated, error_msg));
    }

    verify_issuer_vm_matches_fqin(&peer_cert, expected_server_fqin)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_has_fqin() {
        let bytes: [u8; 3] = [1, 2, 3];
        let fqin = "this:is.string.fqin.ForHeaderTest/default";
        let tonic_request = prepare_unary_request(&bytes, fqin, &None);

        assert_eq!(
            tonic_request.metadata().clone().into_headers().get("x-sdv-fqin").unwrap(),
            fqin
        );
    }

    #[test]
    fn test_set_headers_with_timeout() {
        let mut request = tonic::Request::<()>::new(());
        let fqin = "my.fqin.string";
        let timeout_value = Some(Duration::from_secs(5));

        set_headers(&mut request, fqin, &timeout_value);

        // Check fqin header
        assert_eq!(request.metadata().get("x-sdv-fqin").unwrap().to_str().unwrap(), fqin);
        // Check timeout header
        let timeout_value = request.metadata().get("grpc-timeout").unwrap();
        assert_eq!(
            timeout_value,
            // equivalent to 5 seconds
            "5000000u"
        );
    }
}
