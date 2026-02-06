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
use crate::{
    pubsub_impl::SdvPublication,
    {SdvClientTransport, SdvPublisher, SdvServerTransport, SdvSubscriber},
};
use async_trait::async_trait;
use futures::{FutureExt, Stream, StreamExt};
use protobuf::Message;
use sdv_comms::{
    dt::{
        Publisher as DtPublisher, Subscriber as DtSubscriber,
        SubscriberOptions as DtSubscriberOptions, SubscriberStream as DtStream,
    },
    rpc::{Client, ClientOptions, RpcMethod, Server, ServerOptions, UnaryRpcMethod},
    sd::{
        PublisherUnitMetadata, ServiceDiscoveryManager, ServiceUnitChangeEvent,
        ServiceUnitDefinition, UnitMetadata, UnitNameDiscoveryArgs, UnitRegistrationArgs, UnitType,
    },
    ContextRef,
};
use sdv_mw_rs_comms::{
    Availability, ByteBuffersStream, ClientAttributes, ClientTransport, Communicate, Publication,
    Publish, PublisherAttributes, ServerAttributes, ServerTransport, Subscribe, SubscribeOptions,
    TypeName, UnitName, UnitQuery,
};
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::time::Duration;
use std::{collections::HashMap, pin::Pin, sync::Arc};

static_assertions::assert_impl_all!(SdvComms: Send);
static_assertions::assert_impl_all!(SdvComms: Sync);
/// Middleware adapter for sdv_comms
#[derive(Clone)]
pub struct SdvComms {
    /// ContextRef passed in by Lifecycle Manager
    pub context: ContextRef,
}

#[async_trait]
impl Communicate for SdvComms {
    async fn create_publish(
        &self,
        attributes: &PublisherAttributes,
    ) -> SdvResult<Box<dyn Publish>> {
        let unit_type = UnitType::builder()
            .set_sdv_package_name(attributes.get_type_name().get_package_name())
            .set_service_bundle_name(attributes.get_type_name().get_bundle_name())
            .set_unit_type_name(attributes.get_type_name().get_type_name())
            .build()?;

        let mut metadata = PublisherUnitMetadata::new();
        metadata.set_message_size(
            sdv_vsidl_common::proto_size_utils::compute_message_length_including_header(
                attributes.get_metadata().get_message_size(),
            ),
        );
        metadata.set_message_count(attributes.get_metadata().get_message_count());
        metadata.set_topic(attributes.get_topic_name());

        let mut unit_registration_args = UnitRegistrationArgs::new();
        unit_registration_args.set_unit_type(&unit_type);
        unit_registration_args.set_unit_name(attributes.get_publication_name());
        unit_registration_args.set_unit_metadata(&UnitMetadata::Publisher(metadata));

        if attributes.get_metadata().is_message_metadata_required() {
            let application_metadata =
                match attributes.get_metadata().get_message_metadata().write_to_bytes() {
                    Err(_) => return Err(SdvStatus::new(SdvStatusCode::InvalidArgument)),
                    Ok(bytes) => bytes,
                };
            unit_registration_args.set_application_metadata(&application_metadata);
        }

        let dt_publisher = DtPublisher::create(self.context, unit_registration_args).await?;
        Ok(Box::new(SdvPublisher::new(dt_publisher)))
    }

    #[allow(useless_deprecated)]
    #[allow(unused_attributes)]
    #[deprecated(
        since = "2.0.0",
        note = "Topic-based subscribe methods are called instead, e.g. `create_subscribe_handle`"
    )]
    async fn find_publication(&self, attributes: &UnitQuery) -> SdvResult<Arc<dyn Publication>> {
        let sd = ServiceDiscoveryManager::new(self.context);
        let service_unit = match attributes {
            UnitQuery::Name(unit_name) => self
                .find_first_service_unit_by_name(&sd, unit_name)
                .await
                .ok_or(SdvStatus::with_message(
                    SdvStatusCode::Unavailable,
                    format!("Cannot find unit by name {unit_name:?}"),
                ))?,
            UnitQuery::Type(type_name) => {
                let unit_type = UnitType::builder()
                    .set_sdv_package_name(type_name.get_package_name())
                    .set_service_bundle_name(type_name.get_bundle_name())
                    .set_unit_type_name(type_name.get_type_name())
                    .build()?;
                sd.list_units_by_type(&unit_type).next().await.ok_or(SdvStatus::with_message(
                    SdvStatusCode::Unavailable,
                    format!("Cannot find unit by type {type_name:?}"),
                ))?
            }
            _ => {
                return Err(SdvStatus::with_message(
                    SdvStatusCode::Unimplemented,
                    "UnitQuery variant not handled.",
                ))
            }
        };
        Ok(Arc::new(SdvPublication::new(self.context, service_unit)))
    }

    #[allow(useless_deprecated)]
    #[allow(unused_attributes)]
    #[deprecated(
        since = "2.0.0",
        note = "Topic-based subscribe methods are called instead, e.g. `create_subscribe_handle`"
    )]
    async fn subscribe_by_type(
        &self,
        type_name: &TypeName,
        options: &SubscribeOptions,
    ) -> SdvResult<Box<dyn Subscribe>> {
        let unit_type = UnitType::builder()
            .set_sdv_package_name(type_name.get_package_name())
            .set_service_bundle_name(type_name.get_bundle_name())
            .set_unit_type_name(type_name.get_type_name())
            .build()?;
        let dt_subscriber = DtSubscriber::create_for_multipub(
            self.context,
            &unit_type,
            DtSubscriberOptions { fetch_last_message: options.read_on_subscribe() },
        )
        .await?;
        Ok(Box::new(SdvSubscriber::new(dt_subscriber)))
    }

    async fn create_subscribe_handle(
        &self,
        type_name: &TypeName,
        topic_name: &str,
        options: &SubscribeOptions,
    ) -> SdvResult<Box<dyn Subscribe>> {
        let unit_type = UnitType::builder()
            .set_sdv_package_name(type_name.get_package_name())
            .set_unit_type_name(type_name.get_type_name())
            .build()?;
        let dt_subscriber = DtSubscriber::create_for_topic(
            self.context,
            &unit_type,
            topic_name,
            DtSubscriberOptions { fetch_last_message: options.read_on_subscribe() },
        )
        .await?;
        Ok(Box::new(SdvSubscriber::new(dt_subscriber)))
    }

    async fn observe_by_type(
        &self,
        type_name: &TypeName,
        options: &SubscribeOptions,
    ) -> SdvResult<ByteBuffersStream> {
        let unit_type = UnitType::builder()
            .set_sdv_package_name(type_name.get_package_name())
            .set_service_bundle_name(type_name.get_bundle_name())
            .set_unit_type_name(type_name.get_type_name())
            .build()?;
        let dt_stream = DtStream::create_for_multipub(
            self.context,
            &unit_type,
            options.get_min_interval(),
            DtSubscriberOptions { fetch_last_message: options.read_on_subscribe() },
        )
        .await?;
        Ok(Box::pin(dt_stream))
    }

    async fn create_message_stream(
        &self,
        type_name: &TypeName,
        topic_name: &str,
        options: &SubscribeOptions,
    ) -> SdvResult<ByteBuffersStream> {
        let unit_type = UnitType::builder()
            .set_sdv_package_name(type_name.get_package_name())
            .set_unit_type_name(type_name.get_type_name())
            .build()?;
        let dt_stream = DtStream::create_for_topic(
            self.context,
            &unit_type,
            topic_name,
            options.get_min_interval(),
            DtSubscriberOptions { fetch_last_message: options.read_on_subscribe() },
        )
        .await?;
        Ok(Box::pin(dt_stream))
    }

    async fn create_history_handle(
        &self,
        type_name: &TypeName,
        topic_name: &str,
        options: &SubscribeOptions,
    ) -> SdvResult<Box<dyn sdv_mw_rs_comms::ReadHistory>> {
        let unit_type = UnitType::builder()
            .set_sdv_package_name(type_name.get_package_name())
            .set_unit_type_name(type_name.get_type_name())
            .build()?;
        let dt_history = sdv_comms::dt::HistoryReader::create_for_topic(
            self.context,
            &unit_type,
            topic_name,
            options.get_message_count(),
        )
        .await?;
        Ok(Box::new(crate::pubsub_impl::SdvHistory::new(dt_history)))
    }

    async fn create_client_transport(
        &self,
        attributes: &ClientAttributes,
    ) -> SdvResult<Box<dyn ClientTransport>> {
        let sd = ServiceDiscoveryManager::new(self.context);
        let service_unit_id = self
            .find_first_service_unit_by_name(&sd, attributes.get_server_name())
            .await
            .ok_or(SdvStatus::with_message(
                SdvStatusCode::Unavailable,
                "Cannot find unit for client transport.",
            ))?
            .get_unit_identifier();

        let client_options = attributes.get_client_options();
        let mut comms_client_options = ClientOptions::default();

        let timeout_in_sec = client_options.get_timeout_in_sec();
        if timeout_in_sec > 0 {
            comms_client_options.set_timeout(Duration::from_secs(timeout_in_sec.into()));
        }

        if let Some(max_response_size) = client_options.get_max_response_size() {
            comms_client_options.set_max_incoming_message_size_bytes(max_response_size);
        }

        let sdv_client =
            Client::create(self.context, service_unit_id, &comms_client_options).await?;

        Ok(Box::new(SdvClientTransport::new(sdv_client)))
    }

    #[allow(useless_deprecated)]
    #[allow(unused_attributes)]
    #[deprecated(
        since = "1.1.0",
        note = "please use `create_server_transport_all_methods` instead"
    )]
    async fn create_server_transport(
        &self,
        attributes: &ServerAttributes,
        unary_methods_map: HashMap<String, UnaryRpcMethod>,
    ) -> SdvResult<Box<dyn ServerTransport>> {
        let methods_map =
            unary_methods_map.into_iter().map(|(k, v)| (k, RpcMethod::Unary(v))).collect();
        self.create_server_transport_all_methods(attributes, methods_map).await
    }

    async fn create_server_transport_all_methods(
        &self,
        attributes: &ServerAttributes,
        method_map: HashMap<String, RpcMethod>,
    ) -> SdvResult<Box<dyn ServerTransport>> {
        let unit_type = UnitType::builder()
            .set_sdv_package_name(attributes.get_type_name().get_package_name())
            .set_service_bundle_name(attributes.get_type_name().get_bundle_name())
            .set_unit_type_name(attributes.get_type_name().get_type_name())
            .build()?;

        let mut unit_registration_args = UnitRegistrationArgs::new();
        unit_registration_args.set_unit_type(&unit_type);
        unit_registration_args.set_unit_name(attributes.get_server_name());

        let mw_server_options = attributes.get_server_options();
        let mut comms_server_options = ServerOptions::default();
        if let Some(max_request_size) = mw_server_options.get_max_request_size() {
            comms_server_options.set_max_incoming_message_size_bytes(max_request_size);
        }

        let sdv_server = Server::create_with_generic_handlers(
            self.context,
            unit_registration_args,
            method_map,
            &comms_server_options,
        )
        .await?;

        Ok(Box::new(SdvServerTransport::new(sdv_server)))
    }

    async fn create_registration_event_stream(
        &self,
        attributes: &UnitQuery,
    ) -> SdvResult<Pin<Box<dyn Stream<Item = Availability> + Send>>> {
        let sd = ServiceDiscoveryManager::new(self.context);

        let event_stream: Pin<Box<dyn Stream<Item = Availability> + Send>> = match attributes {
            UnitQuery::Name(unit_name) => {
                let mut discovery_args_builder = UnitNameDiscoveryArgs::new_builder()
                    .set_sdv_package_name(unit_name.get_package_name())
                    .set_service_bundle_name(unit_name.get_bundle_name())
                    .set_service_unit_name(unit_name.get_service_unit_name());
                let vm = unit_name.get_vm_name();
                if !vm.is_empty() {
                    discovery_args_builder = discovery_args_builder.set_sdv_vm_name(vm);
                }
                let discovery_args = discovery_args_builder.build()?;
                let instance_name = unit_name.get_instance_name().to_string();

                let stream = sd.subscribe_service_unit_change_by_name(&discovery_args).await?;
                let filtered_stream = stream.filter_map(move |event: ServiceUnitChangeEvent| {
                    let instance_name = instance_name.clone();
                    async move {
                        match event {
                            ServiceUnitChangeEvent::Registered(service_unit) => {
                                if instance_name.is_empty()
                                    || instance_name
                                        == service_unit
                                            .get_service_bundle_identity()
                                            .get_fqin()
                                            .get_service_instance_name()
                                {
                                    Some(Availability::Available)
                                } else {
                                    None
                                }
                            }
                            ServiceUnitChangeEvent::Unregistered(service_unit) => {
                                if instance_name.is_empty()
                                    || instance_name
                                        == service_unit
                                            .get_service_bundle_identity()
                                            .get_fqin()
                                            .get_service_instance_name()
                                {
                                    Some(Availability::Unavailable)
                                } else {
                                    None
                                }
                            }
                            ServiceUnitChangeEvent::Changed(_service_unit) => None,
                        }
                    }
                    .boxed()
                });

                Box::pin(filtered_stream)
            }
            UnitQuery::Type(type_name) => {
                let unit_type = UnitType::builder()
                    .set_sdv_package_name(type_name.get_package_name())
                    .set_service_bundle_name(type_name.get_bundle_name())
                    .set_unit_type_name(type_name.get_type_name())
                    .build()?;

                let stream = sd.subscribe_service_unit_change_by_type(&unit_type).await?;
                let filtered_stream = stream.filter_map(|event: ServiceUnitChangeEvent| {
                    async move {
                        match event {
                            ServiceUnitChangeEvent::Registered(_) => Some(Availability::Available),
                            ServiceUnitChangeEvent::Unregistered(_) => {
                                Some(Availability::Unavailable)
                            }
                            ServiceUnitChangeEvent::Changed(_) => None,
                        }
                    }
                    .boxed()
                });
                Box::pin(filtered_stream)
            }
            _ => {
                return Err(SdvStatus::with_message(
                    SdvStatusCode::Unimplemented,
                    "UnitQuery variant not handled.",
                ))
            }
        };

        Ok(event_stream)
    }
}

