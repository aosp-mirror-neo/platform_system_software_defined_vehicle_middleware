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

//! Clientlib methods related to RPC server

use crate::ToTypeName;
use sdv_mw_rs_comms::{Communicate, ServerAttributes, ServerOptions, ServerTransport};
use sdv_status_rs::SdvResult;

/// Creates a server-side transport for handling RPC requests.
///
/// This function sets up a transport layer for a server based on the provided
/// attributes. The server will be capable of handling various types of RPC
/// methods, including unary, client-streaming, and server-streaming.
///
/// # Type Parameters
///
/// * `Server` - The service interface trait, which must implement `ToTypeName`. This provides
///   metadata about the service, such as its package and type name.
///
/// # Arguments
///
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `channel` - The communication channel identifier used as the server instance name where
///   this RPC server will listen for requests.
/// * `methods` - A map from method names to their `RpcMethod` implementations.
///   This tells the transport how to handle incoming requests for each method.
/// * `options` - `ServerOptions` to configure the server.
///
/// # Returns
///
/// A `SdvResult` containing an `Arc<dyn ServerTransport>` on success, which
/// represents the active server transport. On failure, it returns an error.
pub async fn create_server<Server: ToTypeName + ?Sized>(
    comms: &dyn Communicate,
    channel: impl Into<String>,
    methods: std::collections::HashMap<String, sdv_comms::rpc::RpcMethod>,
    options: ServerOptions,
) -> SdvResult<std::sync::Arc<dyn ServerTransport>> {
    let attributes = ServerAttributes::builder()
        .server_name(channel)
        .type_name(Server::type_name()?)
        .server_options(options)
        .build()?;

    let transport = comms.create_server_transport_all_methods(&attributes, methods).await?;
    Ok(transport.into())
}
