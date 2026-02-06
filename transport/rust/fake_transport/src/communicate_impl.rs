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

#![allow(missing_docs)]
/// In-Process Communicate Implementation
///
/// WARNING: this is an experimental sample implementation of the sdv_mw_rs_comms traits. It doesn't reflect
/// the exact behavior of SdvComms. It's a naive implementation of a (process-)local comms stack in order to
/// compile, run, and test the sdv_mw_rs_comms crate.
use crate::{
    pubsub_impl::{LocalPublication, LocalSubscriber},
    {LocalMessageQueue, LocalPublisher},
};
use async_trait::async_trait;
use futures::{channel::mpsc, Stream};
use sdv_comms::rpc::{RpcMethod, UnaryRpcMethod};
use sdv_mw_rs_comms::{
    Availability, ByteBuffersStream, ClientAttributes, ClientTransport, Communicate, Publication,
    Publish, PublisherAttributes, ServerAttributes, ServerTransport, Subscribe, SubscribeOptions,
    TypeName, UnitQuery,
};
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::{
    collections::{HashMap, HashSet},
    pin::Pin,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct LocalComms {
    // Channel -> MessageQueue
    msg_queues: Arc<Mutex<HashMap<String, Arc<Mutex<LocalMessageQueue>>>>>,
    // Channel -> List of Listener
    registration_streams: Arc<Mutex<HashMap<String, Vec<mpsc::Sender<Availability>>>>>,
    // Channel -> Set of PublishersName
    publishers: Arc<Mutex<HashMap<String, HashSet<String>>>>,
}

impl LocalComms {
    pub fn new() -> Self {
        Self {
            msg_queues: Arc::new(Mutex::new(HashMap::new())),
            registration_streams: Arc::new(Mutex::new(HashMap::new())),
            publishers: Arc::new(Mutex::new(HashMap::new())), // Initialize the publishers map
        }
    }

    pub fn send_availability_event(&self, topic_name: &str, availability: Availability) {
        let mut registration_streams = self.registration_streams.lock().unwrap();
        if let Some(streams) = registration_streams.get_mut(topic_name) {
            streams.retain_mut(|tx| tx.try_send(availability.clone()).is_ok());
        }
    }

    pub fn publisher_added(&self, topic_name: &str, publisher_name: &str) {
        let mut publishers = self.publishers.lock().unwrap();
        publishers.entry(topic_name.to_string()).or_default().insert(publisher_name.to_string());
        self.send_availability_event(topic_name, Availability::Available);
    }

    pub fn publisher_removed(&self, topic_name: &str, publisher_name: &str) {
        let mut publishers = self.publishers.lock().unwrap();
        let mut channel_empty = false;
        if let Some(channel_publishers) = publishers.get_mut(topic_name) {
            channel_publishers.remove(publisher_name);
            if channel_publishers.is_empty() {
                channel_empty = true;
            }
        }

        self.send_availability_event(topic_name, Availability::Unavailable);
        if channel_empty {
            publishers.remove(topic_name);
            let mut msg_queues = self.msg_queues.lock().unwrap();
            msg_queues.remove(topic_name);
        }
    }

    pub fn find_channel_queue_for_publisher(&self, publisher_name: &str) -> Option<String> {
        let publishers = self.publishers.lock().unwrap();
        let mut topic_name = None;
        for (channel, pubs) in publishers.iter() {
            if pubs.contains(publisher_name) {
                topic_name = Some(channel.clone());
                break;
            }
        }

        topic_name
    }
}

impl Default for LocalComms {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Communicate for LocalComms {
    async fn create_publish(
        &self,
        attributes: &PublisherAttributes,
    ) -> SdvResult<Box<dyn Publish>> {
        let topic_name = attributes.get_topic_name().to_string();
        let publisher_name = attributes.get_publication_name().to_string();

        let mut msg_queues = self.msg_queues.lock().unwrap();
        let msg_q = msg_queues
            .entry(topic_name.clone())
            .or_insert_with(|| Arc::new(Mutex::new(LocalMessageQueue::new())));

        // Let's assume the first publisher sets the queue size.
        if self.publishers.lock().unwrap().get(&topic_name).map_or(true, |s| s.is_empty()) {
            msg_q.lock().unwrap().register_publisher(attributes.get_metadata().get_message_count());
        }

        self.publisher_added(&topic_name, &publisher_name);

        Ok(Box::new(LocalPublisher::new(
            Arc::new(self.clone()), // Clone the Arc to create a new reference
            msg_q.clone(),
            topic_name,
            publisher_name,
        )))
    }

    #[allow(useless_deprecated)]
    #[allow(unused_attributes)]
    #[deprecated(
        since = "2.0.0",
        note = "Channel-based subscribe methods are called instead, e.g. `create_subscribe_handle`"
    )]
    async fn find_publication(&self, attributes: &UnitQuery) -> SdvResult<Arc<dyn Publication>> {
        let publication_name = match &attributes {
            UnitQuery::Name(name) => name.get_service_unit_name().to_owned(),
            UnitQuery::Type(name) => name.to_string(),
            _ => {
                return Err(SdvStatus::with_message(
                    SdvStatusCode::Unimplemented,
                    "UnitQuery variant not handled.",
                ))
            }
        };

        let topic_name = self.find_channel_queue_for_publisher(&publication_name);

        if let Some(topic_name) = topic_name {
            let msg_queues = self.msg_queues.lock().unwrap();
            match msg_queues.get(&topic_name) {
                Some(q) => Ok(Arc::new(LocalPublication::new(q.clone()))),
                None => Err(SdvStatus::new(SdvStatusCode::Unavailable)),
            }
        } else {
            Err(SdvStatus::new(SdvStatusCode::Unavailable))
        }
    }

    async fn create_registration_event_stream(
        &self,
        attributes: &UnitQuery,
    ) -> SdvResult<Pin<Box<dyn Stream<Item = Availability> + Send>>> {
        let (mut tx, rx): (mpsc::Sender<Availability>, mpsc::Receiver<Availability>) =
            mpsc::channel(1);
        let publication_name = match attributes {
            UnitQuery::Name(name) => name.get_service_unit_name().to_owned(),
            UnitQuery::Type(name) => name.to_string(),
            _ => {
                return Err(SdvStatus::with_message(
                    SdvStatusCode::Unimplemented,
                    "UnitQuery variant not handled.",
                ))
            }
        };

        let mut topic_name = self.find_channel_queue_for_publisher(&publication_name);

        let publishers = self.publishers.lock().unwrap();
        let availability = if let Some(ref channel) = topic_name {
            if publishers.get(channel).is_some_and(|s| !s.is_empty()) {
                Availability::Available
            } else {
                Availability::Unavailable
            }
        } else {
            Availability::Unavailable
        };

        let _ = tx.try_send(availability);

        if topic_name.is_none() {
            // if the channel does not exist, let's register this on the test default channel
            topic_name = Some("pressure-channel".to_string())
        }
        let topic_name = topic_name.expect("Expecting channel name to be set");
        self.registration_streams.lock().unwrap().entry(topic_name).or_default().push(tx);

        Ok(Box::pin(rx))
    }

    #[allow(useless_deprecated)]
    #[allow(unused_attributes)]
    #[deprecated(
        since = "2.0.0",
        note = "Channel-based subscribe methods are called instead, e.g. `create_subscribe_handle`"
    )]
    async fn subscribe_by_type(
        &self,
        _type_name: &TypeName,
        _options: &SubscribeOptions,
    ) -> SdvResult<Box<dyn Subscribe>> {
        unimplemented!("subscribe by type not implemented")
    }

    async fn create_subscribe_handle(
        &self,
        _type_name: &TypeName,
        topic_name: &str,
        options: &SubscribeOptions,
    ) -> SdvResult<Box<dyn Subscribe>> {
        let msg_queues = self.msg_queues.lock().unwrap();
        match msg_queues.get(topic_name) {
            Some(msg_q_arc) => {
                let id = msg_q_arc.lock().unwrap().subscribe(options);
                Ok(Box::new(LocalSubscriber::new(msg_q_arc.clone(), id)))
            }
            None => Err(SdvStatus::new(SdvStatusCode::Unavailable)),
        }
    }

    async fn observe_by_type(
        &self,
        _type_name: &TypeName,
        _options: &SubscribeOptions,
    ) -> SdvResult<ByteBuffersStream> {
        unimplemented!("observe by type not implemented")
    }

    async fn create_message_stream(
        &self,
        _type_name: &TypeName,
        topic_name: &str,
        _options: &SubscribeOptions,
    ) -> SdvResult<ByteBuffersStream> {
        let msg_queues = self.msg_queues.lock().unwrap();
        match msg_queues.get(topic_name) {
            Some(msg_q_arc) => {
                let stream = msg_q_arc.lock().unwrap().observe();
                Ok(stream)
            }
            None => Err(SdvStatus::new(SdvStatusCode::Unavailable)),
        }
    }

    async fn create_history_handle(
        &self,
        _type_name: &TypeName,
        topic_name: &str,
        options: &SubscribeOptions,
    ) -> SdvResult<Box<dyn sdv_mw_rs_comms::ReadHistory>> {
        let msg_queues = self.msg_queues.lock().unwrap();
        match msg_queues.get(topic_name) {
            Some(msg_q_arc) => {
                let id = msg_q_arc.lock().unwrap().enable_history(options.get_message_count());
                Ok(Box::new(crate::pubsub_impl::LocalHistory::new(msg_q_arc.clone(), id)))
            }
            None => Err(SdvStatus::new(SdvStatusCode::Unavailable)),
        }
    }

    async fn create_server_transport(
        &self,
        _attributes: &ServerAttributes,
        _method_map: HashMap<String, UnaryRpcMethod>,
    ) -> SdvResult<Box<dyn ServerTransport>> {
        unimplemented!("Implement client transport for Local Comms")
    }

    async fn create_server_transport_all_methods(
        &self,
        _attributes: &ServerAttributes,
        _method_map: HashMap<String, RpcMethod>,
    ) -> SdvResult<Box<dyn ServerTransport>> {
        unimplemented!("Implement client transport for Local Comms")
    }

    async fn create_client_transport(
        &self,
        _attributes: &ClientAttributes,
    ) -> SdvResult<Box<dyn ClientTransport>> {
        unimplemented!("Implement client transport for Local Comms")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::StreamExt;
    use sdv_mw_commstests_protos::test_messages::Pressure;
    use sdv_mw_rs_comms::{
        observe, History, InstantReader, Publisher, PublisherMetadata, SubscribeOptions,
        Subscriber, TypeName, UnitName,
    };

    fn assert_send<T: Send>(_t: &T) {}
    fn assert_sync<T: Sync>(_t: &T) {}

    const PACKAGE: &str = "sdv.package";
    const VM: &str = "vm-name";
    const BUNDLE: &str = "Bundle";
    const TYPE: &str = "Pressure";
    static DEFAULT_PUBLISHER_NAME: &str = "pressure-publication";
    static DEFAULT_TOPIC_NAME: &str = "pressure-channel";
    const DEFAULT_COUNT: u32 = 10;

    fn get_pressure_unit_name() -> UnitName {
        UnitName::builder()
            .vm_name(VM)
            .package_name(PACKAGE)
            .bundle_name(BUNDLE)
            .service_unit_name(DEFAULT_PUBLISHER_NAME)
            .build()
            .unwrap()
    }

    fn get_pressure_type_name() -> TypeName {
        TypeName::builder()
            .package_name(PACKAGE)
            .bundle_name(BUNDLE)
            .type_name(TYPE)
            .build()
            .unwrap()
    }

    fn get_pressure_pub_attr(publisher_name: &str, message_count: u32) -> PublisherAttributes {
        let pub_metadata =
            PublisherMetadata::builder().message_count(message_count).build().unwrap();
        PublisherAttributes::builder()
            .publication_name(publisher_name)
            .type_name(get_pressure_type_name())
            .metadata(pub_metadata)
            .topic_name(DEFAULT_TOPIC_NAME)
            .build()
            .unwrap()
    }

    fn query_args() -> UnitQuery {
        UnitQuery::Name(get_pressure_unit_name())
    }

    #[tokio::test]
    async fn test_publish_before_subscribe() {
        let comms = LocalComms::new();
        let publisher = Publisher::<Pressure>::new(
            comms
                .create_publish(&get_pressure_pub_attr(DEFAULT_PUBLISHER_NAME, DEFAULT_COUNT))
                .await
                .unwrap(),
        );

        // GIVEN a message is published before calling `subscribe`
        let mut pressure_msg_1 = Pressure::new();
        pressure_msg_1.pressure = 1234;
        let _ = publisher.publish(&pressure_msg_1);

        // WHEN the subscriber subscribes and reads_next_message
        let subscriber = Subscriber::<Pressure>::new(
            comms
                .create_subscribe_handle(
                    &get_pressure_type_name(),
                    DEFAULT_TOPIC_NAME,
                    &SubscribeOptions::default(),
                )
                .await
                .unwrap(),
        );
        // THEN no messages are returned
        assert!(subscriber.read_next_messages(1).unwrap().is_empty());

        // WHEN another message is published
        let mut pressure_msg_2 = Pressure::new();
        pressure_msg_2.pressure = 2345;
        let _ = publisher.publish(&pressure_msg_2);

        // THEN subscriber can read that message
        assert_eq!(subscriber.read_next_messages(1).unwrap()[0].pressure, 2345);
    }

    #[tokio::test]
    async fn test_read_published_messages() {
        let comms = LocalComms::new();
        let publisher = Publisher::<Pressure>::new(
            comms
                .create_publish(&get_pressure_pub_attr(DEFAULT_PUBLISHER_NAME, DEFAULT_COUNT))
                .await
                .unwrap(),
        );
        let subscriber = Subscriber::<Pressure>::new(
            comms
                .create_subscribe_handle(
                    &get_pressure_type_name(),
                    DEFAULT_TOPIC_NAME,
                    &SubscribeOptions::default(),
                )
                .await
                .unwrap(),
        );

        // GIVEN we publish 3 messages:
        let mut pressure_msg_1 = Pressure::new();
        pressure_msg_1.pressure = 1234;

        let mut pressure_msg_2 = Pressure::new();
        pressure_msg_2.pressure = 2345;

        let mut pressure_msg_3 = Pressure::new();
        pressure_msg_3.pressure = 3456;

        let _ = publisher.publish_batch(&[pressure_msg_1, pressure_msg_2, pressure_msg_3]);

        // WHEN reading the next two messages;
        let messages = subscriber.read_next_messages(2).unwrap();

        // THEN msg_1 and msg_2 are returned
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].pressure, 1234);
        assert_eq!(messages[1].pressure, 2345);

        // WHEN reading another two messages:
        let messages = subscriber.read_next_messages(2).unwrap();

        // THEN only msg_3 is returned;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].pressure, 3456);
    }

    #[tokio::test]
    async fn test_read_latest_message() {
        let comms = LocalComms::new();
        let publisher = Publisher::<Pressure>::new(
            comms
                .create_publish(&get_pressure_pub_attr(DEFAULT_PUBLISHER_NAME, DEFAULT_COUNT))
                .await
                .unwrap(),
        );

        // GIVEN 2 messages are published from a publisher:
        let mut pressure_msg_1 = Pressure::new();
        pressure_msg_1.pressure = 1234;

        let mut pressure_msg_2 = Pressure::new();
        pressure_msg_2.pressure = 2345;

        let _ = publisher.publish_batch(&[pressure_msg_1, pressure_msg_2]);

        // WHEN creating a subscriber (not calling .subscribe()):
        let publication = comms.find_publication(&query_args()).await.unwrap();
        let reader = InstantReader::<Pressure>::new(publication.create_instant_read());

        // AND WHEN reading the latest message:
        // THEN msg_2 is returned:
        assert_eq!(reader.read_latest_message().await.unwrap().unwrap().pressure, 2345);
    }

    #[tokio::test]
    async fn test_read_from_history() {
        let comms = LocalComms::new();
        // GIVEN A Publisher with message_count (queue size) of 1
        const PUBLICATION_QUEUE_SIZE: u32 = 1;
        let publisher = Publisher::<Pressure>::new(
            comms
                .create_publish(&get_pressure_pub_attr(
                    DEFAULT_PUBLISHER_NAME,
                    PUBLICATION_QUEUE_SIZE,
                ))
                .await
                .unwrap(),
        );

        // WHEN we enable subscriber history with a message count of 3:
        const HISTORY_MESSAGE_COUNT: u32 = 3;
        let publication = comms.find_publication(&query_args()).await.unwrap();
        let history = History::<Pressure>::new(
            publication
                .create_history(
                    &SubscribeOptions::builder()
                        .message_count(HISTORY_MESSAGE_COUNT)
                        .build()
                        .unwrap(),
                )
                .await
                .unwrap(),
        );

        // AND WHEN we publish 3 messages
        let mut pressure_msg_1 = Pressure::new();
        pressure_msg_1.pressure = 1234;

        let mut pressure_msg_2 = Pressure::new();
        pressure_msg_2.pressure = 2345;

        let mut pressure_msg_3 = Pressure::new();
        pressure_msg_3.pressure = 3456;

        let _ = publisher.publish_batch(&[pressure_msg_1, pressure_msg_2, pressure_msg_3]);

        // THEN we can read 3 messages from history;
        let messages = history.read_from_history(HISTORY_MESSAGE_COUNT).unwrap();
        assert_eq!(messages.len(), HISTORY_MESSAGE_COUNT as usize);
        assert_eq!(messages[2].pressure, 3456);

        // AND when we read again 1 message from history:
        let messages = history.read_from_history(1).unwrap();
        //THEN msg_3 is returned:
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].pressure, 3456);
    }

    #[tokio::test]
    async fn test_data_on_subscribe() {
        let comms = LocalComms::new();
        let publisher = Publisher::<Pressure>::new(
            comms
                .create_publish(&get_pressure_pub_attr(DEFAULT_PUBLISHER_NAME, DEFAULT_COUNT))
                .await
                .unwrap(),
        );

        // GIVEN 2 messages are published from a publisher:
        let mut pressure_msg_1 = Pressure::new();
        pressure_msg_1.pressure = 1234;

        let mut pressure_msg_2 = Pressure::new();
        pressure_msg_2.pressure = 2345;

        let _ = publisher.publish_batch(&[pressure_msg_1, pressure_msg_2]);

        // WHEN creating a subscriber and calling .subscribe with data_on_subscribe enabled:
        let subscriber = Subscriber::<Pressure>::new(
            comms
                .create_subscribe_handle(
                    &get_pressure_type_name(),
                    DEFAULT_TOPIC_NAME,
                    &SubscribeOptions::builder().read_on_subscribe(true).build().unwrap(),
                )
                .await
                .unwrap(),
        );

        // AND WHEN reading the next 2 messages:
        let messages = subscriber.read_next_messages(2).unwrap();

        // THEN msg_2 is returned:
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].pressure, 2345);
    }

    #[tokio::test]
    async fn test_observer() {
        let comms = LocalComms::new();
        let publisher = Publisher::<Pressure>::new(
            comms
                .create_publish(&get_pressure_pub_attr(DEFAULT_PUBLISHER_NAME, DEFAULT_COUNT))
                .await
                .unwrap(),
        );

        // GIVEN we observe a publication:
        let publication = comms.find_publication(&query_args()).await.unwrap();
        let mut typed_stream =
            observe::<Pressure>(publication.observe(&SubscribeOptions::default()).await.unwrap());

        // GIVEN 2 messages are published from a publisher:
        let mut pressure_msg_1 = Pressure::new();
        pressure_msg_1.pressure = 1234;

        let mut pressure_msg_2 = Pressure::new();
        pressure_msg_2.pressure = 2345;

        let _ = publisher.publish_batch(&[pressure_msg_1, pressure_msg_2]);

        // WHEN creating a subscriber and calling .subscribe with data_on_subscribe enabled:
        let messages = typed_stream.next().await.unwrap().unwrap();

        // THEN msg_2 is returned:
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].pressure, 1234);
    }

    #[tokio::test]
    async fn test_sdv_local_pub_sub_e2e() {
        let comms = LocalComms::new();

        // WHEN create registration event stream
        let mut registration_event_stream_created_at_start = comms
            .create_registration_event_stream(&UnitQuery::Name(get_pressure_unit_name()))
            .await
            .unwrap();
        // THEN unavailable before the publisher was created
        assert_eq!(
            registration_event_stream_created_at_start.next().await,
            Some(Availability::Unavailable)
        );

        // WHEN finding a publication that doesn't exist.
        let failed_publication_res =
            comms.find_publication(&UnitQuery::Name(get_pressure_unit_name())).await;

        // THEN the return error.
        assert!(failed_publication_res.is_err());

        // WHEN creating a publisher using SdvComms.
        let publisher = Publisher::<Pressure>::new(
            comms
                .create_publish(&get_pressure_pub_attr(DEFAULT_PUBLISHER_NAME, DEFAULT_COUNT))
                .await
                .unwrap(),
        );
        // THEN the publisher is `Send`.
        assert_send(&publisher);

        // WHEN finding a publication.
        let publication =
            comms.find_publication(&UnitQuery::Name(get_pressure_unit_name())).await.unwrap();

        // THEN the publication is `Send + Sync`.
        assert_send(&publication);
        assert_sync(&publication);
        // and available after the publisher was created
        assert_eq!(
            registration_event_stream_created_at_start.next().await,
            Some(Availability::Available)
        );

        // WHEN create registration event stream after the publisher was created
        let mut registration_event_stream_created_after_publisher = comms
            .create_registration_event_stream(&UnitQuery::Name(get_pressure_unit_name()))
            .await
            .unwrap();
        // THEN available
        assert_eq!(
            registration_event_stream_created_after_publisher.next().await,
            Some(Availability::Available)
        );

        // WHEN creating a subscriber using SdvComms.
        let subscriber = Subscriber::<Pressure>::new(
            comms
                .create_subscribe_handle(
                    &get_pressure_type_name(),
                    DEFAULT_TOPIC_NAME,
                    &SubscribeOptions::default(),
                )
                .await
                .unwrap(),
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

        // AND WHEN a message is published.
        let mut pressure_msg = Pressure::new();
        pressure_msg.pressure = 1234;
        publisher.publish(&pressure_msg).unwrap();

        // THEN subscriber can read messages.
        let read = subscriber.read_next_messages(1).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].pressure, 1234);

        // THEN observer can receive a message.
        let received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].pressure, 1234);

        // THEN one can read from history.
        let read = history.read_from_history(1).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].pressure, 1234);
        publisher.publish(&pressure_msg).unwrap();

        // THEN subscriber can read messages.
        let read = subscriber.read_next_messages(1).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].pressure, 1234);

        // THEN observer can receive a message.
        let received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].pressure, 1234);

        // THEN one can read from history.
        let read = history.read_from_history(1).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].pressure, 1234);

        // THEN one can instant-read.
        let read = instant_reader.read_latest_message().await.unwrap().unwrap();
        assert_eq!(read.pressure, 1234);

        // Dropping the publisher to test for the `Unavailable` event
        drop(publisher);

        // THEN the channel should become Unavailable
        assert_eq!(
            registration_event_stream_created_at_start.next().await,
            Some(Availability::Unavailable)
        );
        assert_eq!(
            registration_event_stream_created_after_publisher.next().await,
            Some(Availability::Unavailable)
        );

        // WHEN create registration event stream after the publisher was dropped
        let mut registration_event_stream_created_after_publisher_dropped = comms
            .create_registration_event_stream(&UnitQuery::Name(get_pressure_unit_name()))
            .await
            .unwrap();
        // THEN unavailable
        assert_eq!(
            registration_event_stream_created_after_publisher_dropped.next().await,
            Some(Availability::Unavailable)
        );
    }

    #[tokio::test]
    async fn test_multiple_publishers_same_channel() {
        let comms = LocalComms::new();

        // GIVEN two publishers on the same channel
        let publisher1 = Publisher::<Pressure>::new(
            comms.create_publish(&get_pressure_pub_attr("pub1", DEFAULT_COUNT)).await.unwrap(),
        );
        let publisher2 = Publisher::<Pressure>::new(
            comms.create_publish(&get_pressure_pub_attr("pub2", DEFAULT_COUNT)).await.unwrap(),
        );

        // AND WHEN a subscriber is created
        let subscriber = Subscriber::<Pressure>::new(
            comms
                .create_subscribe_handle(
                    &get_pressure_type_name(),
                    DEFAULT_TOPIC_NAME,
                    &SubscribeOptions::default(),
                )
                .await
                .unwrap(),
        );

        // AND WHEN an observer is created
        let mut observer = observe::<Pressure>(
            comms
                .create_message_stream(
                    &get_pressure_type_name(),
                    DEFAULT_TOPIC_NAME,
                    &SubscribeOptions::default(),
                )
                .await
                .unwrap(),
        );

        // AND WHEN an history reader is created
        const HISTORY_MESSAGE_COUNT: u32 = 3;
        let history = History::<Pressure>::new(
            comms
                .create_history_handle(
                    &get_pressure_type_name(),
                    DEFAULT_TOPIC_NAME,
                    &SubscribeOptions::builder()
                        .message_count(HISTORY_MESSAGE_COUNT)
                        .build()
                        .unwrap(),
                )
                .await
                .unwrap(),
        );

        // WHEN publisher1 publishes a message
        let mut pressure_msg_1 = Pressure::new();
        pressure_msg_1.pressure = 1234;
        let _ = publisher1.publish(&pressure_msg_1);

        // THEN the subscriber can read the message
        let messages = subscriber.read_next_messages(1).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].pressure, 1234);

        // THEN the observer can receive the message
        let messages = observer.next().await.unwrap().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].pressure, 1234);

        // AND WHEN publisher2 publishes a message
        let mut pressure_msg_2 = Pressure::new();
        pressure_msg_2.pressure = 5678;
        let _ = publisher2.publish(&pressure_msg_2);

        // THEN the subscriber can read the message
        let messages = subscriber.read_next_messages(1).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].pressure, 5678);

        // THEN the observer can receive the message
        let messages = observer.next().await.unwrap().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].pressure, 5678);

        // THEN we can read 2 messages from history;
        let messages = history.read_from_history(HISTORY_MESSAGE_COUNT).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].pressure, 1234);
        assert_eq!(messages[1].pressure, 5678);

        pressure_msg_2.pressure = 9012;
        let _ = publisher2.publish(&pressure_msg_2);

        pressure_msg_1.pressure = 3456;
        let _ = publisher1.publish(&pressure_msg_1);

        // THEN we can read 3 messages from history;
        let messages = history.read_from_history(HISTORY_MESSAGE_COUNT).unwrap();
        assert_eq!(messages.len(), HISTORY_MESSAGE_COUNT as usize);
        assert_eq!(messages[0].pressure, 5678);
        assert_eq!(messages[1].pressure, 9012);
        assert_eq!(messages[2].pressure, 3456);
    }
}