impl SdvComms {
    /// returns the first ServiceUnitDefinition provided by ServiceDiscoveryManager
    /// matching both the discovery_args and the instance_name.
    /// if instance_name is empty, then the first ServiceUnitDefinition is returned.
    async fn find_first_service_unit_by_name(
        &self,
        sd: &ServiceDiscoveryManager,
        unit_name: &UnitName,
    ) -> Option<ServiceUnitDefinition> {
        let mut discovery_args_builder = UnitNameDiscoveryArgs::new_builder()
            .set_sdv_package_name(unit_name.get_package_name())
            .set_service_bundle_name(unit_name.get_bundle_name())
            .set_service_unit_name(unit_name.get_service_unit_name());
        let vm = unit_name.get_vm_name();
        if !vm.is_empty() {
            discovery_args_builder = discovery_args_builder.set_sdv_vm_name(vm);
        }
        let discovery_args = discovery_args_builder.build().ok()?;
        let instance_name = unit_name.get_instance_name().to_string();
        sd.list_units_by_name(&discovery_args)
            .filter(|service_unit| {
                let instance_name = instance_name.clone();
                futures::future::ready(
                    instance_name.is_empty()
                        || service_unit
                            .get_service_bundle_identity()
                            .get_fqin()
                            .get_service_instance_name()
                            == instance_name,
                )
            })
            .next()
            .await
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use protobuf::well_known_types::empty::Empty;
    use sdv_comms::id::ServiceFqin;
    use sdv_mw_commstests_protos::test_messages::{Payload, Pressure, VariableString};
    use sdv_mw_rs_comms::{
        call_client_streaming_method, call_method, call_server_streaming_method, observe,
        ClientAttributes, ClientOptions, History, InstantReader, MethodMapBuilder, Publisher,
        PublisherMetadata, ServerAttributes, ServerOptions, SubscribeOptions, Subscriber, TypeName,
        UnitName,
    };
    use std::str::FromStr;
    use std::thread;

    fn assert_send<T: Send>(_t: &T) {}
    fn assert_sync<T: Sync>(_t: &T) {}

    const PACKAGE: &str = "sdv.package";
    const VM: &str = "vm-name";
    const OWNING_BUNDLE_NAME: &str = "PressureBundle";
    const TYPE_NAME: &str = "Pressure";
    const EVENT_STREAM_TIMEOUT: Duration = Duration::from_millis(500);
    const HISTORY_PROCESSING_DURATION: Duration = Duration::from_millis(200);
    const TOPIC_CREATION_DURATION: Duration = Duration::from_millis(200);

    async fn create_pressure_publisher(
        comms: Arc<dyn Communicate>,
        service_unit_name: &str,
        type_name: Option<&str>,
    ) -> Publisher<Pressure> {
        let pub_metadata = PublisherMetadata::builder()
            .message_count(10)
            .message_size(5) // 4 bytes for fixed32 + 1 byte for type and field index value
            .build()
            .unwrap();

        new_pressure_publisher(comms, service_unit_name, type_name, None, pub_metadata).await
    }

    async fn new_pressure_publisher(
        comms: Arc<dyn Communicate>,
        service_unit_name: &str,
        type_name: Option<&str>,
        topic_name: Option<&str>,
        metadata: PublisherMetadata,
    ) -> Publisher<Pressure> {
        new_publisher::<Pressure>(comms, service_unit_name, type_name, topic_name, metadata).await
    }

    async fn new_publisher<T: protobuf::Message>(
        comms: Arc<dyn Communicate>,
        service_unit_name: &str,
        type_name: Option<&str>,
        topic_name: Option<&str>,
        metadata: PublisherMetadata,
    ) -> Publisher<T> {
        let mut builder = PublisherAttributes::builder()
            .publication_name(service_unit_name)
            .type_name(
                TypeName::builder()
                    .package_name(PACKAGE)
                    .bundle_name(OWNING_BUNDLE_NAME)
                    .type_name(type_name.unwrap_or(TYPE_NAME))
                    .build()
                    .unwrap(),
            )
            .metadata(metadata);

        if let Some(topic) = topic_name {
            builder = builder.topic_name(topic);
        }

        let attr = builder.build().unwrap();

        Publisher::<T>::new(comms.create_publish(&attr).await.unwrap())
    }

    #[tokio::test]
    async fn test_sdv_pub_sub() {
        // Initialize tracing for this test to ensure instrumented fields are processed.
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();

        let bundle_name: &str = "Pubsub";
        let service_unit_name: &str = "pressure-publication";
        let publication_unit_name = UnitName::builder()
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .build()
            .unwrap();

        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("{VM}:{PACKAGE}.{bundle_name}/sample"))
                .expect("ServiceFqin should not be malformed"),
        );
        let comms = Arc::new(SdvComms { context });

        // WHEN creating a publisher using SdvComms.
        let publisher = create_pressure_publisher(comms.clone(), service_unit_name, None).await;
        // THEN the publisher is `Send`.
        assert_send(&publisher);

        // WHEN finding a publication.
        let publication =
            comms.find_publication(&UnitQuery::Name(publication_unit_name.clone())).await.unwrap();

