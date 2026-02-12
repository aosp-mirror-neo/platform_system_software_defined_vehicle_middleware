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

use sdv_status_rs::SdvResult;
use sdv_status_rs::SdvStatus;

use std::collections::HashMap;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;

use futures::stream::Stream;
use futures::stream::StreamExt;
use tokio::sync::watch;

use crate::id::ServiceFqin;
use crate::rpc::{
    Client, ClientOptions, ClientStreamingRpcMethod, RpcMethod, Server, ServerOptions,
    ServerStreamingRpcMethod, UnaryRpcMethod,
};
use crate::sd::ServerUnitMetadata;
use crate::sd::UnitMetadata;
use crate::sd::UnitRegistrationArgs;
use crate::sd::UnitType;
use crate::ContextRef;

use crate::id::UnitIdentifier;
use crate::sd::ServiceDiscoveryManager;
use crate::sd::ServiceUnitDefinition;
use crate::sd::UnitNameDiscoveryArgs;

const UNIT_NAME: &str = "serviceunitname";

async fn start_server(context: ContextRef, methods_map: HashMap<String, RpcMethod>) -> Server {
    let server_options = ServerOptions::default();
    start_server_with_options(context, methods_map, &server_options).await
}

async fn start_server_with_options(
    context: ContextRef,
    methods_map: HashMap<String, RpcMethod>,
    server_options: &ServerOptions,
) -> Server {
    let builder = UnitType::builder();
    let unit_type = builder
        .set_sdv_package_name("com.example.unit.type")
        .set_service_bundle_name("UnitTypeBundleName")
        .set_unit_type_name("Typename")
        .build()
        .expect("Failed to build Unit Type");
    let mut unit_registration_args = UnitRegistrationArgs::new();
    {
        let mut server_unit_metadata = ServerUnitMetadata::new();
        server_unit_metadata.set_version(1);
        unit_registration_args.set_unit_name(UNIT_NAME);
        unit_registration_args.set_unit_type(&unit_type);
        unit_registration_args.set_unit_metadata(&UnitMetadata::Server(server_unit_metadata));
    }

    let server = Server::create_with_generic_handlers(
        context,
        unit_registration_args,
        methods_map,
        server_options,
    )
    .await;
    assert!(server.is_ok(), "Server creation failed: {:?}", server.err());
    server.unwrap()
}

// Fetches service identity from the service discovery.
async fn get_server_identity(ctx: ContextRef) -> Option<UnitIdentifier> {
    let name_args = UnitNameDiscoveryArgs::new_builder()
        .set_sdv_package_name(ctx.get_self_fqin().get_sdv_package_name().to_string())
        .set_service_bundle_name(ctx.get_self_fqin().get_service_bundle_name().to_string())
        .set_service_unit_name(UNIT_NAME.to_string())
        .build()
        .expect("UnitNameDiscoveryArgs build() failed");

    let discovery_result: Vec<ServiceUnitDefinition> =
        ServiceDiscoveryManager::new(ctx).list_units_by_name(&name_args).collect().await;

    if discovery_result.is_empty() {
        return None;
    }

    assert_eq!(discovery_result.len(), 1);

    Some(discovery_result[0].get_unit_identifier())
}

async fn create_client(context: ContextRef) -> Client {
    let client_options = ClientOptions::default();
    create_client_with_options(context, &client_options).await
}

async fn create_client_with_options(context: ContextRef, client_options: &ClientOptions) -> Client {
    let client = create_client_with_result(context, client_options).await;
    assert!(client.is_ok(), "Client creation failed: {:?}", client.err());
    client.unwrap()
}

async fn create_client_with_result(
    context: ContextRef,
    client_options: &ClientOptions,
) -> SdvResult<Client> {
    let unit_identifier = get_server_identity(context).await;
    assert!(unit_identifier.is_some(), "Client should be able to discover a server");
    Client::create(context, unit_identifier.unwrap(), client_options).await
}

