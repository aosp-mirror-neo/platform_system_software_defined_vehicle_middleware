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

#[cfg(test)]
mod test {
    // rust
    use futures::StreamExt;
    use std::str::FromStr;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    // sdv
    use sdv::comms::id::ServiceFqin;
    use sdv::comms::ContextRef;
    use sdv::mw::clientlib;
    use sdv::mw::{SdvComms, SubscribeOptions};
    use sdv::status::SdvStatusCode;
    // test catalog
    use sdv_mw_rs_sdv_vsidl_pubsub_test_publishing_bundle::publisher;
    use sdv_mw_rs_sdv_vsidl_pubsub_test_subscribing_bundle::subscriber;
    use vsidl_pubsub_test_proto::tire::TirePressure;

    // This is required by Datatunnel to guarantee forwarding the message to the history buffer.
    const TOPIC_PROCESSING_DURATION: Duration = Duration::from_millis(200);
    const HISTORY_PROCESSING_DURATION: Duration = Duration::from_millis(200);
    const PUBLISHER_DROP_SIGNAL_PROPAGATION: Duration = Duration::from_millis(500);
    const OBSERVER_TIMEOUT_DURATION: Duration = Duration::from_millis(200);
    const HISTORY_BUFFER_SIZE: u32 = 5;

    fn create_sdv_comms(instance_name: &str) -> Arc<SdvComms> {
        let context = ContextRef::create(
            ServiceFqin::from_str(
                format!("vm:sdv.vsidl.pubsub.test.PublishingBundle/{instance_name}").as_ref(),
            )
            .unwrap(),
        );
        Arc::new(SdvComms { context })
    }

    async fn setup(
        instance_name: &str,
        topic_num: &str,
    ) -> (
        Arc<SdvComms>,
        sdv::mw::Publisher<TirePressure>,
        std::pin::Pin<
            Box<
                dyn futures::stream::Stream<Item = sdv::status::SdvResult<Vec<TirePressure>>>
                    + Send,
            >,
        >,
        sdv::mw::Subscriber<TirePressure>,
        sdv::mw::History<TirePressure>,
        sdv::mw::InstantReader<TirePressure>,
    ) {
        let (pub_topic, sub_topic) = match topic_num {
            "topic-zero" => (
                publisher::Channel::<TirePressure>::TOPIC_ZERO,
                subscriber::Channel::<TirePressure>::TOPIC_ZERO,
            ),
            "topic-one" => (
                publisher::Channel::<TirePressure>::TOPIC_ONE,
                subscriber::Channel::<TirePressure>::TOPIC_ONE,
            ),
            "topic-two" => (
                publisher::Channel::<TirePressure>::TOPIC_TWO,
                subscriber::Channel::<TirePressure>::TOPIC_TWO,
            ),
            "topic-three" => (
                publisher::Channel::<TirePressure>::TOPIC_THREE,
                subscriber::Channel::<TirePressure>::TOPIC_THREE,
            ),
            "topic-four" => (
                publisher::Channel::<TirePressure>::TOPIC_FOUR,
                subscriber::Channel::<TirePressure>::TOPIC_FOUR,
            ),
            "topic-five" => (
                publisher::Channel::<TirePressure>::TOPIC_FIVE,
                subscriber::Channel::<TirePressure>::TOPIC_FIVE,
            ),
            "topic-six" => (
                publisher::Channel::<TirePressure>::TOPIC_SIX,
                subscriber::Channel::<TirePressure>::TOPIC_SIX,
            ),
            "topic-seven" => (
                publisher::Channel::<TirePressure>::TOPIC_SEVEN,
                subscriber::Channel::<TirePressure>::TOPIC_SEVEN,
            ),
            "topic-eight" => (
                publisher::Channel::<TirePressure>::TOPIC_EIGHT,
                subscriber::Channel::<TirePressure>::TOPIC_EIGHT,
            ),
            "topic-nine" => (
                publisher::Channel::<TirePressure>::TOPIC_NINE,
                subscriber::Channel::<TirePressure>::TOPIC_NINE,
            ),
            "topic-ten" => (
                publisher::Channel::<TirePressure>::TOPIC_TEN,
                subscriber::Channel::<TirePressure>::TOPIC_TEN,
            ),
            "topic-eleven" => (
                publisher::Channel::<TirePressure>::TOPIC_ELEVEN,
                subscriber::Channel::<TirePressure>::TOPIC_ELEVEN,
            ),
            "topic-twelve" => (
                publisher::Channel::<TirePressure>::TOPIC_TWELVE,
                subscriber::Channel::<TirePressure>::TOPIC_TWELVE,
            ),
            "topic-thirteen" => (
                publisher::Channel::<TirePressure>::TOPIC_THIRTEEN,
                subscriber::Channel::<TirePressure>::TOPIC_THIRTEEN,
            ),
            "topic-fourteen" => (
                publisher::Channel::<TirePressure>::TOPIC_FOURTEEN,
                subscriber::Channel::<TirePressure>::TOPIC_FOURTEEN,
            ),
            "topic-fifteen" | &_ => (
                publisher::Channel::<TirePressure>::TOPIC_FIFTEEN,
                subscriber::Channel::<TirePressure>::TOPIC_FIFTEEN,
            ),
        };

        setup_test(instance_name, pub_topic, sub_topic).await
    }

