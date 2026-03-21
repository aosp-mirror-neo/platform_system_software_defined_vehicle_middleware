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

use crate::{IntoRpcClientAttributes, IntoSubscriberAttributes};
use sdv_mw_rs_comms::{Availability, Communicate, UnitQuery};
use sdv_status_rs::SdvResult;
use std::pin::Pin;

/// Creates a stream that yields an [`Availability`] event whenever a publisher for the given
/// topic becomes available or unavailable.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `descriptor` - The subscriber descriptor to monitor for publisher availability.
///
/// # Returns
/// A `SdvResult` containing a pinned, boxed, dynamic `Stream` of `Availability` events.
pub async fn create_registration_event_stream<Descriptor>(
    comms: &dyn Communicate,
    descriptor: &Descriptor,
) -> SdvResult<Pin<Box<dyn futures::Stream<Item = Availability> + Send>>>
where
    Descriptor: IntoSubscriberAttributes,
{
    let (type_name_obj, topic) = descriptor.to_attributes()?;

    let unit_query = UnitQuery::Topic((type_name_obj, topic));
    comms.create_registration_event_stream(&unit_query).await
}

/// Creates a stream that yields an [`Availability`] event whenever a server becomes
/// available or unavailable.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `descriptor` - The client descriptor to monitor for availability.
///
/// # Returns
/// A `SdvResult` containing a pinned, boxed, dynamic `Stream` of `Availability` events on
/// Success.
pub async fn create_server_registration_event_stream<Descriptor>(
    comms: &dyn Communicate,
    descriptor: &Descriptor,
) -> sdv_status_rs::SdvResult<std::pin::Pin<Box<dyn futures::Stream<Item = Availability> + Send>>>
where
    Descriptor: IntoRpcClientAttributes,
{
    let (type_name, channel) = descriptor.to_attributes()?;

    comms.create_registration_event_stream(&UnitQuery::Channel((type_name, channel))).await
}