fn generate_u8_vector(size: usize) -> Vec<u8> {
    let mut gen_vector = Vec::with_capacity(size);
    for i in 0..size {
        let v = i % (u8::MAX as usize);
        gen_vector.push(v as u8);
    }
    gen_vector
}

// Default DEFAULT_MAX_RECV_MESSAGE_SIZE is 4 * 1024 * 1024 = 4194304;
// This value should be larger than the default value.
static LARGE_MESSAGE_SIZE: usize = 25_000_000;

#[tokio::test]
async fn test_simple_echo_server() {
    let fqin_str: &str = "vm-cdc:com.android.sdv.rpc.TestEcho/default";
    let fqin = ServiceFqin::from_str(fqin_str).expect("Valid ServiceFqin is expected");
    let context = ContextRef::create(fqin);

    // The VM name in the FQIN from Context may be different from what
    // was provided during Context creation. Let's update the FQIN
    // because of that.
    let fqin = context.get_self_fqin();

    let callback: UnaryRpcMethod = Arc::new(move |caller_fqin: ServiceFqin, data: &[u8]| {
        assert_eq!(fqin, caller_fqin);
        let message = String::from_utf8_lossy(data);
        let response = format!("Hello! You said: {message}");
        Box::pin(async move { Ok(response.into_bytes()) })
    });
    let method_name = "SayHello";
    let methods_map: HashMap<String, RpcMethod> =
        [(method_name.to_string(), RpcMethod::Unary(callback))].iter().cloned().collect();

    let _server = start_server(context, methods_map).await;

    let client = create_client(context).await;
    let request = b"This is a test message";
    let response = client.unary_call(method_name, request).await;
    assert!(response.is_ok(), "Unary call failed: {:?}", response.err());
    let response = response.unwrap();
    assert_eq!(response, b"Hello! You said: This is a test message");
}

#[tokio::test]
async fn test_unary_large_data() {
    let fqin_str: &str = "vm-cdc:com.android.sdv.rpc.UnaryLarge/default";
    let fqin = ServiceFqin::from_str(fqin_str).expect("Valid ServiceFqin is expected");
    let context = ContextRef::create(fqin);

    let fqin = context.get_self_fqin();

    let callback: UnaryRpcMethod = Arc::new(move |caller_fqin: ServiceFqin, data: &[u8]| {
        assert_eq!(fqin, caller_fqin);
        let response = data.to_vec();
        Box::pin(async move { Ok(response) })
    });
    let method_name = "BigData";
    let methods_map: HashMap<String, RpcMethod> =
        [(method_name.to_string(), RpcMethod::Unary(callback))].iter().cloned().collect();

    let mut server_options = ServerOptions::new();
    server_options.set_max_incoming_message_size_bytes(LARGE_MESSAGE_SIZE);
    let _server = start_server_with_options(context, methods_map, &server_options).await;

    let mut client_options = ClientOptions::new();
    client_options.set_max_incoming_message_size_bytes(LARGE_MESSAGE_SIZE);
    let client = create_client_with_options(context, &client_options).await;

    let request = generate_u8_vector(LARGE_MESSAGE_SIZE);
    let response = client.unary_call(method_name, &request).await;
    assert!(response.is_ok(), "Unary call failed: {:?}", response.err());
    let response = response.unwrap();
    assert_eq!(response, request);
}

