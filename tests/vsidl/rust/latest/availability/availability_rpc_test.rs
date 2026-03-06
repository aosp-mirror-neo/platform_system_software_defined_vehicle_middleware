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
    use std::time::Duration;
    // sdv
    use sdv::comms::id::ServiceFqin;
    use sdv::comms::ContextRef;
    use sdv::mw::clientlib;
    use sdv::mw::{Availability, ClientOptions, SdvComms};
    use sdv::status::{SdvResult, SdvStatus, SdvStatusCode};
    // test catalog
    use sdv_mw_rs_sdv_vsidl_rpc_test_tire_status_consumer::client;
    use sdv_mw_rs_sdv_vsidl_rpc_test_tire_status_producer::TireStatusProducer;
    use sdv_vsidl_rpc_test_tire_status_rpc::Interface;
    use vsidl_rpc_test_proto::tire_rpc::{
        GetTirePressureResponse, GetTireStatusRequest, TireLocation,
    };

    // Timeout to ensure propagation of Availability updates across discovery processes
    const EVENT_STREAM_TIMEOUT: Duration = Duration::from_millis(500);

    struct TireStatusImpl {}
    #[async_trait::async_trait]
    impl Interface for TireStatusImpl {
        async fn GetTirePressure(
            &self,
            caller_id: ServiceFqin,
            request: &GetTireStatusRequest,
        ) -> SdvResult<GetTirePressureResponse> {
            if request.error {
                return Err(SdvStatus::with_cause_and_message(
                    SdvStatusCode::PermissionDenied,
                    SdvStatus::new(SdvStatusCode::ResourceExhausted),
                    "Internal error occurred",
                ));
            }

            Ok(GetTirePressureResponse {
                pressure: 803,
                tire: request.tire,
                caller: caller_id.to_string(),
                ..Default::default()
            })
        }
    }

    #[tokio::test]
    async fn test_server_availability() {
        // We need to init Binder for the event registration stream
        // as Binder is used internally by SDV Service Discovery.
        // Events are not emitted unless `binder::ProcessState::start_thread_pool()`
        // is called beforehand.
        binder::ProcessState::start_thread_pool();

        let package_name = "sdv.vsidl.rpc.test";
        let server_bundle_name = "TireStatusProducer";
        let instance_name = "instance";
        let context = ContextRef::create(
            ServiceFqin::from_str(&format!(
                "vm:{package_name}.{server_bundle_name}/{instance_name}"
            ))
            .unwrap(),
        );
        let sdv_comms = Arc::new(SdvComms { context });

        // WHEN create registration event stream
        let mut registration_event_stream_before =
            clientlib::create_server_registration_event_stream(
                sdv_comms.as_ref(),
                client::Descriptor::<sdv_vsidl_rpc_test_tire_status_rpc::Client>::TIRE_STATUS,
            )
            .await
            .unwrap();

        // --------------------- Server Initially Available -----------------------

        // GIVEN we have a server implementing TireStatus
        let _tire_status_producer = TireStatusProducer::builder()
            .set_comms(sdv_comms.clone())
            .set_server_sdv_vsidl_rpc_test_tire_status(Arc::new(TireStatusImpl {}))
            .build()
            .await
            .unwrap();

        // WHEN create registration event stream after the sever was created
        let mut registration_event_stream_after =
            clientlib::create_server_registration_event_stream(
                sdv_comms.as_ref(),
                client::Descriptor::<sdv_vsidl_rpc_test_tire_status_rpc::Client>::TIRE_STATUS,
            )
            .await
            .unwrap();

        // THEN stream has an available item
        assert_eq!(
            tokio::time::timeout(EVENT_STREAM_TIMEOUT, registration_event_stream_before.next(),)
                .await,
            Ok(Some(Availability::Available))
        );
        assert_eq!(
            tokio::time::timeout(EVENT_STREAM_TIMEOUT, registration_event_stream_after.next(),)
                .await,
            Ok(Some(Availability::Available))
        );

        // AND THEN the client can make a successful request
        let client = clientlib::create_rpc_client(
            sdv_comms.as_ref(),
            client::Descriptor::<sdv_vsidl_rpc_test_tire_status_rpc::Client>::TIRE_STATUS,
            ClientOptions::default(),
        )
        .await
        .unwrap();

        let request =
            GetTireStatusRequest { tire: TireLocation::FRONT_RIGHT.into(), ..Default::default() };
        let response = client.GetTirePressure(&request).await.unwrap();
        assert_eq!(response.pressure, 803);

        // ----------------------------- Server Dropped  -----------------------------

        // WHEN the server is dropped.
        drop(_tire_status_producer);

        // THEN unavailable
        assert_eq!(
            tokio::time::timeout(EVENT_STREAM_TIMEOUT, registration_event_stream_before.next(),)
                .await,
            Ok(Some(Availability::Unavailable))
        );
        assert_eq!(
            tokio::time::timeout(EVENT_STREAM_TIMEOUT, registration_event_stream_after.next(),)
                .await,
            Ok(Some(Availability::Unavailable))
        );

        // --------------------- Server Available Again after Drop -----------------------

        // AND WHEN the publisher becomes available again
        let _tire_status_producer = TireStatusProducer::builder()
            .set_comms(sdv_comms.clone())
            .set_server_sdv_vsidl_rpc_test_tire_status(Arc::new(TireStatusImpl {}))
            .build()
            .await
            .unwrap();

        // THEN the availability steam yields 'Available'.
        assert_eq!(
            tokio::time::timeout(EVENT_STREAM_TIMEOUT, registration_event_stream_before.next(),)
                .await,
            Ok(Some(Availability::Available))
        );
    }
}
