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

//! SDV Middleware Clientlib methods related to availability.

use crate::rpc_descriptor::client;
use crate::subscriber;
use sdv_mw_rs_comms::{Availability, Communicate, UnitQuery};
use sdv_status_rs::SdvResult;
use std::pin::Pin;

/// Creates a stream that yields an [`Availability`] event whenever a publisher for the given
/// channel becomes available or unavailable.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `channel` - The subscription channel to monitor for publisher availability.
/// * `options` - Subscription options, which can specify details like the target VM or instance
///   for single-pub channels.
///
/// # Returns
/// A `SdvResult` containing a pinned, boxed, dynamic `Stream` of `Availability` events.
pub async fn create_registration_event_stream<Channel: subscriber::Channel>(
    comms: &dyn Communicate,
    channel: &Channel,
) -> SdvResult<Pin<Box<dyn futures::Stream<Item = Availability> + Send>>> {
    let unit_query = UnitQuery::Topic((Channel::type_name()?, channel.get_name().to_string()));
    comms.create_registration_event_stream(&unit_query).await
}

/// Creates a stream that yields an [`Availability`] event whenever a server becomes
/// available or unavailable.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `descriptor` - The descriptor to monitor for availability.
///
/// # Returns
/// A `SdvResult` containing a pinned, boxed, dynamic `Stream` of `Availability` events on
/// success.
pub async fn create_server_registration_event_stream<Descriptor: client::Descriptor>(
    comms: &dyn Communicate,
    descriptor: &Descriptor,
) -> sdv_status_rs::SdvResult<std::pin::Pin<Box<dyn futures::Stream<Item = Availability> + Send>>> {
    let channel_name = descriptor.get_channel();
    let type_name = Descriptor::type_name()?;
    comms
        .create_registration_event_stream(&UnitQuery::Channel((
            type_name,
            channel_name.to_string(),
        )))
        .await
}
