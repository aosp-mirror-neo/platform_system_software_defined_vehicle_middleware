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
    use std::time::Duration;

    // SDV comms
    use sdv::comms::{id::ServiceFqin, ContextRef};
    use sdv::mw::{Availability, SdvComms, SubscribeOptions};
    const SUBSCRIBER_SETUP_DURATION: Duration = Duration::from_millis(200);
    const EVENT_STREAM_TIMEOUT: Duration = Duration::from_millis(500);

    // SDV clientlib / MW SDK
    use sdv::mw::clientlib::{
        self, PublisherDescriptorFull as PubDesc, SubscriberDescriptorFull as SubDesc,
    };

    // test catalog
    use vsidl_hello_world_test_proto::hello::{hello::Inner, Hello};

    fn setup_identity(instance_name: &str) -> ContextRef {
        binder::ProcessState::start_thread_pool();

        ContextRef::create(
            ServiceFqin::from_str(format!("vm:sdv.mw.hello.test.Bundle/{instance_name}").as_ref())
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn test_hello_world() {
        let topic = "hello-world";

        // setup SDV Comms
        let context = setup_identity(topic);
        let comms = SdvComms { context };

        // GIVEN a publisher.
        let pub_decr = PubDesc::<Hello>::new(topic);
        let publisher = clientlib::create_publisher(&comms, &pub_decr).await.unwrap();

        // AND GIVEN a subscriber.
        let sub_decr = SubDesc::<Hello>::new(topic);
        let sub_opt = SubscribeOptions::default();
        let subscriber = clientlib::create_subscriber(&comms, &sub_decr, &sub_opt).await.unwrap();
        tokio::time::sleep(SUBSCRIBER_SETUP_DURATION).await;

        // WHEN publishing a message.
        let message = &Hello {
            inner: vec![Inner { some_text: "Hello, world!".to_string(), ..Default::default() }],
            ..Default::default()
        };
        publisher.publish(message).unwrap();

        // THEN we can read the message.
        let read = subscriber.read_next_messages(1).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].inner.len(), 1);
        assert_eq!(read[0].inner[0].some_text, "Hello, world!");
    }

    #[tokio::test]
    async fn test_publisher_availability() {
        let topic = "publisher-availability";

        // setup SDV Comms
        let context = setup_identity(topic);
        let comms = SdvComms { context };

        // GIVEN a subscriber descriptor.
        let sub_decr = SubDesc::<Hello>::new(topic);

        // AND GIVEN an availability stream.
        let mut availability_stream =
            clientlib::create_registration_event_stream(&comms, &sub_decr)
                .await
                .expect("Availability stream creation failed");

        // WHEN creating the publisher.
        let pub_decr = PubDesc::<Hello>::new(topic);
        let publisher = clientlib::create_publisher(&comms, &pub_decr).await.unwrap();

        // THEN the availability steam yields 'Available'.
        assert_eq!(
            tokio::time::timeout(EVENT_STREAM_TIMEOUT, availability_stream.next()).await,
            Ok(Some(Availability::Available))
        );

        // WHEN dropping the publisher.
        drop(publisher);

        // THEN the availability steam yields 'Unavailable'.
        assert_eq!(
            tokio::time::timeout(EVENT_STREAM_TIMEOUT, availability_stream.next()).await,
            Ok(Some(Availability::Unavailable))
        );
    }
}
