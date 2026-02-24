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
    use sdv::mw::{Availability, SdvComms, SubscribeOptions};
    use sdv::status::SdvStatusCode;
    // test catalog
    use sdv_mw_rs_sdv_vsidl_multipub_test_cabin_sensor::{
        publisher as cabin_publisher, CabinSensor,
    };
    use sdv_mw_rs_sdv_vsidl_multipub_test_hvac_control::subscriber as hvac_subscriber;
    use sdv_mw_rs_sdv_vsidl_pubsub_test_publishing_bundle::{publisher, PublishingBundle};
    use sdv_mw_rs_sdv_vsidl_pubsub_test_subscribing_bundle::subscriber;
    use vsidl_multipub_test::cabin::CabinTemperature;
    use vsidl_pubsub_test_proto::tire::TirePressure;

    // Timeout to ensure propagation of Availability updates across discovery processes.
    const EVENT_STREAM_TIMEOUT: Duration = Duration::from_millis(500);
    // Time required by Topic multiplexer to learn about publishers
    const TOPIC_PROCESSING_DURATION: Duration = Duration::from_millis(200);

    #[tokio::test]
    async fn test_publication_availability() {
        // We need to init Binder for the event registration stream
        // as Binder is used internally by SDV Service Discovery.
        // Events are not emitted unless `binder::ProcessState::start_thread_pool()`
        // is called beforehand.
        binder::ProcessState::start_thread_pool();

        // Timeout to ensure propagation of Availability updates across discovery processes.
        const EVENT_STREAM_TIMEOUT: Duration = Duration::from_millis(500);

        let context = ContextRef::create(
            ServiceFqin::from_str("vm:sdv.vsidl.pubsub.test.PublishingBundle/availability")
                .unwrap(),
        );
        let sdv_comms = Arc::new(SdvComms { context });

        // AND GIVEN an availability stream.
        let mut availability_stream = sdv::mw::clientlib::create_registration_event_stream(
            sdv_comms.as_ref(),
            subscriber::Channel::<TirePressure>::CHANNEL_ZERO,
        )
        .await
        .expect("Availability stream creation failed");

        // WHEN trying to create an observer.
        let result = sdv::mw::clientlib::create_observer(
            sdv_comms.as_ref(),
            subscriber::Channel::<TirePressure>::CHANNEL_ZERO,
            &SubscribeOptions::default(),
        )
        .await;

        // THEN observer creation failed.
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status_code(), SdvStatusCode::NotFound);

        // --------------------- Publisher Initially Available -----------------------

        // WHEN creating the publishing service bundle.
        let mut publishing_bundle = PublishingBundle::builder()
            .set_comms(sdv_comms.clone())
            .build()
            .await
            .expect("PublishingBundle creation failed.");

        // THEN the availability steam yields 'Available'.
        assert_eq!(
            tokio::time::timeout(EVENT_STREAM_TIMEOUT, availability_stream.next()).await,
            Ok(Some(Availability::Available))
        );

        // AND THEN we can create an observer and receive messages
        let pressure = 12823;

        let mut observer = sdv::mw::clientlib::create_observer(
            sdv_comms.as_ref(),
            subscriber::Channel::<TirePressure>::CHANNEL_ZERO,
            &SubscribeOptions::default(),
        )
        .await
        .expect("observer creation for TirePressure::CHANNEL_ZERO failed.");

        let pressure_pub = publishing_bundle
            .take_publisher(publisher::Channel::<TirePressure>::CHANNEL_ZERO)
            .expect("taking publisher Channel::CHANNEL_ZERO failed.");
        // SLEEP to ensure the topic multiplexer is created on a separate process.
        thread::sleep(TOPIC_PROCESSING_DURATION);

        pressure_pub
            .publish(&TirePressure { pressure, ..Default::default() })
            .expect("pressure_pub pulishing failed.");

        let received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].pressure, pressure);

        // ----------------------------- Publisher Dropped  -----------------------------

        // AND WHEN the publisher is dropped.
        drop(pressure_pub);

        // THEN the availability steam yields 'Unavailable'.
        assert_eq!(
            tokio::time::timeout(EVENT_STREAM_TIMEOUT, availability_stream.next()).await,
            Ok(Some(Availability::Unavailable))
        );

        // SLEEP to ensure the topic multiplexer is created on a separate process.
        thread::sleep(TOPIC_PROCESSING_DURATION);

        // --------------------- Publisher Available Again after Drop -----------------------

        // AND WHEN the publisher becomes available again
        drop(publishing_bundle);
        let mut publishing_bundle = PublishingBundle::builder()
            .set_comms(sdv_comms.clone())
            .build()
            .await
            .expect("PublishingBundle creation failed.");

        // THEN the availability steam yields 'Available'.
        assert_eq!(
            tokio::time::timeout(EVENT_STREAM_TIMEOUT, availability_stream.next()).await,
            Ok(Some(Availability::Available))
        );

        // AND WHEN publishing a message:
        let pressure_two = 84723;

        let pressure_pub = publishing_bundle
            .take_publisher(publisher::Channel::<TirePressure>::CHANNEL_ZERO)
            .expect("taking publisher Channel::CHANNEL_ZERO failed.");

        // SLEEP to ensure the topic multiplexer is created on a separate process.
        thread::sleep(TOPIC_PROCESSING_DURATION);

        pressure_pub
            .publish(&TirePressure { pressure: pressure_two, ..Default::default() })
            .expect("pressure_pub publishing failed.");

        let received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].pressure, pressure_two);
    }

    #[tokio::test]
    async fn test_multipub_availability() {
        // We need to init Binder for the event registration stream
        // as Binder is used internally by SDV Service Discovery.
        // Events are not emitted unless `binder::ProcessState::start_thread_pool()`
        // is called beforehand.
        binder::ProcessState::start_thread_pool();

        let context = ContextRef::create(
            ServiceFqin::from_str("vm:sdv.vsidl.multipub.test.MultipubTest/availability").unwrap(),
        );
        let sdv_comms = Arc::new(SdvComms { context });

        // AND GIVEN an availability stream.
        let mut availability_stream = sdv::mw::clientlib::create_registration_event_stream(
            sdv_comms.as_ref(),
            hvac_subscriber::Channel::<CabinTemperature>::MULTIPUB,
        )
        .await
        .expect("Availability stream creation failed");

        // WHEN trying to create an observer.
        let result = sdv::mw::clientlib::create_observer(
            sdv_comms.as_ref(),
            hvac_subscriber::Channel::<CabinTemperature>::MULTIPUB,
            &SubscribeOptions::default(),
        )
        .await;

        // THEN observer creation failed.
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status_code(), SdvStatusCode::NotFound);

        // WHEN creating the publishing service bundle.
        let mut cabin_sensor = CabinSensor::builder()
            .set_comms(sdv_comms.clone())
            .build()
            .await
            .expect("CabinSensor creation failed.");
        // THEN the availability steam yields 'Available'.
        assert_eq!(
            tokio::time::timeout(EVENT_STREAM_TIMEOUT, availability_stream.next()).await,
            Ok(Some(Availability::Available))
        );

        // AND THEN we can create an observer and receive messages
        let cabin_temp = 8562;

        let mut observer = sdv::mw::clientlib::create_observer(
            sdv_comms.as_ref(),
            hvac_subscriber::Channel::<CabinTemperature>::MULTIPUB,
            &SubscribeOptions::default(),
        )
        .await
        .expect("Create CabinTemperature observer failed.");

        // SLEEP to ensure the topic multiplexer is created on a separate process.
        thread::sleep(TOPIC_PROCESSING_DURATION);

        let cabin_pub = cabin_sensor
            .take_publisher(cabin_publisher::Channel::<CabinTemperature>::MULTIPUB)
            .expect("taking cabin pub failed.");
        cabin_pub
            .publish(&CabinTemperature { temperature: cabin_temp, ..Default::default() })
            .expect("cabin_pub pulishing failed.");

        let received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].temperature, cabin_temp);
    }
}
