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

use crate::{subscriber, to_unit_name};
use sdv_mw_rs_comms::{Availability, Communicate, SubscribeOptions, UnitName, UnitQuery};
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
    options: &SubscribeOptions,
) -> SdvResult<Pin<Box<dyn futures::Stream<Item = Availability> + Send>>> {
    let unit_query = if Channel::IS_MULTIPUB {
        UnitQuery::Type(Channel::type_name()?)
    } else {
        UnitQuery::Name(to_unit_name(channel, options)?)
    };
    comms.create_registration_event_stream(&unit_query).await
}

/// Creates a stream that yields an [`Availability`] event whenever a server becomes
/// available or unavailable.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `server` - The `UnitName` of the server to monitor for availability.
///
/// # Returns
/// A `SdvResult` containing a pinned, boxed, dynamic `Stream` of `Availability` events on
/// success.
pub async fn create_server_registration_event_stream(
    comms: &dyn Communicate,
    server: UnitName,
) -> sdv_status_rs::SdvResult<std::pin::Pin<Box<dyn futures::Stream<Item = Availability> + Send>>> {
    comms.create_registration_event_stream(&UnitQuery::Name(server)).await
}
