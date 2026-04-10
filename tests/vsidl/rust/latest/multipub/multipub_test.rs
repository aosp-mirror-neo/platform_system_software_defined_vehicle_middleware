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
    use rand::Rng;
    use std::str::FromStr;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    // sdv
    use sdv::comms::id::ServiceFqin;
    use sdv::comms::ContextRef;
    use sdv::mw::{clientlib, SdvComms, SubscribeOptions};
    // test catalog
    use sdv_mw_rs_sdv_vsidl_multipub_test_hvac_control::SubscriberDescriptors as HvacSubscriberDescriptors;
    use vsidl_multipub_test::cabin::CabinTemperature;

    #[tokio::test]
    async fn test_multipub() {
        // Multipub depends on a SD listener that needs a running binder thread.
        binder::ProcessState::start_thread_pool();
        const MULTIPUB_PROCESSING_DURATION: Duration = Duration::from_millis(200);
        const OBSERVER_TIMEOUT_DURATION: Duration = Duration::from_millis(500);

        let mut rng = rand::thread_rng();
        let random_instance: String = (0..40).map(|_| rng.gen_range(b'a'..=b'z') as char).collect();
        let randomized_fqin = format!("vm:sdv.vsidl.multipub.test.MultipubTest/{random_instance}");
        let context = ContextRef::create(ServiceFqin::from_str(&randomized_fqin).unwrap());

        let sdv_comms = Arc::new(SdvComms { context });

        // GIVEN a 'multipub' publisher (see VSIDL declaration).
        let cabin_pub =
            clientlib::create_publisher(
                sdv_comms.as_ref(),
                sdv_mw_rs_sdv_vsidl_multipub_test_cabin_sensor::PublisherDescriptors::<
                    CabinTemperature,
                >::MULTIPUB,
            )
            .await
            .expect("taking cabin pub failed.");
        // AND GIVEN a second 'mutlipub' publisher.
        let footwell_pub = clientlib::create_publisher(
            sdv_comms.as_ref(),
            sdv_mw_rs_sdv_vsidl_multipub_test_footwell_sensor::PublisherDescriptors::<
                CabinTemperature,
            >::MULTIPUB,
        )
        .await
        .expect("taking footwell pub failed.");

        // AND GIVEN a 'multipub' subscriber, observer,
        // history and instant reader (see VSIDL declaration).
        let subscriber = sdv::mw::clientlib::create_subscriber(
            sdv_comms.as_ref(),
            HvacSubscriberDescriptors::<CabinTemperature>::MULTIPUB,
            &SubscribeOptions::default(),
        )
        .await
        .expect("Create CabinTemperature subscriber failed.");
        let mut observer = sdv::mw::clientlib::create_observer(
            sdv_comms.as_ref(),
            HvacSubscriberDescriptors::<CabinTemperature>::MULTIPUB,
            &SubscribeOptions::default(),
        )
        .await
        .expect("Create CabinTemperature observer failed.");
        let history_reader = sdv::mw::clientlib::create_history(
            sdv_comms.as_ref(),
            HvacSubscriberDescriptors::<CabinTemperature>::MULTIPUB,
            &SubscribeOptions::builder().message_count(10).build().unwrap(),
        )
        .await
        .expect("Create CabinTemperature history reader failed.");
        let instant_reader = sdv::mw::clientlib::create_instant_reader(
            sdv_comms.as_ref(),
            HvacSubscriberDescriptors::<CabinTemperature>::MULTIPUB,
            &SubscribeOptions::default(),
        )
        .await
        .expect("Create InstantRead should succeed with valid parameter.");

        // SLEEP to ensure the multiplexer is created on a separate process.
        thread::sleep(MULTIPUB_PROCESSING_DURATION);

        // WHEN publishing a message from multiple publishers
        let cabin_temp = 123;
        let footwell_temp = 321;
        cabin_pub
            .publish(&CabinTemperature { temperature: cabin_temp, ..Default::default() })
            .expect("cabin_pub pulishing failed.");
        footwell_pub
            .publish(&CabinTemperature { temperature: footwell_temp, ..Default::default() })
            .expect("footwell_pub pulishing failed.");

        // SLEEP to ensure ALL messages are propagated to the multiplexer queue.
        thread::sleep(MULTIPUB_PROCESSING_DURATION);

        // THEN observer receives two notifications.
        // With the first one both messages are read.
        let received: Vec<u32> = observer
            .next()
            .await
            .unwrap()
            .unwrap()
            .iter()
            .map(|message| message.temperature)
            .collect();
        assert_eq!(received.len(), 2);
        assert!(received.contains(&cabin_temp));
        assert!(received.contains(&footwell_temp));

        // AND THEN we can read the all messages.
        let read: Vec<u32> = subscriber
            .read_next_messages(5)
            .unwrap()
            .iter()
            .map(|message| message.temperature)
            .collect();
        assert_eq!(read.len(), 2);
        assert!(read.contains(&cabin_temp));
        assert!(read.contains(&footwell_temp));

        let history_buf: Vec<u32> = history_reader
            .read_from_history(5)
            .unwrap()
            .iter()
            .map(|message| message.temperature)
            .collect();
        assert_eq!(history_buf.len(), 2);
        assert!(history_buf.contains(&cabin_temp));
        assert!(history_buf.contains(&footwell_temp));

        // WHEN dropping both publishers
        drop(cabin_pub);
        drop(footwell_pub);

        // SLEEP to ensure ALL messages are propagated to the multiplexer queue.
        thread::sleep(MULTIPUB_PROCESSING_DURATION);

        // THEN observer does not receive any further events
        let result = tokio::time::timeout(OBSERVER_TIMEOUT_DURATION, observer.next()).await;
        assert!(result.is_err()); // Timeout

        // AND THEN subscriber reads no new messages.
        let read: Vec<u32> = subscriber
            .read_next_messages(5)
            .unwrap()
            .iter()
            .map(|message| message.temperature)
            .collect();
        assert_eq!(read.len(), 0);

        // GIVEN a NEW 'multipub' publisher (see VSIDL declaration).
        let passenger_pub = clientlib::create_publisher(
            sdv_comms.as_ref(),
            sdv_mw_rs_sdv_vsidl_multipub_test_passenger_sensor::PublisherDescriptors::<
                CabinTemperature,
            >::MULTIPUB,
        )
        .await
        .expect("taking passenger pub failed.");

        // SLEEP to ensure the multiplexer is created on a separate process.
        thread::sleep(MULTIPUB_PROCESSING_DURATION);

        // WHEN publishing a message from the new publisher
        let passenger_temp = 234;
        passenger_pub
            .publish(&CabinTemperature { temperature: passenger_temp, ..Default::default() })
            .expect("passenger_pub pulishing failed.");

        // SLEEP to ensure ALL messages are propagated to the multiplexer queue.
        thread::sleep(MULTIPUB_PROCESSING_DURATION);

        // THEN existing observer receives new notification.
        let received: Vec<u32> = observer
            .next()
            .await
            .unwrap()
            .unwrap()
            .iter()
            .map(|message| message.temperature)
            .collect();
        assert_eq!(received.len(), 1);
        assert!(received.contains(&passenger_temp));

        // AND THEN subscriber can reads new messages.
        let read: Vec<u32> = subscriber
            .read_next_messages(5)
            .unwrap()
            .iter()
            .map(|message| message.temperature)
            .collect();
        assert_eq!(read.len(), 1);
        assert!(read.contains(&passenger_temp));

        // AND THEN history reader can reads new and old messages.
        let history_buf: Vec<u32> = history_reader
            .read_from_history(5)
            .unwrap()
            .iter()
            .map(|message| message.temperature)
            .collect();
        assert_eq!(history_buf.len(), 3);
        assert!(history_buf.contains(&cabin_temp));
        assert!(history_buf.contains(&footwell_temp));
        assert!(history_buf.contains(&passenger_temp));

        let latest_message = instant_reader.read_latest_message().await.unwrap().unwrap();
        assert_eq!(latest_message.temperature, passenger_temp);
    }
}