#[tokio::test]
async fn test_unary_timeout() {
    let fqin_str: &str = "vm-cdc:com.android.sdv.rpc.UnaryTimeout/default";
    let fqin = ServiceFqin::from_str(fqin_str).expect("Valid ServiceFqin is expected");
    let context = ContextRef::create(fqin);

    let fqin = context.get_self_fqin();

    let callback: UnaryRpcMethod = Arc::new(move |caller_fqin: ServiceFqin, _data: &[u8]| {
        assert_eq!(fqin, caller_fqin);
        Box::pin(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            Ok(b"This should be ignored by client due to timeout.".to_vec())
        })
    });
    let method_name = "SlowMethod";
    let methods_map: HashMap<String, RpcMethod> =
        [(method_name.to_string(), RpcMethod::Unary(callback))].iter().cloned().collect();

    let server_options = ServerOptions::new();
    let _server = start_server_with_options(context, methods_map, &server_options).await;

    let mut client_options = ClientOptions::new();
    client_options.set_timeout(std::time::Duration::from_millis(5));
    let client = create_client_with_options(context, &client_options).await;

    let request = b"Unused request data";
    let response = client.unary_call(method_name, request).await;
    assert!(response.is_err(), "Unary call should fail with timeout");
    // Tonic returns "Cancelled" on timeout:
    // http://ac/external/rust/android-crates-io/crates/tonic/src/status.rs?l=618
    assert_eq!(response.unwrap_err().status_code(), sdv_status_rs::SdvStatusCode::Cancelled);
}

#[tokio::test]
async fn test_streaming_echo_server() {
    let fqin = ServiceFqin::from_str("fm-cdc:com.android.sdv.rpc.TestStreamingEchoServer/default")
        .expect("Valid ServiceFqin is expected");
    let context = ContextRef::create(fqin);
    let fqin = context.get_self_fqin(); // VM name might have changed

    let handler: ServerStreamingRpcMethod =
        Arc::new(move |caller_fqin: ServiceFqin, req: &[u8]| {
            assert_eq!(fqin, caller_fqin);
            let message = String::from_utf8_lossy(req);
            let single_response = format!("Hello! You said: {message}");
            let response = tokio_stream::iter(
                std::iter::repeat_with(move || Ok(single_response.clone().into_bytes())).take(5),
            );
            Box::pin(async move {
                Ok(Box::pin(response)
                    as Pin<Box<dyn Stream<Item = Result<Vec<u8>, SdvStatus>> + Send>>)
            })
        });
    let method_name = "StreamHello";
    let methods_map =
        HashMap::from([(method_name.to_string(), RpcMethod::ServerStreaming(handler))]);
    let _server = start_server(context, methods_map).await;
    let client = create_client(context).await;

    let request = b"This is a test message";
    let response = client.server_streaming_call(method_name, request).await;
    assert!(response.is_ok(), "Server streaming call failed: {:?}", response.err());
    let mut response = response.unwrap();
    let mut num_items = 0;
    while let Some(item) = response.next().await {
        assert!(item.is_ok(), "Item #{} of the stream is an error: {:?}", num_items, item.err());
        assert_eq!(item.unwrap(), b"Hello! You said: This is a test message");
        num_items += 1;
    }
    assert_eq!(num_items, 5);
}

#[derive(Default)]
struct LargeDataStreamServer {
    counter: u8,
}

impl Stream for LargeDataStreamServer {
    type Item = SdvResult<Vec<u8>>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        _cx: &mut futures::task::Context<'_>,
    ) -> futures::task::Poll<Option<Self::Item>> {
        if self.counter == 10 {
            return futures::task::Poll::Ready(None);
        }

        self.counter += 1;

        let val = Some(Ok(generate_u8_vector(LARGE_MESSAGE_SIZE)));
        futures::task::Poll::Ready(val)
    }
}

