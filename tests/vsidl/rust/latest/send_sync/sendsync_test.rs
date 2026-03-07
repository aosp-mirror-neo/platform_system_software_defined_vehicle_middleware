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
    use tokio::time::{interval, Duration};
    use tokio_util::sync::CancellationToken;
    // sdv
    use sdv::comms::id::ServiceFqin;
    use sdv::comms::ContextRef;
    use sdv::mw::{clientlib, Availability, ClientOptions, SdvComms, SubscribeOptions};
    use sdv::status::SdvResult;
    // test catalog
    use sdv_mw_rs_sdv_vsidl_sendsync_test_bundle_one::{publisher as publisher_one, BundleOne};
    use sdv_mw_rs_sdv_vsidl_sendsync_test_bundle_two::{
        client, publisher as publisher_two, subscriber, BundleTwo,
    };
    use vsidl_sendsync_test_proto::tire::TirePressure;
    use vsidl_sendsync_test_proto::tire_rpc::GetTirePressureResponse;

    fn assert_send<T: Send>(_t: &T) {}

    struct TireStatusImpl {}
    #[async_trait::async_trait]
    impl sdv_vsidl_sendsync_test_tire_status_rpc::Interface for TireStatusImpl {
        async fn GetTirePressure(
            &self,
            _caller_id: sdv::comms::id::ServiceFqin,
            _request: &protobuf::well_known_types::empty::Empty,
        ) -> SdvResult<GetTirePressureResponse> {
            Ok(GetTirePressureResponse { pressure: 379, ..Default::default() })
        }
    }

    struct OtherTireStatusImpl {}
    #[async_trait::async_trait]
    impl sdv_vsidl_sendsync_test_other_tire_status_rpc::Interface for OtherTireStatusImpl {
        async fn GetTirePressure(
            &self,
            _caller_id: sdv::comms::id::ServiceFqin,
            _request: &protobuf::well_known_types::empty::Empty,
        ) -> SdvResult<GetTirePressureResponse> {
            Ok(GetTirePressureResponse { pressure: 379, ..Default::default() })
        }
    }

    /// This test ensures that the VSIDL-generated service-bundle is `Send`.
    ///
    /// Note: Any combination of methods / members could make the service bundle !Send.
    ///       Therefore, this test needs to be exhaustive:
    ///       - In order to ensure that the full generated code surface is tested, the service
    ///         bundle in this test has a publisher, subscriber, server, and client interface.
    ///       - In order to avoid false positive test results, all public interfaces are called so
    ///         that the compiler does not optimize them away (this was observed before when not
    ///         creating the RPC client in the test).
    ///
    /// Note: Given this is a UnitTest, we can only have one context object. I.e. both bundles share
    ///       the same FQIN.
    ///       This means that BundleOne cannot discover the publisher of BundleTwo (as long as MW is
    ///       based on discovery-by-name). Hence, we exhaustively test the interfaces of BundleTwo.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_service_bundle_is_send() {
        // We need to init Binder for the event registration stream
        // as Binder is used internally by SDV Service Discovery.
        // Events are not emitted unless `binder::ProcessState::start_thread_pool()`
        // is called beforehand.
        binder::ProcessState::start_thread_pool();

        let context = ContextRef::create(
            ServiceFqin::from_str("vm:sdv.vsidl.sendsync.test.BundleOne/sendsync").unwrap(),
        );
        let sdv_comms = Arc::new(SdvComms { context });
        let pressure = 2138;
        let token = CancellationToken::new();

        // GIVEN an instance of BundleOne.
        let bundle_one = BundleOne::builder()
            .set_comms(sdv_comms.clone())
            .set_server_sdv_vsidl_sendsync_test_other_tire_status(Arc::new(OtherTireStatusImpl {}))
            .build()
            .await
            .expect("BundleOne creation failed.");

        // THEN BundleOne is `Send`.
        assert_send(&bundle_one);
        let cloned_token = token.clone();
        let handle_one = tokio::spawn(async move {
            // Publish message repeatedly until cancelled.
            tokio::select! {
                _ = publish_repeat(bundle_one, pressure) => {
                }
                _ = cloned_token.cancelled() => {
                }
            }
        });

        // AND GIVEN instance of bundle Two.
        let bundle_two = BundleTwo::builder()
            .set_comms(sdv_comms.clone())
            .set_server_sdv_vsidl_sendsync_test_tire_status(Arc::new(TireStatusImpl {}))
            .build()
            .await
            .expect("BundleTwo creation failed.");

        // THEN BundleTwo is `Send`.
        assert_send(&bundle_two);
        let handle_two = tokio::spawn(async move {
            // Wait until message was observed.
            let received = init_and_observe_message(bundle_two, sdv_comms).await.unwrap().unwrap();
            assert_eq!(received.len(), 1);
            assert_eq!(received[0].pressure, pressure);
            // Cancel all tasks once the test succeeds.
            token.cancel();
        });

        // Wait for all futures to complete.
        let _ = tokio::join!(handle_one, handle_two);
    }

    /// Repeat-publish the pressure message with the given pressure.
    async fn publish_repeat(mut bundle: BundleOne, pressure: u32) {
        let publisher = bundle
            .take_publisher(publisher_one::Channel::<TirePressure>::FRONT_LEFT)
            .expect("taking publisher_one::FRONT_LEFT failed.");

        // Publish a pressure message every 200 ms.
        let mut interval = interval(Duration::from_millis(200));
        loop {
            interval.tick().await;
            publisher
                .publish(&TirePressure { pressure, ..Default::default() })
                .expect("publisher pulishing failed.");
        }
    }

    /// Fully initialize the service bundle (calling all public methods),
    /// then return the first observed message.
    async fn init_and_observe_message(
        mut bundle: BundleTwo,
        comms: Arc<SdvComms>,
    ) -> Option<SdvResult<Vec<TirePressure>>> {
        // Take and use publisher, to ensure this API is used.
        let publisher = bundle
            .take_publisher(publisher_two::Channel::<TirePressure>::FRONT_RIGHT)
            .expect("taking publisher_two::FRONT_RIGHT failed.");
        publisher
            .publish(&TirePressure { pressure: 0, ..Default::default() })
            .expect("publisher pulishing failed.");

        // Wait for client availability, to ensure create_client API is used.
        let mut stream = clientlib::create_server_registration_event_stream(
            comms.as_ref(),
            client::Descriptor::<sdv_vsidl_sendsync_test_other_tire_status_rpc::Client>::OTHER_TIRE_STATUS,
        )
        .await
        .expect("Server registration event stream creation failed");

        while let Some(event) = stream.next().await {
            if event == Availability::Available {
                break;
            }
        }

        clientlib::create_rpc_client(
            comms.as_ref(),
            client::Descriptor::<sdv_vsidl_sendsync_test_other_tire_status_rpc::Client>::OTHER_TIRE_STATUS,
            ClientOptions::default(),
        )
        .await
        .expect("create_rpc_client failed");

        // Check publication availability.
        let mut availability_stream = clientlib::create_registration_event_stream(
            comms.as_ref(),
            subscriber::Channel::<TirePressure>::FRONT_LEFT,
        )
        .await
        .expect("Availability stream creation failed");

        assert_eq!(
            tokio::time::timeout(Duration::from_millis(500), availability_stream.next()).await,
            Ok(Some(Availability::Available))
        );

        // Create all subscribe options to ensure APIs are called.
        let subscribe_result = clientlib::create_subscriber(
            comms.as_ref(),
            subscriber::Channel::<TirePressure>::FRONT_LEFT,
            &SubscribeOptions::default(),
        )
        .await;
        assert!(subscribe_result.is_ok());

        let history_result = clientlib::create_history(
            comms.as_ref(),
            subscriber::Channel::<TirePressure>::FRONT_LEFT,
            &SubscribeOptions::builder().message_count(6).build().unwrap(),
        )
        .await;
        assert!(history_result.is_ok());

        let instant_result = clientlib::create_instant_reader(
            comms.as_ref(),
            subscriber::Channel::<TirePressure>::FRONT_LEFT,
            &SubscribeOptions::default(),
        )
        .await;
        assert!(instant_result.is_ok());

        let mut observer = clientlib::create_observer(
            comms.as_ref(),
            subscriber::Channel::<TirePressure>::FRONT_LEFT,
            &SubscribeOptions::default(),
        )
        .await
        .expect("Create TirePressure observer failed.");

        // Return first observed message.
        observer.next().await
    }
}
