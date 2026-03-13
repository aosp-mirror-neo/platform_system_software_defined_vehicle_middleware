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
    use futures::stream::{self, Stream, StreamExt};
    use std::pin::Pin;
    use std::str::FromStr;
    use std::sync::Arc;
    // sdv
    use sdv::comms::id::ServiceFqin;
    use sdv::comms::ContextRef;
    use sdv::mw::{clientlib, Availability, ClientOptions, SdvComms};
    use sdv::status::{SdvResult, SdvStatus, SdvStatusCode};
    // test catalog
    use sdv_mw_rs_sdv_vsidl_rpc_test_tire_status_consumer::client;
    use sdv_mw_rs_sdv_vsidl_rpc_test_tire_status_producer::TireStatusProducer;
    use vsidl_rpc_test_proto::tire_rpc::{
        GetTirePressureResponse, GetTireStatusRequest, TireLocation,
    };

    struct TireStatusImpl {}
    #[async_trait::async_trait]
    impl sdv_vsidl_rpc_test_tire_status_rpc::Interface for TireStatusImpl {
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
                pressure: 992,
                tire: request.tire,
                caller: caller_id.to_string(),
                ..Default::default()
            })
        }

        async fn GetTirePressureServerStream(
            &self,
            caller_id: ServiceFqin,
            request: &GetTireStatusRequest,
        ) -> SdvResult<Pin<Box<dyn Stream<Item = SdvResult<GetTirePressureResponse>> + Send>>>
        {
            if request.error {
                return Err(SdvStatus::with_message(
                    SdvStatusCode::InvalidArgument,
                    "Invalid argument in server stream",
                ));
            }

            let caller_id_str = caller_id.to_string();
            let tire = request.tire;
            let stream = stream::iter(0..3).map(move |i| {
                Ok(GetTirePressureResponse {
                    pressure: 992 + i,
                    tire,
                    caller: caller_id_str.clone(),
                    ..Default::default()
                })
            });

            Ok(Box::pin(stream))
        }

        async fn GetTirePressureClientStream(
            &self,
            caller_id: ServiceFqin,
            request: Pin<Box<dyn Stream<Item = GetTireStatusRequest> + Send>>,
        ) -> SdvResult<GetTirePressureResponse> {
            let count = request.count().await;

            Ok(GetTirePressureResponse {
                pressure: count as u32,
                caller: caller_id.to_string(),
                ..Default::default()
            })
        }

        fn init_server_options(&self) -> sdv::mw::ServerOptions {
            sdv::mw::ServerOptions::builder().max_request_size(Some(1024)).build().unwrap()
        }
    }

    async fn setup_rpc_test(instance_name: &str) -> (ServiceFqin, Arc<SdvComms>) {
        let package_name = "sdv.vsidl.rpc.test";
        let server_bundle_name = "TireStatusProducer";
        let context = ContextRef::create(
            ServiceFqin::from_str(&format!(
                "vm:{package_name}.{server_bundle_name}/{instance_name}"
            ))
            .unwrap(),
        );
        // fqin is shared by client and server in this sample, due to this being a unit test.
        let fqin = context.get_self_fqin();
        let sdv_comms = Arc::new(SdvComms { context });

        (fqin, sdv_comms)
    }

    #[tokio::test]
    async fn test_rpc_unary() {
        let (fqin, sdv_comms) = setup_rpc_test("unary-test-instance").await;

        // GIVEN a server
        let tire_status_producer = TireStatusProducer::builder()
            .set_comms(sdv_comms.clone())
            .set_server_tire_status_unary(Arc::new(TireStatusImpl {}))
            .build()
            .await
            .unwrap();

        // AND GIVEN we have a client for TireStatus.
        let mut stream = clientlib::create_server_registration_event_stream(
            sdv_comms.as_ref(),
            client::Descriptor::<sdv_vsidl_rpc_test_tire_status_rpc::Client>::TIRE_STATUS_UNARY,
        )
        .await
        .unwrap();

        while let Some(event) = stream.next().await {
            if event == Availability::Available {
                break;
            }
        }
        let client = clientlib::create_rpc_client(
            sdv_comms.as_ref(),
            client::Descriptor::<sdv_vsidl_rpc_test_tire_status_rpc::Client>::TIRE_STATUS_UNARY,
            ClientOptions::default(),
        )
        .await
        .unwrap();

        // WHEN calling the remote method
        let location = TireLocation::FRONT_RIGHT;
        let request = GetTireStatusRequest { tire: location.into(), ..Default::default() };
        let response = client.GetTirePressure(&request).await.unwrap();

        // THEN the expected value is returned.
        assert_eq!(response.pressure, 992);
        assert_eq!(response.tire, location.into());
        assert_eq!(response.caller, fqin.to_string());

        // WHEN calling the remote method with error
        let location = TireLocation::FRONT_RIGHT;
        let request =
            GetTireStatusRequest { tire: location.into(), error: true, ..Default::default() };
        let response = client.GetTirePressure(&request).await;

        // THEN the expected value is returned.
        assert!(response.is_err());
        let response_err = response.unwrap_err();
        assert_eq!(response_err.status_code(), sdv::status::SdvStatusCode::PermissionDenied);
        assert!(response_err.to_string().contains("PermissionDenied"));
        assert!(response_err.to_string().contains("Internal error occurred"));
        assert!(response_err.to_string().contains("ResourceExhausted"));

        // dropping the server
        drop(tire_status_producer);

        // Unfortunately drop() just initiates the destruction of server. There are
        // no guarantees that server stops accepting new connections on drop() return.
        // So we have to wait for a while.
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // WHEN calling the remote method of a dropped server.
        let location = TireLocation::FRONT_RIGHT;
        let request = GetTireStatusRequest { tire: location.into(), ..Default::default() };
        let response = client.GetTirePressure(&request).await;

        // THEN an error for Unavailable service is returned.
        assert!(response.is_err());
        let response_err = response.unwrap_err();
        assert!(response_err.to_string().contains("Unavailable"));
    }

    #[tokio::test]
    async fn test_rpc_server_streaming() {
        let (fqin, sdv_comms) = setup_rpc_test("server-streaming-instance").await;

        // GIVEN a server
        let _tire_status_producer = TireStatusProducer::builder()
            .set_comms(sdv_comms.clone())
            .set_server_tire_status_server_stream(Arc::new(TireStatusImpl {}))
            .build()
            .await
            .unwrap();

        // AND GIVEN we have a client for TireStatus.
        let mut stream = clientlib::create_server_registration_event_stream(
            sdv_comms.as_ref(),
            client::Descriptor::<sdv_vsidl_rpc_test_tire_status_rpc::Client>::TIRE_STATUS_SERVER_STREAM,
        )
        .await
        .unwrap();

        while let Some(event) = stream.next().await {
            if event == Availability::Available {
                break;
            }
        }
        let client = clientlib::create_rpc_client(
            sdv_comms.as_ref(),
            client::Descriptor::<sdv_vsidl_rpc_test_tire_status_rpc::Client>::TIRE_STATUS_SERVER_STREAM,
            ClientOptions::default(),
        )
        .await
        .unwrap();

        // WHEN calling the server streaming method
        let location = TireLocation::REAR_LEFT;
        let request = GetTireStatusRequest { tire: location.into(), ..Default::default() };
        let mut response_stream = client.GetTirePressureServerStream(&request).await.unwrap();

        // THEN the expected stream is returned.
        for i in 0..3 {
            let response = response_stream.next().await.unwrap().unwrap();
            assert_eq!(response.pressure, 992 + i);
            assert_eq!(response.tire, location.into());
            assert_eq!(response.caller, fqin.to_string());
        }
        assert!(response_stream.next().await.is_none());

        // WHEN calling the server streaming method with an error
        let request =
            GetTireStatusRequest { tire: location.into(), error: true, ..Default::default() };
        let response = client.GetTirePressureServerStream(&request).await;

        // THEN result is err
        assert!(response.is_err());
        let err = response.err().unwrap();
        assert_eq!(err.status_code(), SdvStatusCode::InvalidArgument);
        assert!(err.to_string().contains("Invalid argument in server stream"));
    }

    #[tokio::test]
    async fn test_rpc_client_streaming() {
        let (fqin, sdv_comms) = setup_rpc_test("client-streaming-instance").await;

        // GIVEN a server
        let _tire_status_producer = TireStatusProducer::builder()
            .set_comms(sdv_comms.clone())
            .set_server_tire_status_client_stream(Arc::new(TireStatusImpl {}))
            .build()
            .await
            .unwrap();

        // AND GIVEN we have a client for TireStatus.
        let mut stream = clientlib::create_server_registration_event_stream(
            sdv_comms.as_ref(),
            client::Descriptor::<sdv_vsidl_rpc_test_tire_status_rpc::Client>::TIRE_STATUS_CLIENT_STREAM,
        )
        .await
        .unwrap();

        while let Some(event) = stream.next().await {
            if event == Availability::Available {
                break;
            }
        }
        let client = clientlib::create_rpc_client(
            sdv_comms.as_ref(),
            client::Descriptor::<sdv_vsidl_rpc_test_tire_status_rpc::Client>::TIRE_STATUS_CLIENT_STREAM,
            ClientOptions::default(),
        )
        .await
        .unwrap();

        // WHEN calling the client streaming method
        let requests = vec![
            GetTireStatusRequest { ..Default::default() },
            GetTireStatusRequest { ..Default::default() },
            GetTireStatusRequest { ..Default::default() },
            GetTireStatusRequest { ..Default::default() },
        ];
        let request_stream = stream::iter(requests);
        let response = client.GetTirePressureClientStream(Box::pin(request_stream)).await.unwrap();

        // THEN the response contains the count of requests.
        assert_eq!(response.pressure, 4);
        assert_eq!(response.caller, fqin.to_string());
    }

    #[tokio::test]
    #[should_panic]
    async fn test_rpc_server_defined_twice() {
        let package_name = "sdv.vsidl.rpc.test";
        let server_bundle_name = "TireStatusProducer";
        let instance_name = "instance-defined-twice";
        let context = ContextRef::create(
            ServiceFqin::from_str(&format!(
                "vm:{package_name}.{server_bundle_name}/{instance_name}"
            ))
            .unwrap(),
        );
        let sdv_comms = Arc::new(SdvComms { context });

        // GIVEN we have a server implementing TireStatus.
        let _tire_status_producer = TireStatusProducer::builder()
            .set_comms(sdv_comms.clone())
            .set_server_tire_status_twice(Arc::new(TireStatusImpl {}))
            .build()
            .await
            .unwrap();

        // WHEN define another producer it's success
        let _tire_status_producer_2 = TireStatusProducer::builder()
            .set_comms(sdv_comms.clone())
            .set_server_tire_status_twice(Arc::new(TireStatusImpl {}))
            .build()
            .await
            .unwrap();

        // THEN expect panic when the object are destroyed
    }

    #[tokio::test]
    async fn test_rpc_producer_with_no_server_setting() {
        let package_name = "sdv.vsidl.rpc.test";
        let server_bundle_name = "TireStatusProducer";
        let instance_name = "no-server-instance";
        let context = ContextRef::create(
            ServiceFqin::from_str(&format!(
                "vm:{package_name}.{server_bundle_name}/{instance_name}"
            ))
            .unwrap(),
        );
        let sdv_comms = Arc::new(SdvComms { context });

        // GIVEN a service bundle builder
        // WHEN building the producer without setting a server implementation
        let tire_status_producer_result = TireStatusProducer::builder()
            .set_comms(sdv_comms.clone())
            // .set_server_sdv_vsidl_rpc_test_tire_status(...) is intentionally not called
            .build()
            .await;

        // THEN the producer is created successfully without error.
        assert!(tire_status_producer_result.is_ok());
    }
}