#[tokio::test]
async fn test_server_streaming_large_data() {
    let fqin = ServiceFqin::from_str("fm-cdc:com.android.sdv.rpc.TestServerStreamingLarge/default")
        .expect("Valid ServiceFqin is expected");
    let context = ContextRef::create(fqin);
    let fqin = context.get_self_fqin(); // VM name might have changed

    let handler: ServerStreamingRpcMethod =
        Arc::new(move |caller_fqin: ServiceFqin, req: &[u8]| {
            assert_eq!(fqin, caller_fqin);
            assert_eq!(req.len(), 0); // Check that empty request is possible.
            Box::pin(async move {
                Ok(Box::pin(LargeDataStreamServer::default())
                    as Pin<Box<dyn Stream<Item = Result<Vec<u8>, SdvStatus>> + Send>>)
            })
        });
    let method_name = "StreamLargeData";
    let methods_map =
        HashMap::from([(method_name.to_string(), RpcMethod::ServerStreaming(handler))]);
    let _server = start_server(context, methods_map).await;
    let mut client_options = ClientOptions::new();
    client_options.set_max_incoming_message_size_bytes(LARGE_MESSAGE_SIZE);
    let client = create_client_with_options(context, &client_options).await;

    let request = [];
    let response = client.server_streaming_call(method_name, &request).await;

    assert!(response.is_ok(), "Server streaming call failed: {:?}", response.err());
    let mut response = response.unwrap();
    while let Some(item) = response.next().await {
        assert!(item.is_ok(), "Item of the stream is an error: {:?}", item.err());
        assert_eq!(item.unwrap().len(), LARGE_MESSAGE_SIZE);
    }
}

#[tokio::test]
async fn test_client_streaming() {
    let fqin = ServiceFqin::from_str("fm-cdc:com.android.sdv.rpc.TestClientStreaming/default")
        .expect("Valid ServiceFqin is expected");
    let context = ContextRef::create(fqin);
    let fqin = context.get_self_fqin(); // VM name might have changed

    let callback: ClientStreamingRpcMethod =
        Arc::new(move |caller_fqin: ServiceFqin, request: tonic::Streaming<bytes::BytesMut>| {
            assert_eq!(fqin, caller_fqin);
            let mut request = request;

            Box::pin(async move {
                let mut counter: u8 = 0;

                while let Result::Ok(Some(next_message)) = request.message().await {
                    assert_eq!("Hello!", next_message);
                    counter += 1;
                }

                let response: Vec<u8> = vec![counter];
                Ok(response)
            })
        });
    let method_name = "CountHello";
    let methods_map: HashMap<String, RpcMethod> =
        [(method_name.to_string(), RpcMethod::ClientStreaming(callback))].iter().cloned().collect();

    let _server = start_server(context, methods_map).await;
    let client = create_client(context).await;

    let n_msg: usize = 5;
    let client_stream_request = {
        let single_message = "Hello!".as_bytes().to_vec();
        let messages =
            tokio_stream::iter(std::iter::repeat_with(move || single_message.clone()).take(n_msg));

        Box::pin(messages) as Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>
    };

    let response = client.client_streaming_call("CountHello", client_stream_request).await;

    assert!(response.is_ok());
    let response = response.unwrap();
    assert_eq!(usize::from(response[0]), n_msg);
}

#[derive(Default)]
struct LargeDataStreamClient {
    counter: u8,
}

impl Stream for LargeDataStreamClient {
    type Item = Vec<u8>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        _cx: &mut futures::task::Context<'_>,
    ) -> futures::task::Poll<Option<Self::Item>> {
        if self.counter == 10 {
            return futures::task::Poll::Ready(None);
        }

        self.counter += 1;

        let val = Some(generate_u8_vector(LARGE_MESSAGE_SIZE));
        futures::task::Poll::Ready(val)
    }
}