    async fn setup_test(
        instance_name: &str,
        pub_topic: &publisher::Channel<TirePressure>,
        sub_topic: &subscriber::Channel<TirePressure>,
    ) -> (
        Arc<SdvComms>,
        sdv::mw::Publisher<TirePressure>,
        std::pin::Pin<
            Box<
                dyn futures::stream::Stream<Item = sdv::status::SdvResult<Vec<TirePressure>>>
                    + Send,
            >,
        >,
        sdv::mw::Subscriber<TirePressure>,
        sdv::mw::History<TirePressure>,
        sdv::mw::InstantReader<TirePressure>,
    ) {
        // Channels depends on a SD listener that needs a running binder thread.
        binder::ProcessState::start_thread_pool();

        let sdv_comms = create_sdv_comms(instance_name);

        // GIVEN a publisher.
        let publisher = clientlib::create_publisher(sdv_comms.as_ref(), pub_topic)
            .await
            .expect("Create TirePressure publisher failed.");

        // AND GIVEN different the subscription options to that publisher.
        let subscribe_option =
            &SubscribeOptions::builder().instance_name(instance_name).build().unwrap();
        let observer = clientlib::create_observer(sdv_comms.as_ref(), sub_topic, subscribe_option)
            .await
            .expect("Create TirePressure observer failed.");
        let subscriber =
            clientlib::create_subscriber(sdv_comms.as_ref(), sub_topic, subscribe_option)
                .await
                .expect("Create TirePressure subscriber failed.");
        let instant_reader =
            clientlib::create_instant_reader(sdv_comms.as_ref(), sub_topic, subscribe_option)
                .await
                .expect("Create TirePressure instant_reader failed.");
        let history = clientlib::create_history(
            sdv_comms.as_ref(),
            sub_topic,
            &SubscribeOptions::builder()
                .message_count(HISTORY_BUFFER_SIZE)
                .instance_name(instance_name)
                .build()
                .unwrap(),
        )
        .await
        .expect("Create TirePressure history failed.");

        // Make sure publisher is registered on the channel.
        tokio::time::sleep(HISTORY_PROCESSING_DURATION).await;

        (sdv_comms, publisher, observer, subscriber, history, instant_reader)
    }

    #[tokio::test]
    async fn test_pub_sub_observer_before_publishing_data() {
        let instance_name = "pubsub-observer-before-data";

        // GIVEN Publisher and all the different subscription option
        let (_, publisher, mut observer, _, _, _) = setup(instance_name, "topic-zero").await;

        // WHEN we don't publish any data.
        // THEN observer does not get any data
        let result = tokio::time::timeout(OBSERVER_TIMEOUT_DURATION, observer.next()).await;
        assert!(result.is_err()); // Timeout

        // prevent Rust to drop the unused publisher before time.
        drop(publisher)
    }

    #[tokio::test]
    async fn test_pub_sub_subscriber_before_publishing_data() {
        let instance_name = "pubsub-subscriber-before-data";
        // GIVEN Publisher and all the different subscription option
        let (_, publisher, _, subscriber, _, _) = setup(instance_name, "topic-one").await;

        // WHEN we don't publish any data.
        // THEN subscriber can't retrieve any message
        let read = subscriber.read_next_messages(1).unwrap();
        assert_eq!(read.len(), 0);

        // prevent Rust to drop the unused publisher before time.
        drop(publisher)
    }

