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

use async_trait::async_trait;
use futures::Stream;
use sdv_mw_rs_comms::{ClientTransport, ServerTransport};
use sdv_status_rs::SdvResult;
use std::pin::Pin;

pub struct SdvServerTransport {
    _sdv_server: sdv_comms::rpc::Server,
}

impl SdvServerTransport {
    pub fn new(sdv_server: sdv_comms::rpc::Server) -> Self {
        Self { _sdv_server: sdv_server }
    }
}

impl ServerTransport for SdvServerTransport {}

pub struct SdvClientTransport {
    client: sdv_comms::rpc::Client,
}

impl SdvClientTransport {
    pub fn new(client: sdv_comms::rpc::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl ClientTransport for SdvClientTransport {
    /// Calls a method with method name and request bytes.
    async fn call_method(&self, method_name: &str, request: &[u8]) -> SdvResult<Vec<u8>> {
        self.client.unary_call(method_name, request).await
    }

    /// Calls a method with method name and request bytes using client streaming.
    async fn call_client_streaming_method(
        &self,
        method_name: &str,
        request_stream: Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>,
    ) -> SdvResult<Vec<u8>> {
        self.client.client_streaming_call(method_name, request_stream).await
    }

    /// Calls a method with method name and request bytes using server streaming.
    async fn call_server_streaming_method(
        &self,
        method_name: &str,
        request: &[u8],
    ) -> SdvResult<Pin<Box<dyn Stream<Item = SdvResult<Vec<u8>>> + Send>>> {
        self.client.server_streaming_call(method_name, request).await
    }
}