#[tokio::test]
async fn test_client_streaming_large_data() {
    let fqin = ServiceFqin::from_str("fm-cdc:com.android.sdv.rpc.TestClinetStreamingLarge/default")
        .expect("Valid ServiceFqin is expected");
    let context = ContextRef::create(fqin);
    let fqin = context.get_self_fqin(); // VM name might have changed

    let callback: ClientStreamingRpcMethod = Arc::new(
        move |caller_fqin: ServiceFqin, mut request: tonic::Streaming<bytes::BytesMut>| {
            assert_eq!(fqin, caller_fqin);
            Box::pin(async move {
                loop {
                    let msg = request.message().await;
                    let payload = match msg {
                        Result::Ok(payload) => payload,
                        Result::Err(err) => {
                            panic!("We don't expect streaming error in this test, but got this error: {err}");
                        }
                    };

                    match payload {
                        Some(payload) => {
                            assert_eq!(payload.len(), LARGE_MESSAGE_SIZE);
                        }
                        None => {
                            break;
                        }
                    }
                }

                Ok(vec![])
            })
        },
    );

    let method_name = "LargeDataCheck";
    let methods_map: HashMap<String, RpcMethod> =
        [(method_name.to_string(), RpcMethod::ClientStreaming(callback))].iter().cloned().collect();

    let mut server_options = ServerOptions::new();
    server_options.set_max_incoming_message_size_bytes(LARGE_MESSAGE_SIZE);
    let _server = start_server_with_options(context, methods_map, &server_options).await;

    let client = create_client(context).await;
    let client_stream_request =
        Box::pin(LargeDataStreamClient::default()) as Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>;

    let response = client.client_streaming_call(method_name, client_stream_request).await;

    assert!(response.is_ok());
    let response = response.unwrap();
    assert_eq!(response.len(), 0); // Verify that zero length response is possible
}

#[tokio::test]
/// This test ensures concurrent RPC call handling on the server.
///
/// It simulates a scenario where one RPC method ("WaitAndRespond") deliberately
/// delays its response, mimicking a real-world scenario like waiting for I/O.
/// The test verifies that the server can still process a fast call from
/// another client ("ImmediateRespond") while the first call is waiting.
async fn test_concurrent_calls_and_waiting() {
    let fqin_str: &str = "vm-cdc:com.android.sdv.rpc.TestConcurrent/default";
    let fqin = ServiceFqin::from_str(fqin_str).expect("Valid ServiceFqin");

    let (tx, rx) = watch::channel(());

    let slow_method: UnaryRpcMethod = Arc::new(move |_fqin: ServiceFqin, _data: &[u8]| {
        let mut rx = rx.clone();
        Box::pin(async move {
            rx.changed().await.unwrap(); // Wait for the signal.
            Ok(b"Waited and responded!".to_vec())
        })
    });

    let fast_method: UnaryRpcMethod = Arc::new(move |_fqin: ServiceFqin, data: &[u8]| {
        let message = String::from_utf8_lossy(data);
        let response = format!("Hello! You said: {message}");
        Box::pin(async move { Ok(response.into_bytes()) })
    });

    let methods_map: HashMap<String, RpcMethod> = [
        ("WaitAndRespond".to_string(), RpcMethod::Unary(slow_method)),
        ("ImmediateRespond".to_string(), RpcMethod::Unary(fast_method)),
    ]
    .iter()
    .cloned()
    .collect();

    let context = ContextRef::create(fqin);

    let _server = start_server(context, methods_map).await;

    let client1 = create_client(context).await;
    let client2 = create_client(context).await;
    let request1 = b"First message";
    let request2 = b"Second message";
    // Spawn a task to make the first call (will wait).
    let slow_task =
        tokio::spawn(async move { client1.unary_call("WaitAndRespond", request1).await });

    // Spawn a task to make the second call (immediate response).
    let fast_task =
        tokio::spawn(async move { client2.unary_call("ImmediateRespond", request2).await });

    // Check that the second call already finished, even though the first
    // one is still in process.
    let response2 = fast_task.await.unwrap();
    assert!(response2.is_ok(), "Fast call returned with an error: {:?}", response2.err());
    let response2 = response2.unwrap();
    assert_eq!(response2, b"Hello! You said: Second message");
    assert!(!slow_task.is_finished());

    // Signal the first task to proceed
    tx.send(()).unwrap();
    let response1 = slow_task.await.unwrap();
    assert!(response1.is_ok(), "Slow call returned with an error: {:?}", response1.err());
    let response1 = response1.unwrap();
    assert_eq!(response1, b"Waited and responded!");
}