        // THEN the publication is `Send + Sync`.
        assert_send(&publication);
        assert_sync(&publication);

        // WHEN creating a subscriber using SdvComms.
        let subscriber = Subscriber::<Pressure>::new(
            publication.subscribe(&SubscribeOptions::default()).await.unwrap(),
        );
        // THEN the subscriber is `Send`.
        assert_send(&subscriber);

        // WHEN observing the publication.
        let mut observer =
            observe::<Pressure>(publication.observe(&SubscribeOptions::default()).await.unwrap());
        // THEN the observer is `Send`.
        assert_send(&observer);

        // WHEN creating History using SdvComms.
        let history = History::<Pressure>::new(
            publication
                .create_history(&SubscribeOptions::builder().message_count(1).build().unwrap())
                .await
                .unwrap(),
        );
        // THEN History is `Send`.
        assert_send(&history);

        // WHEN creating instant reader using SdvComms.
        let instant_reader = InstantReader::<Pressure>::new(publication.create_instant_read());
        // THEN InstantReader is `Send`.
        assert_send(&instant_reader);

        // AND WHEN reading the latest message before any message is published.
        // THEN an empty Option is returned.
        assert!(instant_reader.read_latest_message().await.unwrap().is_none());

        // AND WHEN a batch of two messages is published.
        let pressure_msg1 = Pressure { pressure: 1234, ..Default::default() };
        let pressure_msg2 = Pressure { pressure: 5678, ..Default::default() };
        publisher.publish_batch(&[pressure_msg1.clone(), pressure_msg2.clone()]).unwrap();

        // THEN subscriber can read messages.
        // Read both messages published in the batch.
        let read_batch = subscriber.read_next_messages(2).unwrap();
        assert_eq!(read_batch.len(), 2);
        assert_eq!(read_batch[0].pressure, 1234);
        assert_eq!(read_batch[1].pressure, 5678);

        // THEN observer can receive a message.
        // Observer receives messages in batches as they are published.
        let received_batch = observer.next().await.unwrap().unwrap();
        assert_eq!(received_batch.len(), 2);
        assert_eq!(received_batch[0].pressure, 1234);
        assert_eq!(received_batch[1].pressure, 5678);

        // THEN one can read from history.
        // SLEEP to ensure the message is forwarded to the history buffer.
        // The history in this test is created with message_count(1), so it will store the latest message.
        thread::sleep(HISTORY_PROCESSING_DURATION);
        let history_read_batch = history.read_from_history(1).unwrap();
        assert_eq!(history_read_batch.len(), 1);
        assert_eq!(history_read_batch[0].pressure, 5678); // Latest message from the batch