    #[tokio::test]
    async fn test_pub_sub_history_before_publishing_data() {
        let instance_name = "pubsub-history-before-data";
        // GIVEN Publisher and all the different subscription option
        let (_, publisher, _, _, history, _) = setup(instance_name, "topic-two").await;

        // WHEN we don't publish any data.
        // THEN history can't retrieve any message
        let read_history = history.read_from_history(1).unwrap();
        assert_eq!(read_history.len(), 0);

        // prevent Rust to drop the unused publisher before time.
        drop(publisher)
    }

    #[tokio::test]
    async fn test_pub_sub_instant_read_before_publishing_data() {
        let instance_name = "pubsub-instant-read-before-data";
        // GIVEN Publisher and all the different subscription option
        let (_, publisher, _, _, _, instant_reader) = setup(instance_name, "topic-three").await;

        // WHEN we don't publish any data.
        // THEN instant reader can't retrieve any message
        let instant_read = instant_reader.read_latest_message().await.unwrap();
        assert!(instant_read.is_none());
        // prevent Rust to drop the unused publisher before time.
        drop(publisher)
    }

    #[tokio::test]
    async fn test_pub_sub() {
        let instance_name = "pubsub";

        // GIVEN Publisher and all the different subscription option
        let (_, pressure_pub, mut observer, subscriber, history, instant_reader) =
            setup(instance_name, "topic-four").await;

        // THEN subscriber cannot retrieve any message.
        let read = subscriber.read_next_messages(1).unwrap();
        assert_eq!(read.len(), 0);

        // AND THEN history cannot retrieve any message.
        let read_history = history.read_from_history(1).unwrap();
        assert_eq!(read_history.len(), 0);

        // WHEN publishing a message.
        let pressure = 43678;
        pressure_pub
            .publish(&TirePressure { pressure, ..Default::default() })
            .expect("pressure_pub publishing failed.");

        // THEN we can observe the message.
        let received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].pressure, pressure);

        // AND THEN we can read the message.
        let read = subscriber.read_next_messages(1).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].pressure, pressure);

        // AND THEN we can instant read.
        let instant_read = instant_reader.read_latest_message().await.unwrap();
        assert!(instant_read.is_some());
        assert_eq!(instant_read.unwrap().pressure, pressure);

        // AND THEN we can read from history.
        // SLEEP to ensure the message is forwarded to the history buffer.
        thread::sleep(HISTORY_PROCESSING_DURATION);
        let read_history = history.read_from_history(1).unwrap();
        assert_eq!(read_history.len(), 1);
        assert_eq!(read_history[0].pressure, pressure);

        // AND THEN we cannot read more messages than available from history
        let read_history = history.read_from_history(2).unwrap();
        assert_eq!(read_history.len(), 1);
        assert_eq!(read_history[0].pressure, pressure);
    }

    #[tokio::test]
    async fn test_pub_sub_observer_create_before_publisher() {
        let instance_name = "observer-created-before-publisher";
        let sdv_comms = create_sdv_comms(instance_name);

        let subscribe_option =
            &SubscribeOptions::builder().instance_name(instance_name).build().unwrap();
        let observer = clientlib::create_observer(
            sdv_comms.as_ref(),
            subscriber::Channel::<TirePressure>::TOPIC_FIVE,
            subscribe_option,
        )
        .await;
        assert!(observer.is_err());
        let _ = observer.inspect_err(|e| assert_eq!(e.status_code(), SdvStatusCode::NotFound));
    }

    #[tokio::test]
    async fn test_pub_sub_subscriber_create_before_publisher() {
        let instance_name = "subscriber-created-before-publisher";
        let sdv_comms = create_sdv_comms(instance_name);

        let subscribe_option =
            &SubscribeOptions::builder().instance_name(instance_name).build().unwrap();
        let subscriber = clientlib::create_subscriber(
            sdv_comms.as_ref(),
            subscriber::Channel::<TirePressure>::TOPIC_SIX,
            subscribe_option,
        )
        .await;
        assert!(subscriber.is_err());
        let _ = subscriber.inspect_err(|e| assert_eq!(e.status_code(), SdvStatusCode::NotFound));
    }

    #[tokio::test]
    async fn test_pub_sub_history_create_before_publisher() {
        let instance_name = "history-created-before-publisher";
        let sdv_comms = create_sdv_comms(instance_name);

        let subscribe_option = &SubscribeOptions::builder()
            .message_count(HISTORY_BUFFER_SIZE)
            .instance_name(instance_name)
            .build()
            .unwrap();

        let history = clientlib::create_history(
            sdv_comms.as_ref(),
            subscriber::Channel::<TirePressure>::TOPIC_SEVEN,
            subscribe_option,
        )
        .await;
        assert!(history.is_err());
        let _ = history.inspect_err(|e| assert_eq!(e.status_code(), SdvStatusCode::NotFound));
    }

    #[tokio::test]
    async fn test_pub_sub_instant_read_create_before_publisher() {
        let instance_name = "instant-read-created-before-publisher";
        let sdv_comms = create_sdv_comms(instance_name);

        let instant_reader = clientlib::create_instant_reader(
            sdv_comms.as_ref(),
            subscriber::Channel::<TirePressure>::TOPIC_EIGHT,
            &SubscribeOptions::builder()
                .message_count(HISTORY_BUFFER_SIZE)
                .instance_name(instance_name)
                .build()
                .unwrap(),
        )
        .await;
        assert!(instant_reader.is_err());
        let _ =
            instant_reader.inspect_err(|e| assert_eq!(e.status_code(), SdvStatusCode::Unavailable));
    }

    #[tokio::test]
    async fn test_pub_sub_buffer_overflow() {
        let instance_name = "pubsub-overflow";

        // GIVEN Publisher and all the different subscription option
        let (_, pressure_pub, mut observer, subscriber, history, instant_reader) =
            setup(instance_name, "topic-nine").await;

        // WHEN publisher writes more message than buffer size ( buffer size is 8 )
        for n in 1..=10 {
            pressure_pub
                .publish(&TirePressure { pressure: 50000 + n, ..Default::default() })
                .expect("pressure_pub publishing failed.");
        }

        // THEN the observe receives a DataLoss error.
        let received = observer.next().await.unwrap();
        assert!(received.is_err());
        if let Err(e) = received {
            assert_eq!(e.status_code(), SdvStatusCode::DataLoss)
        }

        // AND THEN the observe receives the last message
        let received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 4);
        assert_eq!(received[0].pressure, 50007);
        assert_eq!(received[1].pressure, 50008);
        assert_eq!(received[2].pressure, 50009);
        assert_eq!(received[3].pressure, 50010);

        // AND THEN subscriber receives a buffer overflow error
        let read_error = subscriber.read_next_messages(1);
        match read_error {
            Ok(_) => panic!("expected a buffer overflow error"),
            Err(e) => {
                assert_eq!(e.status_code(), SdvStatusCode::DataLoss)
            }
        }

        // AND THEN subscriber receives half of the buffer size
        // Read will return the last 4 messages in this case (43685, 43686, 43687, 43688)
        let read = subscriber.read_next_messages(8).unwrap();
        assert_eq!(read.len(), 4);
        assert_eq!(read[0].pressure, 50007);
        assert_eq!(read[1].pressure, 50008);
        assert_eq!(read[2].pressure, 50009);
        assert_eq!(read[3].pressure, 50010);

        // AND THEN we can instant read.
        let instant_read = instant_reader.read_latest_message().await.unwrap();
        assert!(instant_read.is_some());
        assert_eq!(instant_read.unwrap().pressure, 50010);

        // AND THEN we can read from history.
        // SLEEP to ensure the message is forwarded to the history buffer.
        thread::sleep(HISTORY_PROCESSING_DURATION);
        // This test shows that we always return the latest valid consecutive written message with no gap.
        // A gap could be cause by the history buffer being to slow copy data from the publisher buffer.
        // When that happen, data tunnel resets the history buffer, so we never return wrong sequencing of data.
        //
        // Additional details:
        // History keeps the last HISTORY_BUFFER_SIZE valid messages received.
        // In case of an overflow however, the read buffer is reset and only the valid values after the overflow
        // will be accessible, in this example the last 4. The overflow scenario or not depends on how fast
        // the callback triggered by CommChannel for every new message will copy data from publisher buffer to
        // the history one.
        // If this copy is slow, then the callback will get a DataLoss error and the History buffer will be reset.
        // To avoid having a flaky test, we cover both scenarios.
        // If buffer overflow occurs only 4 values will be read, otherwise 5.
        let read_history = history.read_from_history(HISTORY_BUFFER_SIZE).unwrap();

        if read_history.len() == HISTORY_BUFFER_SIZE.try_into().unwrap() {
            // no overflow, then all written values has been correctly
            // read and those are the last 5.
            assert_eq!(read_history[0].pressure, 50006);
            assert_eq!(read_history[1].pressure, 50007);
            assert_eq!(read_history[2].pressure, 50008);
            assert_eq!(read_history[3].pressure, 50009);
            assert_eq!(read_history[4].pressure, 50010);
        } else {
            // overflow occurred and only 4 values are available on the buffer
            assert_eq!(read_history[0].pressure, 50007);
            assert_eq!(read_history[1].pressure, 50008);
            assert_eq!(read_history[2].pressure, 50009);
            assert_eq!(read_history[3].pressure, 50010);
        }
    }

    #[tokio::test]
    async fn test_pub_sub_observer_after_publisher_drop() {
        let instance_name = "pubsub-observer-after-publisher-drop";
        // GIVEN Publisher and all the different subscription option
        let (sdv_comms, pressure_pub, mut observer, _, _, _) =
            setup(instance_name, "topic-ten").await;

        // WHEN publishing a message.
        let pressure = 43678;
        pressure_pub
            .publish(&TirePressure { pressure, ..Default::default() })
            .expect("pressure_pub publishing failed.");

        // THEN we can observe the message.
        let received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].pressure, pressure);

        // WHEN the publisher is dropped
        drop(pressure_pub);

        // no further notification is received
        let result = tokio::time::timeout(OBSERVER_TIMEOUT_DURATION, observer.next()).await;
        assert!(result.is_err()); // Timeout

        // WHEN the publisher is re-created
        let publisher = clientlib::create_publisher(
            sdv_comms.as_ref(),
            publisher::Channel::<TirePressure>::TOPIC_TEN,
        )
        .await
        .expect("Create pressure_pub::TOPIC_TEN failed.");

        // Make sure publisher is registered on the topic.
        tokio::time::sleep(TOPIC_PROCESSING_DURATION).await;

        // WHEN publishing a new message.
        let pressure_after_recreation = 43679;
        publisher
            .publish(&TirePressure { pressure: pressure_after_recreation, ..Default::default() })
            .expect("pressure_pub publishing failed.");

        // THEN the observer's stream returns the new message
        let received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].pressure, pressure_after_recreation);
    }

    #[tokio::test]
    async fn test_pub_sub_subscriber_after_publisher_drop() {
        let instance_name = "pubsub-subscriber-after-publisher-drop";

        // GIVEN Publisher and all the different subscription option
        let (sdv_comms, pressure_pub, _, subscriber, _, _) =
            setup(instance_name, "topic-eleven").await;

        // WHEN publishing a message.
        let pressure = 43678;
        pressure_pub
            .publish(&TirePressure { pressure, ..Default::default() })
            .expect("pressure_pub publishing failed.");

        // THEN we can read the message.
        let read = subscriber.read_next_messages(1).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].pressure, pressure);

        // WHEN the publisher is dropped
        drop(pressure_pub);

        // TODO(b/409710102): Will allow the subscriber to receive an error
        // when publisher drop, but for now nothing new is retrieved.
        // THEN the subscriber receives no other messages
        let read = subscriber.read_next_messages(1).unwrap();
        assert_eq!(read.len(), 0);

        // AND THEN after all signal are propagated, subscriber reads no new messages.
        thread::sleep(PUBLISHER_DROP_SIGNAL_PROPAGATION);
        let read = subscriber.read_next_messages(1).unwrap();
        assert_eq!(read.len(), 0);

        // WHEN the publisher is re-created
        let publisher = clientlib::create_publisher(
            sdv_comms.as_ref(),
            publisher::Channel::<TirePressure>::TOPIC_ELEVEN,
        )
        .await
        .expect("Create pressure_pub::TOPIC_ELEVEN failed.");

        // Make sure publisher is registered on the topic.
        tokio::time::sleep(TOPIC_PROCESSING_DURATION).await;

        // WHEN publishing a new message.
        let pressure_after_recreation = 43679;
        publisher
            .publish(&TirePressure { pressure: pressure_after_recreation, ..Default::default() })
            .expect("pressure_pub publishing failed.");

        // THEN subscriber does receive messages from the new publisher
        let read = subscriber.read_next_messages(1).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].pressure, pressure_after_recreation);
    }

    #[tokio::test]
    async fn test_pub_sub_instant_read_after_publisher_drop() {
        let instance_name = "pubsub-instant-after-publisher-drop";
        // GIVEN Publisher and all the different subscription option
        let (sdv_comms, pressure_pub, _, _, _, instant_reader) =
            setup(instance_name, "topic-twelve").await;

        // WHEN publishing a message.
        let pressure = 43678;
        pressure_pub
            .publish(&TirePressure { pressure, ..Default::default() })
            .expect("pressure_pub publishing failed.");

        // THEN we can read last message.
        let instant_read = instant_reader.read_latest_message().await.unwrap();
        assert!(instant_read.is_some());
        assert_eq!(instant_read.unwrap().pressure, pressure);

        // WHEN the publisher is dropped
        drop(pressure_pub);

        // THEN instant_read reads Internal error.
        thread::sleep(HISTORY_PROCESSING_DURATION);
        let instant_read = instant_reader.read_latest_message().await;
        assert!(instant_read.is_err());
        assert_eq!(instant_read.unwrap_err().status_code(), SdvStatusCode::Internal);

        // THEN after all signal are propagated, instant_read reads Internal error.
        thread::sleep(PUBLISHER_DROP_SIGNAL_PROPAGATION);
        let instant_read = instant_reader.read_latest_message().await;
        assert!(instant_read.is_err());
        assert_eq!(instant_read.unwrap_err().status_code(), SdvStatusCode::Internal);

        // WHEN the publisher is re-created
        let publisher = clientlib::create_publisher(
            sdv_comms.as_ref(),
            publisher::Channel::<TirePressure>::TOPIC_TWELVE,
        )
        .await
        .expect("Create pressure_pub::TOPIC_TWELVE failed.");

        // WHEN publishing a new message.
        let pressure_after_recreation = 43679;
        publisher
            .publish(&TirePressure { pressure: pressure_after_recreation, ..Default::default() })
            .expect("pressure_pub publishing failed.");

        // THEN instant_read CAN read messages from the new publisher without being recreated.
        thread::sleep(PUBLISHER_DROP_SIGNAL_PROPAGATION);
        let instant_read = instant_reader.read_latest_message().await.unwrap();
        assert!(instant_read.is_some());
        assert_eq!(instant_read.unwrap().pressure, pressure_after_recreation);
    }

    #[tokio::test]
    async fn test_pub_sub_history_read_after_publisher_drop() {
        let instance_name = "pubsub-history-after-publisher-drop";
        // GIVEN Publisher and all the different subscription option
        let (sdv_comms, pressure_pub, _, _, history, _) =
            setup(instance_name, "topic-thirteen").await;

        // WHEN publishing a message.
        let pressure = 43678;
        pressure_pub
            .publish(&TirePressure { pressure, ..Default::default() })
            .expect("pressure_pub publishing failed.");

        // AND THEN we can read from history.
        // SLEEP to ensure the message is forwarded to the history buffer.
        thread::sleep(HISTORY_PROCESSING_DURATION);
        let read_history = history.read_from_history(1).unwrap();
        assert_eq!(read_history.len(), 1);
        assert_eq!(read_history[0].pressure, pressure);

        // WHEN the publisher is dropped
        drop(pressure_pub);

        // THEN history returns the last X published messages.
        thread::sleep(HISTORY_PROCESSING_DURATION);
        let read_history = history.read_from_history(2).unwrap();
        assert_eq!(read_history.len(), 1);
        assert_eq!(read_history[0].pressure, pressure);

        // THEN after all signal are propagated, history returns last X published messages.
        thread::sleep(PUBLISHER_DROP_SIGNAL_PROPAGATION);
        let read_history = history.read_from_history(2).unwrap();
        assert_eq!(read_history.len(), 1);
        assert_eq!(read_history[0].pressure, pressure);

        // WHEN the publisher is re-created
        let publisher = clientlib::create_publisher(
            sdv_comms.as_ref(),
            publisher::Channel::<TirePressure>::TOPIC_THIRTEEN,
        )
        .await
        .expect("Create pressure_pub::TOPIC_THIRTEEN failed.");

        // WHEN publishing a new message.
        let pressure_after_recreation = 43679;
        publisher
            .publish(&TirePressure { pressure: pressure_after_recreation, ..Default::default() })
            .expect("pressure_pub publishing failed.");

        // THEN history does not receive any message from this new publisher
        thread::sleep(PUBLISHER_DROP_SIGNAL_PROPAGATION);
        let read_history = history.read_from_history(2).unwrap();
        assert_eq!(read_history.len(), 2);
        assert_eq!(read_history[0].pressure, pressure);
        assert_eq!(read_history[1].pressure, pressure_after_recreation);
    }

    #[tokio::test]
    async fn test_pub_sub_publisher_double_creation() {
        let instance_name = "pubsub-double-creation";
        let sdv_comms = create_sdv_comms(instance_name);

        // GIVEN a publisher.
        let _publisher = clientlib::create_publisher(
            sdv_comms.as_ref(),
            publisher::Channel::<TirePressure>::TOPIC_FOURTEEN,
        )
        .await
        .expect("Create pressure_pub::TOPIC_FOURTEEN failed.");

        // WHEN creating another publisher with same sdv_comms object
        let publisher_2 = clientlib::create_publisher(
            sdv_comms.as_ref(),
            publisher::Channel::<TirePressure>::TOPIC_FOURTEEN,
        )
        .await;

        // THEN an error is returned
        assert!(publisher_2.is_err());
        let _ = publisher_2.inspect_err(|e| assert_eq!(e.status_code(), SdvStatusCode::Internal));
    }

    #[tokio::test]
    async fn test_pub_sub_observer_notification_after_message_published() {
        // Topics depends on a SD listener that needs a running binder thread.
        binder::ProcessState::start_thread_pool();

        let instance_name = "pubsub-observer-created-after-data";
        let sdv_comms = create_sdv_comms(instance_name);

        // GIVEN a publisher, publishing two messages
        let publisher = clientlib::create_publisher(
            sdv_comms.as_ref(),
            publisher::Channel::<TirePressure>::TOPIC_FIFTEEN,
        )
        .await
        .expect("Create publisher::TOPIC_FIFTEEN failed.");

        // AND GIVEN an observer
        let mut observer_from_beginning = clientlib::create_observer(
            sdv_comms.as_ref(),
            subscriber::Channel::<TirePressure>::TOPIC_FIFTEEN,
            &SubscribeOptions::builder()
                .instance_name(instance_name)
                .read_on_subscribe(true)
                .build()
                .unwrap(),
        )
        .await
        .expect("Create TirePressure observer failed.");

        // Make sure publisher is registered on the topic.
        tokio::time::sleep(TOPIC_PROCESSING_DURATION).await;

        // WHEN publishing two messages
        let pressure_1 = 43678;
        publisher
            .publish(&TirePressure { pressure: pressure_1, ..Default::default() })
            .expect("publisher publishing failed.");
        let pressure_2 = 43679;
        publisher
            .publish(&TirePressure { pressure: pressure_2, ..Default::default() })
            .expect("publisher publishing failed.");

        // observer wakes up and read two messages
        let received = observer_from_beginning.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 2);
        assert_eq!(received[0].pressure, pressure_1);
        assert_eq!(received[1].pressure, pressure_2);

        // no further notification is received
        let result =
            tokio::time::timeout(OBSERVER_TIMEOUT_DURATION, observer_from_beginning.next()).await;
        assert!(result.is_err()); // Timeout

        // WHEN creating an observer after messages have been published
        let mut observer_read_on_subscribe = clientlib::create_observer(
            sdv_comms.as_ref(),
            subscriber::Channel::<TirePressure>::TOPIC_FIFTEEN,
            &SubscribeOptions::builder()
                .instance_name(instance_name)
                .read_on_subscribe(true)
                .build()
                .unwrap(),
        )
        .await
        .expect("Create TirePressure observer failed.");

        // THEN the observer is able to read only the last messages
        let received = observer_read_on_subscribe.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].pressure, pressure_2);

        // no further notification is received
        let result =
            tokio::time::timeout(OBSERVER_TIMEOUT_DURATION, observer_read_on_subscribe.next())
                .await;
        assert!(result.is_err()); // Timeout

        // WHEN creating an observer without read on subscribe
        let mut observer_no_read_on_subscribe = clientlib::create_observer(
            sdv_comms.as_ref(),
            subscriber::Channel::<TirePressure>::TOPIC_FIFTEEN,
            &SubscribeOptions::builder()
                .instance_name(instance_name)
                .read_on_subscribe(false)
                .build()
                .unwrap(),
        )
        .await
        .expect("Create TirePressure observer failed.");

        // THEN the observer is able to read past messages
        let result =
            tokio::time::timeout(OBSERVER_TIMEOUT_DURATION, observer_no_read_on_subscribe.next())
                .await;
        assert!(result.is_err()); // Timeout
    }
}
