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
    use sdv::mw::{SdvComms, SubscribeOptions};
    use sdv::status::SdvStatusCode;
    // test catalog
    use sdv_mw_rs_sdv_vsidl_multipub_test_cabin_sensor::CabinSensor;
    use sdv_mw_rs_sdv_vsidl_multipub_test_footwell_sensor::FootwellSensor;
    use sdv_mw_rs_sdv_vsidl_multipub_test_hvac_control::subscriber as hvac_subscriber;
    use sdv_mw_rs_sdv_vsidl_multipub_test_passenger_sensor::PassengerSensor;
    use vsidl_multipub_test::cabin::CabinTemperature;

    #[tokio::test]
    async fn test_multipub() {
        // Multipub depends on a SD listener that needs a running binder thread.
        binder::ProcessState::start_thread_pool();
        const MULTIPUB_PROCESSING_DURATION: Duration = Duration::from_millis(200);
        const OBSERVER_TIMEOUT_DURATION: Duration = Duration::from_millis(500);

        let context = ContextRef::create(
            ServiceFqin::from_str("vm:sdv.vsidl.multipub.test.MultipubTest/sample").unwrap(),
        );
        let sdv_comms = Arc::new(SdvComms { context });

        // GIVEN a 'multipub' publisher (see VSIDL declaration).
        let mut cabin_sensor = CabinSensor::builder()
            .set_comms(sdv_comms.clone())
            .build()
            .await
            .expect("CabinSensor creation failed.");
        let cabin_pub = cabin_sensor
            .take_publisher(
                sdv_mw_rs_sdv_vsidl_multipub_test_cabin_sensor::publisher::Channel::<
                    CabinTemperature,
                >::MULTIPUB,
            )
            .expect("taking cabin pub failed.");
        // AND GIVEN a second 'mutlipub' publisher.
        let mut footwell_sensor = FootwellSensor::builder()
            .set_comms(sdv_comms.clone())
            .build()
            .await
            .expect("FootwellSensor creation failed.");
        let footwell_pub = footwell_sensor
            .take_publisher(
                sdv_mw_rs_sdv_vsidl_multipub_test_footwell_sensor::publisher::Channel::<
                    CabinTemperature,
                >::MULTIPUB,
            )
            .expect("taking footwell pub failed.");

        // AND GIVEN a 'multipub' subscriber and observer (see VSIDL declaration).
        let subscriber = sdv::mw::clientlib::create_subscriber(
            sdv_comms.as_ref(),
            hvac_subscriber::Channel::<CabinTemperature>::MULTIPUB,
            &SubscribeOptions::default(),
        )
        .await
        .expect("Create CabinTemperature subscriber failed.");
        let mut observer = sdv::mw::clientlib::create_observer(
            sdv_comms.as_ref(),
            hvac_subscriber::Channel::<CabinTemperature>::MULTIPUB,
            &SubscribeOptions::default(),
        )
        .await
        .expect("Create CabinTemperature observer failed.");

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

        // WHEN trying to get history on a multipub message.
        let result = sdv::mw::clientlib::create_history(
            sdv_comms.as_ref(),
            hvac_subscriber::Channel::<CabinTemperature>::MULTIPUB,
            &SubscribeOptions::default(),
        )
        .await;

        // THEN an error code is returned, suggesting this is not supported.
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status_code(), SdvStatusCode::Unimplemented);

        // WHEN trying to instant read a multipub message.
        let result = sdv::mw::clientlib::create_instant_reader(
            sdv_comms.as_ref(),
            hvac_subscriber::Channel::<CabinTemperature>::MULTIPUB,
            &SubscribeOptions::default(),
        )
        .await;

        // THEN an error code is returned, suggesting this is not supported.
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status_code(), SdvStatusCode::Unimplemented);

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
        let mut passenger_sensor = PassengerSensor::builder()
            .set_comms(sdv_comms.clone())
            .build()
            .await
            .expect("PassengerSensor creation failed.");
        let passenger_pub = passenger_sensor
            .take_publisher(
                sdv_mw_rs_sdv_vsidl_multipub_test_passenger_sensor::publisher::Channel::<
                    CabinTemperature,
                >::MULTIPUB,
            )
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
    }
}