        // THEN one can read using InstantReader
        let read_instant = instant_reader.read_latest_message().await.unwrap().unwrap();
        assert_eq!(read_instant.pressure, 5678); // Latest message
    }

    #[tokio::test]
    async fn test_sdv_pub_sub_find_per_instance_name() {
        let instance_name: &str = "sample";
        let bundle_name: &str = "PubSubInstance";
        let service_unit_name: &str = "pressure-publication";
        let publication_unit_name = UnitName::builder()
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .build()
            .unwrap();

        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("{VM}:{PACKAGE}.{bundle_name}/{instance_name}"))
                .expect("ServiceFqin should not be malformed"),
        );
        let comms = Arc::new(SdvComms { context });

        // GIVEN a publisher with Fqin with instance-name "sample"
        let _publisher = create_pressure_publisher(comms.clone(), service_unit_name, None).await;

        // WHEN finding a publication.
        let publication =
            comms.find_publication(&UnitQuery::Name(publication_unit_name.clone())).await;

        // THEN the publication is found
        assert!(publication.is_ok());
    }

    #[tokio::test]
    async fn test_sdv_pub_sub_does_not_find_wrong_instance_name() {
        let instance_name: &str = "sample-not-existing";
        let bundle_name: &str = "PubSubWrongInstance";
        let service_unit_name: &str = "pressure-publication";
        let publication_unit_name = UnitName::builder()
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .instance_name(instance_name)
            .build()
            .unwrap();

        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("{VM}:{PACKAGE}.{bundle_name}/sample-wrong-instance"))
                .expect("ServiceFqin should not be malformed"),
        );
        let comms = Arc::new(SdvComms { context });

        // GIVEN a publisher with Fqin with instance-name "sample"
        let _publisher = create_pressure_publisher(comms.clone(), service_unit_name, None).await;

        // WHEN finding a publication with difference instance-name "sample-not-existing"
        let publication =
            comms.find_publication(&UnitQuery::Name(publication_unit_name.clone())).await;

        // THEN the publication is NOT found
        assert!(publication.is_err());
    }

    #[tokio::test]
    async fn test_publication_availability_by_name() {
        // We need to init Binder for the event registration stream
        // as Binder is used internally by SDV Service Discovery.
        // Events are not emitted unless `binder::ProcessState::start_thread_pool()`
        // is called beforehand.
        binder::ProcessState::start_thread_pool();

        let bundle_name: &str = "PublicationAvailabilityName";
        let publication_name: &str = "pressure-publication";

        // GIVEN a service-unit-name
        let service_unit_name = UnitName::builder()
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(publication_name)
            .build()
            .unwrap();

        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("{VM}:{PACKAGE}.{bundle_name}/sample"))
                .expect("ServiceFqin should not be malformed"),
        );
        let comms = Arc::new(SdvComms { context });

        // WHEN create registration event stream
        let mut registration_event_stream_created_at_start = comms
            .create_registration_event_stream(&UnitQuery::Name(service_unit_name.clone()))
            .await
            .unwrap();

        // WHEN finding a publication that doesn't exist.
        let failed_publication_res =
            comms.find_publication(&UnitQuery::Name(service_unit_name.clone())).await;

        // THEN the return error.
        assert!(failed_publication_res.is_err());

        // WHEN creating a publisher using SdvComms.
        let publisher = create_pressure_publisher(comms.clone(), publication_name, None).await;

        // AND WHEN creating a registration event stream after the publisher was created
        let mut registration_event_stream_created_after_publisher = comms
            .create_registration_event_stream(&UnitQuery::Name(service_unit_name.clone()))
            .await
            .unwrap();

        // THEN both event streams have an available item.
        assert_eq!(
            tokio::time::timeout(
                EVENT_STREAM_TIMEOUT,
                registration_event_stream_created_at_start.next()
            )
            .await,
            Ok(Some(Availability::Available))
        );
        assert_eq!(
            tokio::time::timeout(
                EVENT_STREAM_TIMEOUT,
                registration_event_stream_created_after_publisher.next()
            )
            .await,
            Ok(Some(Availability::Available))
        );

        // AND THEN we can read messages.
        let pressure = 281204;
        let publication =
            comms.find_publication(&UnitQuery::Name(service_unit_name.clone())).await.unwrap();
        let mut observer =
            observe::<Pressure>(publication.observe(&SubscribeOptions::default()).await.unwrap());

        let pressure_msg = Pressure { pressure, ..Default::default() };
        publisher.publish(&pressure_msg).unwrap();

        let received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].pressure, pressure);

        // Dropping the publisher to test for the `Unavailable` event
        drop(publisher);

        // THEN unavailable
        assert_eq!(
            tokio::time::timeout(
                EVENT_STREAM_TIMEOUT,
                registration_event_stream_created_at_start.next()
            )
            .await,
            Ok(Some(Availability::Unavailable))
        );
        assert_eq!(
            tokio::time::timeout(
                EVENT_STREAM_TIMEOUT,
                registration_event_stream_created_after_publisher.next()
            )
            .await,
            Ok(Some(Availability::Unavailable))
        );
    }

    #[tokio::test]
    async fn test_service_unit_availability_stream_by_name_with_instance_name() {
        // We need to init Binder for the event registration stream
        // as Binder is used internally by SDV Service Discovery.
        // Events are not emitted unless `binder::ProcessState::start_thread_pool()`
        // is called beforehand.
        binder::ProcessState::start_thread_pool();

        let bundle_name: &str = "PublicationAvailabilityName";
        let service_unit_name: &str = "pressure-publication";
        let service_bundle_instance_name: &str = "instance-name";

        // GIVEN a service-unit-name with instance-name "instance-name"
        let unit_name = UnitName::builder()
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .instance_name(service_bundle_instance_name)
            .build()
            .unwrap();

        // AND GIVEN a context with the same Fqin of service_unit_name
        let context = ContextRef::create(
            ServiceFqin::from_str(&format!(
                "{VM}:{PACKAGE}.{bundle_name}/{service_bundle_instance_name}"
            ))
            .expect("ServiceFqin should not be malformed"),
        );
        let comms = Arc::new(SdvComms { context });

        // WHEN create registration event stream
        let mut registration_event_stream_created_at_start = comms
            .create_registration_event_stream(&UnitQuery::Name(unit_name.clone()))
            .await
            .unwrap();

        // AND WHEN creating a publisher using SdvComms.
        let publisher = create_pressure_publisher(comms.clone(), service_unit_name, None).await;

        // THEN both event streams have an available item.
        assert_eq!(
            tokio::time::timeout(
                EVENT_STREAM_TIMEOUT,
                registration_event_stream_created_at_start.next()
            )
            .await,
            Ok(Some(Availability::Available))
        );

        // WHEN the publisher is dropped
        drop(publisher);

        // THEN the event stream has an unavailable item.
        assert_eq!(
            tokio::time::timeout(
                EVENT_STREAM_TIMEOUT,
                registration_event_stream_created_at_start.next()
            )
            .await,
            Ok(Some(Availability::Unavailable))
        );
    }

    #[tokio::test]
    async fn test_service_unit_availability_stream_by_name_with_invalid_instance_name() {
        // We need to init Binder for the event registration stream
        // as Binder is used internally by SDV Service Discovery.
        // Events are not emitted unless `binder::ProcessState::start_thread_pool()`
        // is called beforehand.
        binder::ProcessState::start_thread_pool();

        let bundle_name: &str = "PublicationAvailabilityName";
        let service_unit_name: &str = "pressure-publication";
        let service_bundle_instance_name: &str = "instance-name-registration";
        let service_bundle_instance_name_search: &str = "do-not-exists";

        // GIVEN a service-unit-name with instance-name "do-not-exists"
        let unit_name = UnitName::builder()
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .instance_name(service_bundle_instance_name_search)
            .build()
            .unwrap();

        // AND GIVEN a Fqin context with instance-name "instance-name-registration"
        let context = ContextRef::create(
            ServiceFqin::from_str(&format!(
                "{VM}:{PACKAGE}.{bundle_name}/{service_bundle_instance_name}"
            ))
            .expect("ServiceFqin should not be malformed"),
        );
        let comms = Arc::new(SdvComms { context });

        // WHEN create registration event stream
        let mut registration_event_stream_created_at_start = comms
            .create_registration_event_stream(&UnitQuery::Name(unit_name.clone()))
            .await
            .unwrap();

        // AND WHEN creating a publisher using SdvComms.
        let _publisher = create_pressure_publisher(comms.clone(), service_unit_name, None).await;

        // THEN both event streams receives no items an available item.
        assert!(tokio::time::timeout(
            EVENT_STREAM_TIMEOUT,
            registration_event_stream_created_at_start.next()
        )
        .await
        .is_err(),);
    }

    #[tokio::test]
    async fn test_publication_availability_by_type() {
        // We need to init Binder for the event registration stream
        // as Binder is used internally by SDV Service Discovery.
        // Events are not emitted unless `binder::ProcessState::start_thread_pool()`
        // is called beforehand.
        binder::ProcessState::start_thread_pool();

        // GIVEN a TypeName.

        // Note: Tests in the same test target can be run in parallel.
        //       Given we test availability-by-type here, the UnitType type_name for this test
        //       must be unique throughout the test target!
        let type_name: &str = "AvailabilityTypePressure";
        let fq_type_name = TypeName::builder()
            .package_name(PACKAGE)
            .bundle_name(OWNING_BUNDLE_NAME)
            .type_name(type_name)
            .build()
            .unwrap();

        let publisher_name_one: &str = "pub-one";
        let publisher_name_two: &str = "pub-two";
        let bundle_name: &str = "PublicationAvailabilityType";
        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("{VM}:{PACKAGE}.{bundle_name}/sample"))
                .expect("ServiceFqin should not be malformed"),
        );
        let comms = Arc::new(SdvComms { context });

        // WHEN creating registration event stream by Type.
        let mut registration_event_stream_created_at_start = comms
            .create_registration_event_stream(&UnitQuery::Type(fq_type_name.clone()))
            .await
            .unwrap();

        // WHEN finding a publication that doesn't exist.
        let failed_publication_res =
            comms.find_publication(&UnitQuery::Type(fq_type_name.clone())).await;

        // THEN the return error.
        assert!(failed_publication_res.is_err());

        // WHEN creating a publisher using SdvComms.
        let publisher_one =
            create_pressure_publisher(comms.clone(), publisher_name_one, Some(type_name)).await;

        // AND WHEN creating a registration event stream after the publisher was created
        let mut registration_event_stream_created_after_publisher = comms
            .create_registration_event_stream(&UnitQuery::Type(fq_type_name.clone()))
            .await
            .unwrap();

        // THEN both event streams have an available item.
        assert_eq!(
            tokio::time::timeout(
                EVENT_STREAM_TIMEOUT,
                registration_event_stream_created_at_start.next()
            )
            .await,
            Ok(Some(Availability::Available))
        );
        assert_eq!(
            tokio::time::timeout(
                EVENT_STREAM_TIMEOUT,
                registration_event_stream_created_after_publisher.next()
            )
            .await,
            Ok(Some(Availability::Available))
        );

        // AND THEN we can read messages.
        let pressure = 54734;
        let publication =
            comms.find_publication(&UnitQuery::Type(fq_type_name.clone())).await.unwrap();
        let mut observer =
            observe::<Pressure>(publication.observe(&SubscribeOptions::default()).await.unwrap());
        let pressure_msg = Pressure { pressure, ..Default::default() };
        publisher_one.publish(&pressure_msg).unwrap();

        let received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].pressure, pressure);

        // AND WHEN creating a second publisher for the same type.
        let _publisher_two =
            create_pressure_publisher(comms.clone(), publisher_name_two, Some(type_name)).await;

        // THEN the streams have an another available item.
        assert_eq!(
            tokio::time::timeout(
                EVENT_STREAM_TIMEOUT,
                registration_event_stream_created_at_start.next()
            )
            .await,
            Ok(Some(Availability::Available))
        );
    }

    #[tokio::test]
    async fn test_read_on_subscribe() {
        let bundle_name: &str = "ReadOnSubscribe";
        let service_unit_name: &str = "pressure-publication";

        let publication_unit_name = UnitName::builder()
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .build()
            .unwrap();

        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("{VM}:{PACKAGE}.{bundle_name}/sample"))
                .expect("ServiceFqin should not be malformed"),
        );
        let comms = Arc::new(SdvComms { context });

        // GIVEN a publisher that publishes a pressure message
        let publisher = create_pressure_publisher(comms.clone(), service_unit_name, None).await;
        let pressure: u32 = 43321;
        let mut pressure_msg = Pressure::new();
        pressure_msg.pressure = pressure;
        publisher.publish(&pressure_msg).unwrap();

        // WHEN creating an observer with read_last_message enabled.
        let publication =
            comms.find_publication(&UnitQuery::Name(publication_unit_name.clone())).await.unwrap();
        let mut observer = observe::<Pressure>(
            publication
                .observe(&SubscribeOptions::builder().read_on_subscribe(true).build().unwrap())
                .await
                .unwrap(),
        );
        // AND WHEN creating a subscriber with read_last_message enabled.
        let subscriber = Subscriber::<Pressure>::new(
            publication
                .subscribe(&SubscribeOptions::builder().read_on_subscribe(true).build().unwrap())
                .await
                .unwrap(),
        );
        // AND WHEN creating history with read_last_message enabled.
        let history = History::<Pressure>::new(
            publication
                .create_history(
                    &SubscribeOptions::builder()
                        .message_count(1)
                        .read_on_subscribe(true) // This is not supported for History and will be ignored.
                        .build()
                        .unwrap(),
                )
                .await
                .unwrap(),
        );
        // AND WHEN creating instant reader using SdvComms.
        let instant_reader = InstantReader::<Pressure>::new(publication.create_instant_read());

        // THEN observer receives the last message before its creation.
        let received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].pressure, pressure);
        // THEN subscriber can read the last message before its creation.
        let read = subscriber.read_next_messages(1).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].pressure, pressure);
        // THEN  InstantReader can read the last message before its creation.
        let read = instant_reader.read_latest_message().await.unwrap().unwrap();
        assert_eq!(read.pressure, pressure);

        // THEN history CANNOT read the last message before its creation.
        let read = history.read_from_history(1).unwrap();
        assert_eq!(read.len(), 0);
    }

    #[tokio::test]
    async fn test_sdv_multipub_sub_e2e() {
        const MULTIPUB_PROCESSING_DURATION: Duration = Duration::from_millis(200);

        // Multipub depends on a SD listener that needs a running binder thread.
        binder::ProcessState::start_thread_pool();

        // Note: Tests in the same test target can be run in parallel.
        //       Given multipub works by-type, the UnitType type_name for this test
        //       must be unique throughout the test target!
        let type_name: &str = "MultipubPressure";
        let fq_type_name = TypeName::builder()
            .package_name(PACKAGE)
            .bundle_name(OWNING_BUNDLE_NAME)
            .type_name(type_name)
            .build()
            .unwrap();

        let publisher_name_one: &str = "pub-one";
        let publisher_name_two: &str = "pub-two";
        let publisher_name_three: &str = "pub-three";

        let bundle_name: &str = "Multipubsub";
        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("{VM}:{PACKAGE}.{bundle_name}/sample"))
                .expect("ServiceFqin should not be malformed"),
        );
        let comms = Arc::new(SdvComms { context });

        // GIVEN multiple publishers for the same UnitType.
        let pub_one =
            create_pressure_publisher(comms.clone(), publisher_name_one, Some(type_name)).await;
        let pub_two =
            create_pressure_publisher(comms.clone(), publisher_name_two, Some(type_name)).await;
        let pub_three =
            create_pressure_publisher(comms.clone(), publisher_name_three, Some(type_name)).await;

        // AND GIVEN a subscriber for the UnitType.
        let subscribe_handle =
            comms.subscribe_by_type(&fq_type_name, &SubscribeOptions::default()).await.unwrap();
        let subscriber = Subscriber::<Pressure>::new(subscribe_handle);

        // AND GIVEN an observer for the UnitType.
        let mut observer = observe::<Pressure>(
            comms.observe_by_type(&fq_type_name, &SubscribeOptions::default()).await.unwrap(),
        );

        // Needed for the multiplexer to be created on a separate process.
        thread::sleep(MULTIPUB_PROCESSING_DURATION);

        // WHEN a message is published.
        pub_one.publish(&Pressure { pressure: 1234, ..Default::default() }).unwrap();
        pub_two.publish(&Pressure { pressure: 9876, ..Default::default() }).unwrap();
        pub_three.publish(&Pressure { pressure: 9182, ..Default::default() }).unwrap();

        // Needed for message propagation to the multiplexer queue.
        thread::sleep(MULTIPUB_PROCESSING_DURATION);

        // THEN subscriber can read messages.
        let mut read = subscriber.read_next_messages(3).unwrap();
        assert_eq!(read.len(), 3);
        // Ordering of messages from different contributing topics is not guaranteed.
        read.sort_by_key(|x| x.pressure);
        assert_eq!(read[0].pressure, 1234);
        assert_eq!(read[1].pressure, 9182);
        assert_eq!(read[2].pressure, 9876);

        // THEN observer can receive a message.
        let mut received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 3);
        // Ordering of messages from different contributing topics is not guaranteed.
        received.sort_by_key(|x| x.pressure);
        assert_eq!(received[0].pressure, 1234);
        assert_eq!(received[1].pressure, 9182);
        assert_eq!(received[2].pressure, 9876);
    }

    #[tokio::test]
    async fn test_get_publication_unit_by_name_fails() {
        // Given SDVComms without any registered publication.
        let bundle_name: &str = "PubsubUnitNameFail";
        let service_unit_name: &str = "unit-name";
        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("{VM}:{PACKAGE}.{bundle_name}/sample"))
                .expect("ServiceFqin should not be malformed"),
        );
        let comms = SdvComms { context };

        // WHEN finding a publication by unit name.
        let result = comms
            .find_publication(&UnitQuery::Name(
                UnitName::builder()
                    .package_name(PACKAGE)
                    .bundle_name(bundle_name)
                    .service_unit_name(service_unit_name)
                    .build()
                    .unwrap(),
            ))
            .await;

        // THEN return error result.
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status_code(), SdvStatusCode::Unavailable);
    }

    #[tokio::test]
    async fn test_get_publication_by_type_fails() {
        // Given SDV comms without any registered publication.
        let bundle_name: &str = "PubSubTypeNameFail";
        let type_name = "UnregisteredType";
        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("{VM}:{PACKAGE}.{bundle_name}/sample"))
                .expect("ServiceFqin should not be malformed"),
        );
        let comms = SdvComms { context };

        // WHEN finding a publication unit type.
        let result = comms
            .find_publication(&UnitQuery::Type(
                TypeName::builder()
                    .package_name(PACKAGE)
                    .bundle_name(OWNING_BUNDLE_NAME)
                    .type_name(type_name)
                    .build()
                    .unwrap(),
            ))
            .await;

        // THEN the return error.
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status_code(), SdvStatusCode::Unavailable);
    }

    async fn find_service_unit(
        unit_name: &UnitName,
        context: ContextRef,
    ) -> sdv_comms::sd::ServiceUnitDefinition {
        let sd = ServiceDiscoveryManager::new(context);

        let discovery_args = UnitNameDiscoveryArgs::new_builder()
            .set_sdv_package_name(unit_name.get_package_name())
            .set_service_bundle_name(unit_name.get_bundle_name())
            .set_service_unit_name(unit_name.get_service_unit_name())
            .build()
            .expect("Discovery args builder failed.");
        sd.list_units_by_name(&discovery_args).next().await.expect("Did not discover publisher.")
    }

    #[tokio::test]
    async fn test_pub_optional_msg_metadata() {
        let message_full_name: &str = "Pressure";
        let bundle_name: &str = "OptionalMetadata";
        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("{VM}:{PACKAGE}.{bundle_name}/sample"))
                .expect("ServiceFqin should not be malformed"),
        );
        let comms = Arc::new(SdvComms { context });
        let pressure_metadata = PublisherMetadata::builder()
            .message_count(10)
            .message_size(5)
            .message_metadata(sdv_proto_reflection_rs::message_metadata::MessageMetadata {
                message_full_name: message_full_name.to_string(),
                ..Default::default()
            });

        // GIVEN a publisher without message metadata.
        let service_unit_name_without_msg_metadata: &str = "without-msg-metadata";

        let unit_name_without_msg_metadata = UnitName::builder()
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name_without_msg_metadata)
            .build()
            .unwrap();

        let pub_metadata_without_msg_metadata = pressure_metadata.clone().build().unwrap(); // message_metadata_required = false

        let _pub_without_msg_metadata = new_pressure_publisher(
            comms.clone(),
            service_unit_name_without_msg_metadata,
            None,
            None,
            pub_metadata_without_msg_metadata,
        )
        .await;

        // AND GIVEN a publisher with message metadata.
        let service_unit_name_with_msg_metadata: &str = "with-msg-metadata";

        let unit_name_with_msg_metadata = UnitName::builder()
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name_with_msg_metadata)
            .build()
            .unwrap();

        let pub_metadata_with_msg_metadata =
            pressure_metadata.message_metadata_required(true).build().unwrap();

        let _pub_with_msg_metadata = new_pressure_publisher(
            comms.clone(),
            service_unit_name_with_msg_metadata,
            None,
            None,
            pub_metadata_with_msg_metadata,
        )
        .await;

        // WHEN discovering the publication service units
        let service_unit_without_msg_metadata =
            find_service_unit(&unit_name_without_msg_metadata, context).await;
        let service_unit_with_msg_metadata =
            find_service_unit(&unit_name_with_msg_metadata, context).await;

        // THEN the one configured without message metadata does not have message metadata.
        assert!(service_unit_without_msg_metadata.get_application_metadata().is_empty());

        // AND THEN the one configured with message metadata has message metadata.
        let app_metadata = service_unit_with_msg_metadata.get_application_metadata();
        assert!(!app_metadata.is_empty());
        let msg_metadata: sdv_proto_reflection_rs::message_metadata::MessageMetadata =
            protobuf::Message::parse_from_bytes(&app_metadata).unwrap();
        assert_eq!(msg_metadata.message_full_name, message_full_name);
    }

    #[tokio::test]
    async fn test_get_client_by_name_fails() {
        // Given SDV comms without any registered servers.
        let bundle_name: &str = "RpcByUnitNameFail";
        let service_unit_name: &str = "unit-name";
        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("{VM}:{PACKAGE}.{bundle_name}/sample"))
                .expect("ServiceFqin should not be malformed"),
        );
        let comms = SdvComms { context };

        // WHEN finding an RPC client by unit name.
        let result = comms
            .create_client_transport(
                &ClientAttributes::builder()
                    .server_name(
                        UnitName::builder()
                            .vm_name(VM)
                            .package_name(PACKAGE)
                            .bundle_name(bundle_name)
                            .service_unit_name(service_unit_name)
                            .build()
                            .unwrap(),
                    )
                    .build()
                    .unwrap(),
            )
            .await;

        // THEN the return error.
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status_code(), SdvStatusCode::Unavailable);
    }

    #[tokio::test]
    async fn test_sdv_rpc_e2e() {
        // We need to init Binder for the event registration stream
        // as Binder is used internally by SDV Service Discovery.
        // Events are not emitted unless `binder::ProcessState::start_thread_pool()`
        // is called beforehand.
        binder::ProcessState::start_thread_pool();

        let service_unit_name: &str = "pressure-server";
        let method_name = String::from("GetTirePressure");
        let bundle_name: &str = "RpcBundle";
        let fqin_string = format!("{VM}:{PACKAGE}.{bundle_name}/sample");
        let context = ContextRef::create(
            ServiceFqin::from_str(&fqin_string).expect("ServiceFqin should not be malformed"),
        );
        let comms = SdvComms { context };

        // The VM name in the FQIN from Context may be different from what
        // was provided during Context creation. Let's update the FQIN
        // because of that.
        let client_fqin = context.get_self_fqin();

        let server_unit_name = UnitName::builder()
            .vm_name(context.get_self_fqin().get_sdv_vm_name())
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .build()
            .unwrap();

        // WHEN create registration event stream
        let mut registration_event_stream_created_at_start = comms
            .create_registration_event_stream(&UnitQuery::Name(server_unit_name.clone()))
            .await
            .unwrap();

        // GIVEN a remote method.
        let get_pressure = Arc::new(
            move |caller_fqin: ServiceFqin, _req: &protobuf::well_known_types::empty::Empty| {
                // THEN the caller_fqin matches the client_fqin
                assert_eq!(client_fqin, caller_fqin);

                let mut response = Pressure::new();
                response.pressure = 1234;
                Box::pin(async move { Ok(response) })
            },
        );

        // AND GIVEN a server.
        let attr = ServerAttributes::builder()
            .server_name(service_unit_name)
            .type_name(
                TypeName::builder()
                    .package_name(PACKAGE)
                    .bundle_name(bundle_name)
                    .type_name(TYPE_NAME)
                    .build()
                    .unwrap(),
            )
            .server_options(ServerOptions::default())
            .build()
            .unwrap();
        let methods = MethodMapBuilder::new()
            .add_method::<Empty, Pressure>(&method_name, {
                std::sync::Arc::new(move |fqin, req| {
                    let callback = get_pressure.clone();
                    Box::pin(async move { callback(fqin, &req).await })
                })
            })
            .build();
        let server = comms.create_server_transport(&attr, methods).await.unwrap();

        // WHEN create registration event stream after the server was created
        let mut registration_event_stream_created_after_server = comms
            .create_registration_event_stream(&UnitQuery::Name(server_unit_name.clone()))
            .await
            .unwrap();
        // THEN available
        assert_eq!(
            tokio::time::timeout(
                EVENT_STREAM_TIMEOUT,
                registration_event_stream_created_at_start.next()
            )
            .await,
            Ok(Some(Availability::Available))
        );
        assert_eq!(
            tokio::time::timeout(
                EVENT_STREAM_TIMEOUT,
                registration_event_stream_created_after_server.next()
            )
            .await,
            Ok(Some(Availability::Available))
        );

        // AND GIVEN a client to that server.
        let client = comms
            .create_client_transport(
                &ClientAttributes::builder().server_name(server_unit_name.clone()).build().unwrap(),
            )
            .await
            .unwrap();

        // WHEN calling the remote method.
        let response =
            call_method::<Empty, Pressure>(&*client, &method_name, &Empty::new()).await.unwrap();

        // THEN the response holds the expected value.
        assert_eq!(response.pressure, 1234);

        // Dropping the server to test for the `Unavailable` event
        drop(server);

        // THEN unavailable
        assert_eq!(
            tokio::time::timeout(
                EVENT_STREAM_TIMEOUT,
                registration_event_stream_created_at_start.next()
            )
            .await,
            Ok(Some(Availability::Unavailable))
        );
        assert_eq!(
            tokio::time::timeout(
                EVENT_STREAM_TIMEOUT,
                registration_event_stream_created_after_server.next()
            )
            .await,
            Ok(Some(Availability::Unavailable))
        );

        // Unfortunately drop(server) just initiates the destruction of server. There are
        // no guarantees that server stops accepting new connections on drop() return.
        // So we have to wait for a while.
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;

        // WHEN calling the remote method of a dropped server.
        let response = call_method::<Empty, Pressure>(&*client, &method_name, &Empty::new()).await;

        // THEN an error for Unavailable service is returned.
        assert!(response.is_err());
        let response_err = response.unwrap_err();
        assert!(response_err.to_string().contains("Unavailable"));
    }

    async fn build_server(
        service_unit_name: &str,
        method_name: String,
        bundle_name: &str,
        instance_name: &str,
    ) -> (SdvResult<Box<dyn ServerTransport>>, ContextRef, SdvComms) {
        let fqin_string = format!("{VM}:{PACKAGE}.{bundle_name}/{instance_name}");
        let context = ContextRef::create(
            ServiceFqin::from_str(&fqin_string).expect("ServiceFqin should not be malformed"),
        );
        let comms = SdvComms { context };

        // The VM name in the FQIN from Context may be different from what
        // was provided during Context creation. Let's update the FQIN
        // because of that.
        let client_fqin = context.get_self_fqin();

        // GIVEN a remote method.
        let get_pressure = Arc::new(
            move |caller_fqin: ServiceFqin, _req: &protobuf::well_known_types::empty::Empty| {
                // THEN the caller_fqin matches the client_fqin
                assert_eq!(client_fqin, caller_fqin);

                let mut response = Pressure::new();
                response.pressure = 1234;
                Box::pin(async move { Ok(response) })
            },
        );

        // AND GIVEN a server.
        let attr = ServerAttributes::builder()
            .server_name(service_unit_name)
            .type_name(
                TypeName::builder()
                    .package_name(PACKAGE)
                    .bundle_name(bundle_name)
                    .type_name(TYPE_NAME)
                    .build()
                    .unwrap(),
            )
            .server_options(ServerOptions::default())
            .build()
            .unwrap();
        let methods = MethodMapBuilder::new()
            .add_method::<Empty, Pressure>(&method_name, {
                std::sync::Arc::new(move |fqin, req| {
                    let callback = get_pressure.clone();
                    Box::pin(async move { callback(fqin, &req).await })
                })
            })
            .build();
        let server = comms.create_server_transport(&attr, methods).await;
        assert!(server.is_ok());
        (server, context, comms)
    }

    #[tokio::test]
    async fn test_sdv_rpc_discovery_instance_name_success() {
        let service_unit_name: &str = "pressure-server-instance";
        let method_name = String::from("GetTirePressureInstance");
        let bundle_name: &str = "RpcInstanceBundle";
        let instance_name: &str = "sample-instance";

        // GIVEN a server is registered with an instance name.
        let (server, context, comms) =
            build_server(service_unit_name, method_name, bundle_name, instance_name).await;
        assert!(server.is_ok());

        let server_unit_name = UnitName::builder()
            .vm_name(context.get_self_fqin().get_sdv_vm_name())
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .instance_name(instance_name)
            .build()
            .unwrap();

        // WHEN creating a client for that bundle instance name
        let client = comms
            .create_client_transport(
                &ClientAttributes::builder().server_name(server_unit_name.clone()).build().unwrap(),
            )
            .await;

        // THEN a valid client is returned.
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_sdv_rpc_discovery_instance_name_not_found() {
        let service_unit_name: &str = "pressure-server-instance-not-found";
        let method_name = String::from("GetTirePressureInstanceNotFound");
        let bundle_name: &str = "RpcInstanceNotFoundBundle";
        let bundle_instance_name: &str = "sample-instance";

        // GIVEN a server is registered with an instance name.
        let (server, context, comms) =
            build_server(service_unit_name, method_name, bundle_name, bundle_instance_name).await;
        assert!(server.is_ok());

        let server_unit_name = UnitName::builder()
            .vm_name(context.get_self_fqin().get_sdv_vm_name())
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .instance_name("instance-not-found")
            .build()
            .unwrap();

        // WHEN creating a client using a different bundle instance name
        let client = comms
            .create_client_transport(
                &ClientAttributes::builder().server_name(server_unit_name.clone()).build().unwrap(),
            )
            .await;

        // THEN an error is returned
        assert!(client.is_err());
    }

    #[tokio::test]
    async fn test_sdv_rpc_response_timeout_ok() {
        let service_unit_name: &str = "call-timeout-ok";
        let method_name = String::from("QuickMethod");
        let bundle_name: &str = "RpcInstanceBundle";
        let fqin_string = format!("{VM}:{PACKAGE}.{bundle_name}/quickmethod");
        let context = ContextRef::create(
            ServiceFqin::from_str(&fqin_string).expect("ServiceFqin should not be malformed"),
        );
        let comms = SdvComms { context };

        const PAYLOAD_SIZE: usize = 5;

        // The VM name in the FQIN from Context may be different from what
        // was provided during Context creation. Let's update the FQIN
        // because of that.
        let client_fqin = context.get_self_fqin();

        let server_unit_name = UnitName::builder()
            .vm_name(context.get_self_fqin().get_sdv_vm_name())
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .build()
            .unwrap();

        // GIVEN a remote method that respond without any delay
        let quick_method = Arc::new(move |caller_fqin: ServiceFqin, _req: &Payload| {
            assert_eq!(client_fqin, caller_fqin);
            let response = Payload::new();
            Box::pin(async move { Ok(response) })
        });

        // AND GIVEN a server
        let server_options = ServerOptions::default();
        let server_attr = ServerAttributes::builder()
            .server_name(service_unit_name)
            .type_name(
                TypeName::builder()
                    .package_name(PACKAGE)
                    .bundle_name(bundle_name)
                    .type_name(TYPE_NAME)
                    .build()
                    .unwrap(),
            )
            .server_options(server_options)
            .build()
            .unwrap();
        let methods = MethodMapBuilder::new()
            .add_method::<Payload, Payload>(&method_name, {
                std::sync::Arc::new(move |fqin, req| {
                    let callback = quick_method.clone();
                    Box::pin(async move { callback(fqin, &req).await })
                })
            })
            .build();
        let _server = comms.create_server_transport(&server_attr, methods).await.unwrap();

        // AND GIVEN a client with 1 second timeout
        let client_options = ClientOptions::builder().timeout_in_sec(1).build().unwrap();
        let client_attr = ClientAttributes::builder()
            .server_name(server_unit_name.clone())
            .client_options(client_options)
            .build()
            .unwrap();
        let client = comms.create_client_transport(&client_attr).await.unwrap();

        // WHEN calling the remote method
        let message = Payload { payload: vec![8u8; PAYLOAD_SIZE], ..Default::default() };
        let response = call_method::<Payload, Payload>(&*client, &method_name, &message).await;

        // THEN response is ok
        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn test_sdv_rpc_response_timeout_error() {
        let service_unit_name: &str = "call-timeout";
        let method_name = String::from("SlowMethod");
        let bundle_name: &str = "RpcInstanceBundle";
        let fqin_string = format!("{VM}:{PACKAGE}.{bundle_name}/slowmethod");
        let context = ContextRef::create(
            ServiceFqin::from_str(&fqin_string).expect("ServiceFqin should not be malformed"),
        );
        let comms = SdvComms { context };

        const PAYLOAD_SIZE: usize = 5;

        // The VM name in the FQIN from Context may be different from what
        // was provided during Context creation. Let's update the FQIN
        // because of that.
        let client_fqin = context.get_self_fqin();

        let server_unit_name = UnitName::builder()
            .vm_name(context.get_self_fqin().get_sdv_vm_name())
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .build()
            .unwrap();

        // GIVEN a remote method that wait for 2 seconds before respond
        let slow_method = Arc::new(move |caller_fqin: ServiceFqin, _req: &Payload| {
            assert_eq!(client_fqin, caller_fqin);
            let response = Payload::new();
            Box::pin(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                Ok(response)
            })
        });

        // AND GIVEN a server
        let server_options = ServerOptions::default();
        let server_attr = ServerAttributes::builder()
            .server_name(service_unit_name)
            .type_name(
                TypeName::builder()
                    .package_name(PACKAGE)
                    .bundle_name(bundle_name)
                    .type_name(TYPE_NAME)
                    .build()
                    .unwrap(),
            )
            .server_options(server_options)
            .build()
            .unwrap();
        let methods = MethodMapBuilder::new()
            .add_method::<Payload, Payload>(&method_name, {
                std::sync::Arc::new(move |fqin, req| {
                    let callback = slow_method.clone();
                    Box::pin(async move { callback(fqin, &req).await })
                })
            })
            .build();
        let _server = comms.create_server_transport(&server_attr, methods).await.unwrap();

        // AND GIVEN a client with 1 second timeout
        let client_options = ClientOptions::builder().timeout_in_sec(1).build().unwrap();
        let client_attr = ClientAttributes::builder()
            .server_name(server_unit_name.clone())
            .client_options(client_options)
            .build()
            .unwrap();
        let client = comms.create_client_transport(&client_attr).await.unwrap();

        // WHEN calling the remote method
        let message = Payload { payload: vec![8u8; PAYLOAD_SIZE], ..Default::default() };
        let response = call_method::<Payload, Payload>(&*client, &method_name, &message).await;

        // THEN response has error due to time out
        assert!(response.is_err());
        assert_eq!(response.unwrap_err().status_code(), SdvStatusCode::Cancelled);
    }

    #[tokio::test]
    async fn test_sdv_rpc_large_request() {
        let service_unit_name: &str = "server";
        let method_name = String::from("SendPayload");
        let bundle_name: &str = "RpcLargeRequestBundle";
        let fqin_string = format!("{VM}:{PACKAGE}.{bundle_name}/sample");
        let context = ContextRef::create(
            ServiceFqin::from_str(&fqin_string).expect("ServiceFqin should not be malformed"),
        );
        let comms = SdvComms { context };

        const PAYLOAD_SIZE: usize = 25_000_000;
        const SIZE_LEN: usize = std::mem::size_of::<usize>();

        // The VM name in the FQIN from Context may be different from what
        // was provided during Context creation. Let's update the FQIN
        // because of that.
        let client_fqin = context.get_self_fqin();

        let server_unit_name = UnitName::builder()
            .vm_name(context.get_self_fqin().get_sdv_vm_name())
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .build()
            .unwrap();

        // GIVEN a remote method.
        let send_payload = Arc::new(move |caller_fqin: ServiceFqin, req: &Payload| {
            // THEN the caller_fqin matches the client_fqin
            assert_eq!(client_fqin, caller_fqin);

            let mut response = Payload::new();
            response.payload = req.payload.len().to_le_bytes().to_vec();
            Box::pin(async move { Ok(response) })
        });

        // AND GIVEN a large Request message
        let message = Payload { payload: vec![8u8; PAYLOAD_SIZE], ..Default::default() };

        // AND GIVEN a server with large payload size options
        let message_size = message.compute_size() as usize; // including size for field numbers.
        let server_options =
            ServerOptions::builder().max_request_size(Some(message_size)).build().unwrap();

        let server_attr = ServerAttributes::builder()
            .server_name(service_unit_name)
            .type_name(
                TypeName::builder()
                    .package_name(PACKAGE)
                    .bundle_name(bundle_name)
                    .type_name(TYPE_NAME)
                    .build()
                    .unwrap(),
            )
            .server_options(server_options)
            .build()
            .unwrap();
        let methods = MethodMapBuilder::new()
            .add_method::<Payload, Payload>(&method_name, {
                std::sync::Arc::new(move |fqin, req| {
                    let callback = send_payload.clone();
                    Box::pin(async move { callback(fqin, &req).await })
                })
            })
            .build();
        let _server = comms.create_server_transport(&server_attr, methods).await.unwrap();

        // AND GIVEN a client to that server.
        let client_options =
            ClientOptions::builder().max_response_size(Some(message_size)).build().unwrap();
        let client_attr = ClientAttributes::builder()
            .server_name(server_unit_name.clone())
            .client_options(client_options)
            .build()
            .unwrap();
        let client = comms.create_client_transport(&client_attr).await.unwrap();

        // WHEN calling the remote method with a large payload.
        let response =
            call_method::<Payload, Payload>(&*client, &method_name, &message).await.unwrap();

        // THEN the server was called with a payload of expected size.
        assert_eq!(
            usize::from_le_bytes(response.payload[..SIZE_LEN].try_into().unwrap()),
            PAYLOAD_SIZE
        );
    }

    #[tokio::test]
    async fn test_sdv_rpc_large_response_client_side_fails() {
        let service_unit_name: &str = "server";
        let method_name = String::from("SendPayload");
        let bundle_name: &str = "RpcLargeRequestFailingBundle";
        let fqin_string = format!("{VM}:{PACKAGE}.{bundle_name}/sample");
        let context = ContextRef::create(
            ServiceFqin::from_str(&fqin_string).expect("ServiceFqin should not be malformed"),
        );
        let comms = SdvComms { context };

        const PAYLOAD_SIZE: usize = 25_000_000;

        // The VM name in the FQIN from Context may be different from what
        // was provided during Context creation. Let's update the FQIN
        // because of that.
        let client_fqin = context.get_self_fqin();

        let server_unit_name = UnitName::builder()
            .vm_name(context.get_self_fqin().get_sdv_vm_name())
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .build()
            .unwrap();

        // GIVEN a remote method.
        let send_payload = Arc::new(move |caller_fqin: ServiceFqin, req: &Payload| {
            // THEN the caller_fqin matches the client_fqin
            assert_eq!(client_fqin, caller_fqin);

            let mut response = Payload::new();
            response.payload = req.payload.len().to_le_bytes().to_vec();
            Box::pin(async move { Ok(response) })
        });

        // AND GIVEN a large Request message
        let message = Payload { payload: vec![8u8; PAYLOAD_SIZE], ..Default::default() };

        // AND GIVEN a server with large payload size options
        let message_size = message.compute_size() as usize; // including size for field numbers.
        let server_options =
            ServerOptions::builder().max_request_size(Some(message_size)).build().unwrap();

        let server_attr = ServerAttributes::builder()
            .server_name(service_unit_name)
            .type_name(
                TypeName::builder()
                    .package_name(PACKAGE)
                    .bundle_name(bundle_name)
                    .type_name(TYPE_NAME)
                    .build()
                    .unwrap(),
            )
            .server_options(server_options)
            .build()
            .unwrap();
        let methods = MethodMapBuilder::new()
            .add_method::<Payload, Payload>(&method_name, {
                std::sync::Arc::new(move |fqin, req| {
                    let callback = send_payload.clone();
                    Box::pin(async move { callback(fqin, &req).await })
                })
            })
            .build();
        let _server = comms.create_server_transport(&server_attr, methods).await.unwrap();

        // AND GIVEN a client to that server.
        let client_options = ClientOptions::builder().max_response_size(Some(1)).build().unwrap();
        let client_attr = ClientAttributes::builder()
            .server_name(server_unit_name.clone())
            .client_options(client_options)
            .build()
            .unwrap();
        let client = comms.create_client_transport(&client_attr).await.unwrap();

        // WHEN calling the remote method with a large payload.
        let response = call_method::<Payload, Payload>(&*client, &method_name, &message).await;

        // THEN client receive error response due to out of range
        assert!(response.is_err());
        let error = response.unwrap_err();
        assert_eq!(error.status_code(), SdvStatusCode::OutOfRange);
    }

    #[tokio::test]
    async fn test_create_publish_oversized_message_fails() {
        let bundle_name: &str = "PublishOversizedMessage";
        let service_unit_name: &str = "oversized-pub";

        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("{VM}:{PACKAGE}.{bundle_name}/sample"))
                .expect("ServiceFqin should not be malformed"),
        );
        let comms = Arc::new(SdvComms { context });

        // GIVEN a publisher created with a specific message_size in its metadata.
        let pub_metadata =
            PublisherMetadata::builder().message_count(2).message_size(12).build().unwrap();

        let attr = PublisherAttributes::builder()
            .publication_name(service_unit_name)
            .type_name(
                TypeName::builder()
                    .package_name(PACKAGE)
                    .bundle_name(OWNING_BUNDLE_NAME)
                    .type_name(TYPE_NAME)
                    .build()
                    .unwrap(),
            )
            .metadata(pub_metadata)
            .build()
            .unwrap();

        let publisher_trait_object = comms.create_publish(&attr).await.unwrap();

        // WHEN trying to publish a message larger than the configured message_size.
        let oversized_message: Box<[u8]> = Box::new([0u8; 19]);

        // THEN the publish operation fails with an Internal error.
        let result = publisher_trait_object.publish_messages(&[oversized_message]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), SdvStatusCode::InvalidArgument);
        assert_eq!(
            err.to_string(),
            "SdvStatusCode::InvalidArgument(3): Message at index 0 (size 19) exceeds maximum allowed size 13"
        );
    }

    #[tokio::test]
    async fn test_sdv_rpc_client_stream_e2e() {
        let service_unit_name: &str = "client-streaming-server";
        let method_name = String::from("GetTirePressureClientStream");
        let bundle_name: &str = "RpcClientStreamingBundle";
        let fqin_string = format!("{VM}:{PACKAGE}.{bundle_name}/sample");
        let context = ContextRef::create(
            ServiceFqin::from_str(&fqin_string).expect("ServiceFqin should not be malformed"),
        );
        let comms = SdvComms { context };

        let server_unit_name = UnitName::builder()
            .vm_name(context.get_self_fqin().get_sdv_vm_name())
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .build()
            .unwrap();

        // GIVEN a server
        let attr = ServerAttributes::builder()
            .server_name(service_unit_name)
            .type_name(
                TypeName::builder()
                    .package_name(PACKAGE)
                    .bundle_name(bundle_name)
                    .type_name(TYPE_NAME)
                    .build()
                    .unwrap(),
            )
            .server_options(ServerOptions::default())
            .build()
            .unwrap();

        let methods = MethodMapBuilder::new()
            .add_client_streaming_method::<Pressure, Pressure>(&method_name, {
                std::sync::Arc::new(move |_fqin, req| {
                    // Consume the stream and return sum of the pressures
                    Box::pin(async move {
                        let total_pressure = req
                            .fold(0, |acc, pressure| async move { acc + pressure.pressure })
                            .await;

                        let mut response = Pressure::new();
                        response.pressure = total_pressure;
                        Ok(response)
                    })
                })
            })
            .build_rpc_methods();

        let _server = comms.create_server_transport_all_methods(&attr, methods).await.unwrap();

        // AND GIVEN a client to that server
        let client = comms
            .create_client_transport(
                &ClientAttributes::builder().server_name(server_unit_name.clone()).build().unwrap(),
            )
            .await
            .unwrap();

        // WHEN calling the remote method with a stream containing multiple messages
        let pressure1 = Pressure { pressure: 95, ..Default::default() };
        let pressure2 = Pressure { pressure: 100, ..Default::default() };
        let pressure3 = Pressure { pressure: 105, ..Default::default() };

        let request_stream = futures::stream::iter(vec![pressure1, pressure2, pressure3]).boxed();
        let response = call_client_streaming_method::<Pressure, Pressure>(
            &*client,
            &method_name,
            request_stream,
        )
        .await
        .unwrap();

        // THEN the response holds the expected value
        assert_eq!(response.pressure, 300);
    }

    #[tokio::test]
    async fn test_sdv_rpc_server_stream_e2e() {
        let service_unit_name: &str = "server-streaming-server";
        let method_name = String::from("GetTirePressureServerStream");
        let bundle_name: &str = "RpcServerStreamingBundle";
        let fqin_string = format!("{VM}:{PACKAGE}.{bundle_name}/sample");
        let context = ContextRef::create(
            ServiceFqin::from_str(&fqin_string).expect("ServiceFqin should not be malformed"),
        );

        let comms = SdvComms { context };

        let server_unit_name = UnitName::builder()
            .vm_name(context.get_self_fqin().get_sdv_vm_name())
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .build()
            .unwrap();

        // GIVEN a server.
        let attr = ServerAttributes::builder()
            .server_name(service_unit_name)
            .type_name(
                TypeName::builder()
                    .package_name(PACKAGE)
                    .bundle_name(bundle_name)
                    .type_name(TYPE_NAME)
                    .build()
                    .unwrap(),
            )
            .server_options(ServerOptions::default())
            .build()
            .unwrap();

        let methods = MethodMapBuilder::new()
            .add_server_streaming_method::<Pressure, Pressure>(&method_name, {
                std::sync::Arc::new(move |_fqin, req| {
                    // Responds with a stream of 3 `Pressure` messages. The pressure value in each
                    // message is one-third of the pressure from the incoming request.
                    Box::pin(async move {
                        let num = req.pressure / 3;

                        let respond_stream: Pin<
                            Box<dyn Stream<Item = SdvResult<Pressure>> + Send>,
                        > = futures::stream::iter(vec![
                            Ok(Pressure { pressure: num, ..Default::default() }),
                            Ok(Pressure { pressure: num, ..Default::default() }),
                            Ok(Pressure { pressure: num, ..Default::default() }),
                        ])
                        .boxed();
                        Ok(respond_stream)
                    })
                })
            })
            .build_rpc_methods();

        let _server = comms.create_server_transport_all_methods(&attr, methods).await.unwrap();

        // AND GIVEN a client to that server.
        let client = comms
            .create_client_transport(
                &ClientAttributes::builder().server_name(server_unit_name.clone()).build().unwrap(),
            )
            .await
            .unwrap();

        // WHEN calling the remote method
        let pressure = Pressure { pressure: 300, ..Default::default() };
        let mut response_stream =
            call_server_streaming_method::<Pressure, Pressure>(&*client, &method_name, &pressure)
                .await
                .unwrap();

        // THEN the response holds the expected value.
        for _ in 0..3 {
            let response = response_stream.next().await.unwrap().unwrap();
            assert_eq!(response.pressure, 100);
        }
        assert!(response_stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_pub_sub_publishing_batch_with_bigger_size_than_expected() {
        let bundle_name: &str = "PubsubMessageTooBig";
        let service_unit_name: &str = "variable-string-publication";

        // Let's remind that VariableString message uses 6 bytes
        // 1. byte for the field index
        // 1. byte for the actual length of the string (<=255)
        // 4. bytes for the payload itself
        // total message size = 6 bytes
        //
        // On top of that there is an additional byte to notify during
        // decode operation the size of the message to decode.
        // So every message slot on the FMQ is 7 bytes.
        // Therefore 2 messages are mapped to 14 bytes.
        let message_size = 6;

        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("{VM}:{PACKAGE}.{bundle_name}/sample"))
                .expect("ServiceFqin should not be malformed"),
        );
        let comms = Arc::new(SdvComms { context });

        // GIVEN a publisher.
        let pub_metadata = PublisherMetadata::builder()
            .message_count(10)
            .message_size(message_size)
            .build()
            .unwrap();

        let publisher = new_publisher::<VariableString>(
            comms.clone(),
            service_unit_name,
            None,
            None,
            pub_metadata,
        )
        .await;

        // WHEN publishing two message with total size
        // being 2 * (MESSAGE_SIZE + header_size).
        let result = publisher.publish_batch(&[
            // First message size is 8 bytes.
            // 7 bytes of payload
            // 1 extra byte for the varint header
            VariableString { description: "12345".to_string(), ..Default::default() },
            // Second message size is 6 bytes
            // 5 bytes of payload
            // 1 extra byte for the varint header
            VariableString { description: "123".to_string(), ..Default::default() },
        ]);

        // THEN an error is returned if any message is bigger than MESSAGE_SIZE
        assert!(result.is_err());
        let _ = result.inspect_err(|e| {
            assert_eq!(e.status_code(), SdvStatusCode::InvalidArgument);
            assert!(e.to_string().contains("exceeds maximum allowed size"));
        });
    }

    #[tokio::test]
    async fn test_sdv_pub_sub_topic() {
        // Topics depend on a SD listener that needs a running binder thread.
        binder::ProcessState::start_thread_pool();
        let bundle_name: &str = "PubsubTopic";
        let topic_name: &str = "pressure-topic";
        let type_name: &str = "TopicPressure";

        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("{VM}:{PACKAGE}.{bundle_name}/sample"))
                .expect("ServiceFqin should not be malformed"),
        );
        let comms = Arc::new(SdvComms { context });

        // GIVEN two publishers on the same topic.
        let pub_one = new_pressure_publisher(
            comms.clone(),
            "pub-one",
            Some(type_name),
            Some(topic_name),
            PublisherMetadata::builder().message_count(10).message_size(5).build().unwrap(),
        )
        .await;
        let pub_two = new_pressure_publisher(
            comms.clone(),
            "pub-two",
            Some(type_name),
            Some(topic_name),
            PublisherMetadata::builder().message_count(10).message_size(5).build().unwrap(),
        )
        .await;

        // AND GIVEN a subscriber on the same topic.
        let fq_type_name = TypeName::builder()
            .package_name(PACKAGE)
            .bundle_name(OWNING_BUNDLE_NAME)
            .type_name(type_name)
            .build()
            .unwrap();
        let subscriber = Subscriber::<Pressure>::new(
            comms
                .create_subscribe_handle(&fq_type_name, topic_name, &SubscribeOptions::default())
                .await
                .unwrap(),
        );

        // AND GIVEN an observer on the same topic.
        let mut observer = observe::<Pressure>(
            comms
                .create_message_stream(&fq_type_name, topic_name, &SubscribeOptions::default())
                .await
                .unwrap(),
        );

        // AND GIVEN a history on the same topic.
        const HISTORY_BUFFER_SIZE: u32 = 2;
        let history = History::<Pressure>::new(
            comms
                .create_history_handle(
                    &fq_type_name,
                    topic_name,
                    &SubscribeOptions::builder()
                        .message_count(HISTORY_BUFFER_SIZE)
                        .build()
                        .unwrap(),
                )
                .await
                .unwrap(),
        );

        // Needed for the topic reader to be created on a separate process.
        tokio::time::sleep(TOPIC_CREATION_DURATION).await;

        // WHEN messages are published from both publishers.
        pub_one.publish(&Pressure { pressure: 1234, ..Default::default() }).unwrap();

        // Make sure History copy data in the same order
        tokio::time::sleep(TOPIC_CREATION_DURATION).await;
        pub_two.publish(&Pressure { pressure: 5678, ..Default::default() }).unwrap();

        // THEN the subscriber can read messages from both publishers.
        let mut read = subscriber.read_next_messages(2).unwrap();
        assert_eq!(read.len(), 2);
        // Ordering of messages from different contributing topics is not guaranteed.
        read.sort_by_key(|x| x.pressure);
        assert_eq!(read[0].pressure, 1234);
        assert_eq!(read[1].pressure, 5678);

        // AND THEN the observer can receive messages from both publishers.
        let mut received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 2);
        received.sort_by_key(|x| x.pressure);
        assert_eq!(received[0].pressure, 1234);
        assert_eq!(received[1].pressure, 5678);

        // THEN one can read from history.
        // SLEEP to ensure the message is forwarded to the history buffer.
        tokio::time::sleep(HISTORY_PROCESSING_DURATION).await;
        let history_read_batch = history.read_from_history(HISTORY_BUFFER_SIZE).unwrap();
        assert_eq!(history_read_batch.len(), HISTORY_BUFFER_SIZE as usize);
        assert_eq!(history_read_batch[0].pressure, 1234);
        assert_eq!(history_read_batch[1].pressure, 5678);

        // WHEN a new message is published
        pub_two.publish(&Pressure { pressure: 5678, ..Default::default() }).unwrap();

        // SLEEP to ensure the message is forwarded to the history buffer.
        tokio::time::sleep(HISTORY_PROCESSING_DURATION).await;
        let history_read_batch = history.read_from_history(HISTORY_BUFFER_SIZE + 1).unwrap();
        assert_eq!(history_read_batch.len(), HISTORY_BUFFER_SIZE as usize);
        assert_eq!(history_read_batch[0].pressure, 5678);
        assert_eq!(history_read_batch[1].pressure, 5678);
    }
}
