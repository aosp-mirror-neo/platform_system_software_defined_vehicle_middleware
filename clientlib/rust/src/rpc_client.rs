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

//! Clientlib methods related to RPC client

use crate::rpc_descriptor::client;
use sdv_mw_rs_comms::{ClientOptions, Communicate, RpcClient};

/// Creates an RPC client for a specified client descriptor.
///
/// This function attempts to connect to an RPC server identified by the descriptor.
/// If the connection is successful, it returns a client stub that can be used to
/// make remote procedure calls.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `descriptor` - The client descriptor.
/// * `options` - `ClientOptions` to configure the client.
///
/// # Returns
/// An `SdvResult` containing the RPC client stub of type `Client` on success.
pub async fn create_rpc_client<Descriptor: client::Descriptor>(
    comms: &dyn Communicate,
    descriptor: &Descriptor,
    options: ClientOptions,
) -> sdv_status_rs::SdvResult<Descriptor::ClientType> {
    Ok(Descriptor::ClientType::new(
        comms
            .create_client_transport_for_channel(
                &Descriptor::type_name()?,
                descriptor.get_channel(),
                &options,
            )
            .await?,
    ))
}