#[tokio::test]
/// This test verifies the behavior of the server after it is destroyed.
/// It ensures that:
/// - New client connections are rejected.
/// - New requests from existing clients are rejected.
/// - In-progress requests are allowed to complete.
async fn test_server_destruction() {
    let fqin_str: &str = "vm-cdc:com.android.sdv.rpc.TestDestruction/default";
    let fqin = ServiceFqin::from_str(fqin_str).expect("Valid ServiceFqin");

    let simple_method: UnaryRpcMethod = Arc::new(move |_fqin: ServiceFqin, _data: &[u8]| {
        Box::pin(async move { Ok(b"Simple response!".to_vec()) })
    });
    let (tx, rx) = watch::channel(());
    let slow_method: UnaryRpcMethod = Arc::new(move |_fqin: ServiceFqin, _data: &[u8]| {
        let mut rx = rx.clone();
        Box::pin(async move {
            rx.changed().await.unwrap(); // Wait for the signal.
            Ok(b"Slow response!".to_vec())
        })
    });
    let methods_map = HashMap::from([
        ("SimpleMethod".to_string(), RpcMethod::Unary(simple_method)),
        ("SlowMethod".to_string(), RpcMethod::Unary(slow_method)),
    ]);

    let context = ContextRef::create(fqin);

    let server = start_server(context, methods_map).await;

    let client1 = create_client(context).await;
    let client2 = create_client(context).await;
    let unit_identifier_cached = get_server_identity(context).await.unwrap();

    // Initiate a long-running request on client1.
    let long_task =
        tokio::spawn(async move { client1.unary_call("SlowMethod", b"Slow request").await });

    drop(server);
    // Unfortunately drop() just initiates the distruction of server. There are
    // no guarantees that server stops accepting new connections on drop() return.
    // So we have to wait for a while.
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Server must be removed from service discovery
    assert!(get_server_identity(context).await.is_none());

    // Attempt a new connection using cached unit identifier should fail.
    assert!(
        Client::create(context, unit_identifier_cached, &ClientOptions::default()).await.is_err(),
        "New connection should fail after destruction"
    );

    // Attempt a new request from an existing client should also fail.
    let new_request_result = client2.unary_call("SimpleMethod", b"New request").await;
    assert!(new_request_result.is_err(), "New requests should fail after destruction");

    // Allow the in-progress SlowMethod request to finish.
    tx.send(()).unwrap();
    let slow_response = long_task.await.unwrap();
    assert!(
        slow_response.is_ok(),
        "In-progress request should complete, got an error instead: {:?}",
        slow_response.err()
    );
    let slow_response = slow_response.unwrap();
    assert_eq!(slow_response, b"Slow response!");
}

#[tokio::test]
// Tests that the client can connect as soon as the server is discoverable through SD.
// Connections should not be rejected on server startup.
async fn test_client_early_creation() {
    let fqin_str: &str = "vm-cdc:com.android.sdv.rpc.TestEarlyClient/default";
    let fqin = ServiceFqin::from_str(fqin_str).expect("Valid ServiceFqin is expected");
    let context = Arc::new(ContextRef::create(fqin));

    let mut handles = Vec::new();
    // Spawn client tasks waiting for server to be discoverable and connecting immediately.
    // 50 is the size of listening queue set by the RPC agent.
    for _ in 0..50 {
        let context_clone = context.clone();
        handles.push(tokio::spawn(async move {
            // Wait until server is registered.
            let unit_identifier = loop {
                let id = get_server_identity(*context_clone).await;
                if id.is_some() {
                    break id;
                }
            };
            let result =
                Client::create(*context_clone, unit_identifier.unwrap(), &ClientOptions::default())
                    .await;
            assert!(result.is_ok(), "{}", result.err().unwrap().to_string());
        }));
    }

    let methods_map = HashMap::<String, RpcMethod>::new();
    let _server = start_server(*context, methods_map).await;
    for handle in handles {
        handle.await.unwrap();
    }
}
