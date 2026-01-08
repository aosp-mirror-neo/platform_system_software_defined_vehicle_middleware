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
    use sdv_mw_rs_sdv_vsidl_test_stable_multipub_cabin_sensor::{
        publisher as cabin_publisher, CabinSensor,
    };
    use sdv_mw_rs_sdv_vsidl_test_stable_multipub_hvac_control::{
        subscriber as hvac_subscriber, HvacControl,
    };
    use sdv_mw_rs_sdv_vsidl_test_stable_pubsub_publishing_bundle::{publisher, PublishingBundle};
    use sdv_mw_rs_sdv_vsidl_test_stable_pubsub_subscribing_bundle::{
        subscriber, SubscribingBundle,
    };
    use vsidl_test_stable_multipub_protos::cabin::CabinTemperature;
    use vsidl_test_stable_pubsub_protos::tire::TirePressure;

    // Timeout to ensure propagation of Availability updates across discovery processes.
    const EVENT_STREAM_TIMEOUT: Duration = Duration::from_millis(500);

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
            ServiceFqin::from_str("vm:sdv.vsidl.test.stable.pubsub.PublishingBundle/availability")
                .unwrap(),
        );
        let sdv_comms = Arc::new(SdvComms { context });

        // GIVEN a subscribing bundle.
        let mut subscribing_bundle = SubscribingBundle::new(sdv_comms.clone())
            .await
            .expect("SubscribingBundle creation failed.");

        // AND GIVEN an availability stream.
        let mut availability_stream = subscribing_bundle
            .create_registration_event_stream(
                subscriber::Variant::<TirePressure>::FRONT_RIGHT,
                &SubscribeOptions::default(),
            )
            .await
            .expect("Availability stream creation failed");

        // WHEN trying to create an observer.
        let result = subscribing_bundle
            .create_observer(
                subscriber::Variant::<TirePressure>::FRONT_RIGHT,
                &SubscribeOptions::default(),
            )
            .await;

        // THEN observer creation failed.
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status_code(), SdvStatusCode::Unavailable);

        // --------------------- Publisher Initially Available -----------------------

        // WHEN creating the publishing service bundle.
        let mut publishing_bundle = PublishingBundle::new(sdv_comms.clone())
            .await
            .expect("PublishingBundle creation failed.");

        // THEN the availability steam yields 'Available'.
        assert_eq!(
            tokio::time::timeout(EVENT_STREAM_TIMEOUT, availability_stream.next()).await,
            Ok(Some(Availability::Available))
        );

        // AND THEN we can create an observer and receive messages
        let pressure = 12823;

        let mut observer = subscribing_bundle
            .create_observer(
                subscriber::Variant::<TirePressure>::FRONT_RIGHT,
                &SubscribeOptions::default(),
            )
            .await
            .expect("observer creation for TirePressure::FRONT_RIGHT failed.");

        let pressure_pub = publishing_bundle
            .take_publisher(publisher::Variant::<TirePressure>::FRONT_RIGHT)
            .expect("taking publisher Variant::FRONT_RIGHT failed.");
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

        // THEN observer stream becomes invalid and always should return None
        assert!(observer.next().await.is_none());

        // --------------------- Publisher Available Again after Drop -----------------------

        // AND WHEN the publisher becomes available again
        drop(publishing_bundle);
        let mut publishing_bundle = PublishingBundle::new(sdv_comms.clone())
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
            .take_publisher(publisher::Variant::<TirePressure>::FRONT_RIGHT)
            .expect("taking publisher Variant::FRONT_RIGHT failed.");
        pressure_pub
            .publish(&TirePressure { pressure: pressure_two, ..Default::default() })
            .expect("pressure_pub pulishing failed.");

        // THEN the observer's stream must still return None
        assert!(tokio::time::timeout(EVENT_STREAM_TIMEOUT, observer.next())
            .await
            .expect("We must get None from the stream instead of time out.")
            .is_none());
    }

    #[tokio::test]
    async fn test_multipub_availability() {
        const MULTIPUB_PROCESSING_DURATION: Duration = Duration::from_millis(200);

        // We need to init Binder for the event registration stream
        // as Binder is used internally by SDV Service Discovery.
        // Events are not emitted unless `binder::ProcessState::start_thread_pool()`
        // is called beforehand.
        binder::ProcessState::start_thread_pool();

        let context = ContextRef::create(
            ServiceFqin::from_str("vm:sdv.vsidl.test.stable.multipub.MultipubTest/availability")
                .unwrap(),
        );
        let sdv_comms = Arc::new(SdvComms { context });

        // GIVEN a subscribing bundle.
        let mut hvac_control =
            HvacControl::new(sdv_comms.clone()).await.expect("HvacControl creation failed.");

        // AND GIVEN an availability stream.
        let mut availability_stream = hvac_control
            .create_registration_event_stream(
                hvac_subscriber::Variant::<CabinTemperature>::UNIQUE,
                &SubscribeOptions::default(),
            )
            .await
            .expect("Availability stream creation failed");

        // WHEN trying to create an observer.
        let result = hvac_control
            .create_observer(
                hvac_subscriber::Variant::<CabinTemperature>::UNIQUE,
                &SubscribeOptions::default(),
            )
            .await;

        // THEN observer creation failed.
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().status_code(), SdvStatusCode::NotFound);

        // WHEN creating the publishing service bundle.
        let mut cabin_sensor =
            CabinSensor::new(sdv_comms.clone()).await.expect("CabinSensor creation failed.");
        // THEN the availability steam yields 'Available'.
        assert_eq!(
            tokio::time::timeout(EVENT_STREAM_TIMEOUT, availability_stream.next()).await,
            Ok(Some(Availability::Available))
        );

        // AND THEN we can create an observer and receive messages
        let cabin_temp = 8562;

        let mut observer = hvac_control
            .create_observer(
                hvac_subscriber::Variant::<CabinTemperature>::UNIQUE,
                &SubscribeOptions::default(),
            )
            .await
            .expect("Create CabinTemperature observer failed.");

        // SLEEP to ensure the multiplexer is created on a separate process.
        thread::sleep(MULTIPUB_PROCESSING_DURATION);

        let cabin_pub = cabin_sensor
            .take_publisher(cabin_publisher::Variant::<CabinTemperature>::UNIQUE)
            .expect("taking cabin pub failed.");
        cabin_pub
            .publish(&CabinTemperature { temperature: cabin_temp, ..Default::default() })
            .expect("cabin_pub pulishing failed.");

        let received = observer.next().await.unwrap().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].temperature, cabin_temp);
    }
}
