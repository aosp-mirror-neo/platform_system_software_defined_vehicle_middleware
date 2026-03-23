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

//! Contains descriptors related to RPC client channels.

use crate::rpc_client::IntoRpcClientAttributes;
use sdv_mw_rs_comms::{RpcClient, TypeName};
use sdv_status_rs::SdvResult;
use std::marker::PhantomData;

/// A descriptor that binds a specific RPC client type to a communication channel
/// (Code-gen use case)
///
/// This struct carries the identity and configuration required to create an RPC client.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ClientDescriptor<T> {
    /// The name of the channel.
    pub channel: &'static str,
    /// The fully qualified service name (e.g., "sdv.sample.TireService").
    pub service: &'static str,
    /// Marker for the client stub type.
    pub(crate) _marker: PhantomData<T>,
}

impl<T> ClientDescriptor<T> {
    /// Creates a new RPC descriptor with mandatory fields.
    pub const fn new(channel: &'static str) -> Self {
        Self { channel, service: "", _marker: PhantomData }
    }

    /// Sets the fully qualified service name.
    pub const fn with_service(mut self, name: &'static str) -> Self {
        self.service = name;
        self
    }
}

impl<T: RpcClient> IntoRpcClientAttributes for ClientDescriptor<T> {
    type Client = T;

    fn to_attributes(&self) -> SdvResult<(TypeName, String)> {
        let type_name_obj = TypeName::from_fqn(self.service)?;
        Ok((type_name_obj, self.channel.to_string()))
    }
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::Stream;
    use sdv_mw_rs_comms::ClientTransport;
    use sdv_status_rs::SdvResult;
    use std::pin::Pin;

    struct ClientTransportStub;

    #[async_trait]
    impl ClientTransport for ClientTransportStub {
        async fn call_method(&self, _method: &str, _request: &[u8]) -> SdvResult<Vec<u8>> {
            Ok(vec![])
        }
        async fn call_client_streaming_method(
            &self,
            _method_name: &str,
            _request_stream: Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>,
        ) -> SdvResult<Vec<u8>> {
            Ok(vec![])
        }
        async fn call_server_streaming_method(
            &self,
            _method_name: &str,
            _request: &[u8],
        ) -> SdvResult<Pin<Box<dyn Stream<Item = SdvResult<Vec<u8>>> + Send>>> {
            unimplemented!()
        }
    }

    struct RpcClientStub {
        _transport: Box<dyn ClientTransport>,
    }

    impl RpcClient for RpcClientStub {
        const DEFAULT_UNIT_NAME: &'static str = "mock-rpc-stub";
        fn new(transport: Box<dyn ClientTransport>) -> Self {
            Self { _transport: transport }
        }
    }

    #[test]
    fn test_client_descriptor_to_attributes() {
        let desc = ClientDescriptor::<RpcClientStub>::new("main-channel")
            .with_service("sdv.sample.TireService");

        let (type_name, channel) = desc.to_attributes().expect("Failed to convert to attributes");
        assert_eq!(channel, "main-channel");
        assert_eq!(type_name.get_package_name(), "sdv.sample");
        assert_eq!(type_name.get_type_name(), "TireService");
    }
}
