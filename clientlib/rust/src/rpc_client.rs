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

use sdv_mw_rs_comms::{ClientOptions, Communicate, RpcClient, TypeName};
use sdv_status_rs::SdvResult;

/// A trait for types that can be converted into RPC client attributes.
pub trait IntoRpcClientAttributes {
    /// The RPC client stub type.
    type Client: RpcClient;

    /// Converts the type into RPC client attributes (Type name and Channel name).
    fn to_attributes(&self) -> SdvResult<(TypeName, String)>;
}

/// Creates an RPC client for a specified RPC descriptor.
///
/// This function attempts to connect to an RPC server identified by the descriptor.
/// If the connection is successful, it returns a client stub that can be used to
/// make remote procedure calls.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `descriptor` - The RPC descriptor for the service.
/// * `options` - Client options.
///
/// # Returns
/// An `SdvResult` containing the RPC client stub of type `Descriptor::Client` on success.
pub async fn create_rpc_client<Descriptor>(
    comms: &dyn Communicate,
    descriptor: &Descriptor,
    options: ClientOptions,
) -> SdvResult<Descriptor::Client>
where
    Descriptor: IntoRpcClientAttributes,
{
    let (type_name, channel) = descriptor.to_attributes()?;
    let transport =
        comms.create_client_transport_for_channel(&type_name, &channel, &options).await?;
    Ok(Descriptor::Client::new(transport))
}
