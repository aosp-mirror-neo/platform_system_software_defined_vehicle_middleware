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

//! Clientlib methods related to subscribing to publications.

use crate::subscriber;
use futures::stream::Stream;
use sdv_mw_rs_comms::{observe, Communicate, History, InstantReader, SubscribeOptions, Subscriber};
use sdv_status_rs::SdvResult;
use std::pin::Pin;

/// Creates a new polling-based subscriber for a specific subscription channel.
///
/// A [`Subscriber`] allows for manually polling for new messages.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `channel` - The subscription channel to subscribe to.
/// * `options` - Subscription options, which can specify details like the target VM or instance.
///
/// # Returns
/// A `SdvResult` containing a `Subscriber` for the channel's `ProtoMessage` type.
pub async fn create_subscriber<Channel: subscriber::Channel>(
    comms: &dyn Communicate,
    channel: &Channel,
    options: &SubscribeOptions,
) -> SdvResult<Subscriber<Channel::ProtoMessage>> {
    let subscribe_handle =
        comms.create_subscribe_handle(&Channel::type_name()?, channel.get_name(), options).await?;
    Ok(Subscriber::<Channel::ProtoMessage>::new(subscribe_handle))
}

/// Creates a new event-driven observer for a specific subscription channel.
///
/// An observer returns a `Stream` that asynchronously yields new messages as they arrive.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `channel` - The subscription channel to observe.
/// * `options` - Subscription options, which can specify details like the target VM or instance.
///
/// # Returns
/// A `SdvResult` containing a pinned, boxed, dynamic `Stream` that yields vectors of the channel's `ProtoMessage` type.
pub async fn create_observer<Channel: subscriber::Channel>(
    comms: &dyn Communicate,
    channel: &Channel,
    options: &SubscribeOptions,
) -> SdvResult<Pin<Box<dyn Stream<Item = SdvResult<Vec<Channel::ProtoMessage>>> + Send>>> {
    let byte_buffer_stream =
        comms.create_message_stream(&Channel::type_name()?, channel.get_name(), options).await?;
    Ok(observe::<Channel::ProtoMessage>(byte_buffer_stream))
}

/// Creates a new history reader for a specific subscription channel.
///
/// A [`History`] reader allows for retrieving historical messages from a publication's buffer.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `channel` - The subscription channel to get history from.
/// * `options` - Subscription options, which can specify details like the number of messages to cache.
///
/// # Returns
/// A `SdvResult` containing a `History` reader for the channel's `ProtoMessage` type.
pub async fn create_history<Channel: subscriber::Channel>(
    comms: &dyn Communicate,
    channel: &Channel,
    options: &SubscribeOptions,
) -> SdvResult<History<Channel::ProtoMessage>> {
    let history_handle =
        comms.create_history_handle(&Channel::type_name()?, channel.get_name(), options).await?;
    Ok(History::<Channel::ProtoMessage>::new(history_handle))
}

/// Creates a new instant reader for a specific subscription channel.
///
/// An [`InstantReader`] allows for retrieving the most recently published message on a channel.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `channel` - The subscription channel to read from.
/// * `options` - Subscription options, which can specify details like the target VM or instance.
///
/// # Returns
/// A `SdvResult` containing an `InstantReader` for the channel's `ProtoMessage` type.
pub async fn create_instant_reader<Channel: subscriber::Channel>(
    comms: &dyn Communicate,
    channel: &Channel,
    _options: &SubscribeOptions,
) -> SdvResult<InstantReader<Channel::ProtoMessage>> {
    let instant_reader_handle =
        comms.create_instant_reader_handle(&Channel::type_name()?, channel.get_name()).await?;
    Ok(InstantReader::<Channel::ProtoMessage>::new(instant_reader_handle))
}
