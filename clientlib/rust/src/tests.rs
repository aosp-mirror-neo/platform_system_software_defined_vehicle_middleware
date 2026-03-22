/*
 * Copyright (C) 2026 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use crate::*;
use async_trait::async_trait;
use futures::Stream;
use sdv_mw_commstests_protos::test_messages::Pressure;
use sdv_mw_local_comms::LocalComms;
use sdv_mw_rs_comms::{
    Availability, ClientOptions, ClientTransport, MockCommunicate, RpcClient, SubscribeOptions,
};
use sdv_status_rs::SdvResult;
use std::pin::Pin;

#[tokio::test]
async fn test_hello_world_pubsub() {
    let comms = LocalComms::new();

    // GIVEN Pub/Sub Descriptors that require only a message type and topic name.
    let pub_descriptor = PublisherDescriptorFull::<Pressure>::new("hello-world-pubsub");
    let sub_descriptor = SubscriberDescriptorFull::<Pressure>::new("hello-world-pubsub");

    // WHEN creating a Publisher and Subscriber from it.
    let publisher =
        create_publisher(&comms, &pub_descriptor).await.expect("Failed to create publisher");
    let subscriber = create_subscriber(&comms, &sub_descriptor, &SubscribeOptions::default())
        .await
        .expect("Failed to create subscriber");

    // THEN we can read a published message.
    let mut msg = Pressure::new();
    msg.pressure = 42;
    publisher.publish(&msg).expect("Failed to publish");

    let received = subscriber.read_next_messages(1).expect("Failed to read");
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].pressure, 42);
}

#[tokio::test]
async fn test_hello_world_pubsub_custom_attributes() {
    let comms = LocalComms::new();

    let pub_descriptor = PublisherDescriptorFull::<Pressure>::new("hello-world-pubsub-custom")
        .with_service_unit_name("hello-world-pubsub-custom-service-unit-name")
        .with_capacity(10);
    let sub_descriptor = SubscriberDescriptorFull::<Pressure>::new("hello-world-pubsub-custom");

    let publisher =
        create_publisher(&comms, &pub_descriptor).await.expect("Failed to create publisher");

    let subscriber = create_subscriber(&comms, &sub_descriptor, &SubscribeOptions::default())
        .await
        .expect("Failed to create subscriber");

    let mut msg = Pressure::new();
    msg.pressure = 42;
    publisher.publish(&msg).expect("Failed to publish");

    let received = subscriber.read_next_messages(1).expect("Failed to read");
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].pressure, 42);
}

#[tokio::test]
async fn test_generated_descriptors_pubsub() {
    let comms = LocalComms::new();

    // GIVEN Pub/Sub descriptors as used in the generated code.
    let pub_descriptor = PublisherDescriptor::<Pressure>::new("gen-pub-sub")
        .with_message("sdv.sample.Pressure")
        .with_max_message_size(1024)
        .with_capacity(10)
        .with_service_unit_name("gen-pub-sub-service-unit-name");
    let sub_descriptor =
        SubscriberDescriptor::<Pressure>::new("gen-pub-sub").with_message("sdv.sample.Pressure");

    // Test create_publisher
    let publisher =
        create_publisher(&comms, &pub_descriptor).await.expect("Failed to create publisher");

    // Test create_subscriber
    let options = SubscribeOptions::default();
    let subscriber = create_subscriber(&comms, &sub_descriptor, &options)
        .await
        .expect("Failed to create subscriber");

    // Verify end-to-local-fake communication
    let mut msg = Pressure::new();
    msg.pressure = 42;
    publisher.publish(&msg).expect("Failed to publish");

    let received = subscriber.read_next_messages(1).expect("Failed to read");
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].pressure, 42);
}

struct ClientTransportStub;

#[async_trait]
impl ClientTransport for ClientTransportStub {
    async fn call_method(&self, _method: &str, _request: &[u8]) -> SdvResult<Vec<u8>> {
        Ok(vec![])
    }
    async fn call_client_streaming_method(
        &self,
        _method_name: &str,
        _request_stream: Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>,
    ) -> SdvResult<Vec<u8>> {
        Ok(vec![])
    }
    async fn call_server_streaming_method(
        &self,
        _method_name: &str,
        _request: &[u8],
    ) -> SdvResult<Pin<Box<dyn Stream<Item = SdvResult<Vec<u8>>> + Send>>> {
        unimplemented!()
    }
}

struct RpcClientStub {
    _transport: Box<dyn ClientTransport>,
}

impl RpcClient for RpcClientStub {
    const DEFAULT_UNIT_NAME: &'static str = "mock-rpc-stub";
    fn new(transport: Box<dyn ClientTransport>) -> Self {
        Self { _transport: transport }
    }
}

#[tokio::test]
async fn test_create_rpc_client() {
    let mut mock_comms = MockCommunicate::new();

    let descriptor = ClientDescriptor::<RpcClientStub>::new("main-channel")
        .with_service("sdv.sample.TireService");

    mock_comms
        .expect_create_client_transport_for_channel()
        .withf(|type_name, channel, _options| {
            type_name.get_package_name() == "sdv.sample"
                && type_name.get_type_name() == "TireService"
                && channel == "main-channel"
        })
        .times(1)
        .returning(|_, _, _| {
            Box::pin(async move { Ok(Box::new(ClientTransportStub) as Box<dyn ClientTransport>) })
        });

    let _client = create_rpc_client(&mock_comms, &descriptor, ClientOptions::default())
        .await
        .expect("Failed to create RPC client");
}

#[tokio::test]
async fn test_create_registration_event_stream() {
    let comms = LocalComms::new();
    let pub_descriptor = PublisherDescriptor::<Pressure>::new("pressure-topic")
        .with_message("sdv.sample.Pressure")
        .with_service_unit_name("tire-service");
    let sub_descriptor =
        SubscriberDescriptor::<Pressure>::new("pressure-topic").with_message("sdv.sample.Pressure");

    use futures::StreamExt;
    let mut events = create_registration_event_stream(&comms, &sub_descriptor).await.unwrap();

    // Initial state should be Unavailable in LocalComms if no publisher exists
    assert_eq!(events.next().await, Some(Availability::Unavailable));

    let _publisher = create_publisher(&comms, &pub_descriptor).await.unwrap();
    assert_eq!(events.next().await, Some(Availability::Available));
}
