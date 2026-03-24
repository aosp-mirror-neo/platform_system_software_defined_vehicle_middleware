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

use futures::stream::Stream;
use sdv_mw_rs_comms::{
    observe, Communicate, History, InstantReader, SubscribeOptions, Subscriber, TypeName,
};
use sdv_status_rs::SdvResult;
use std::pin::Pin;

/// A trait for types that can be converted into subscriber attributes.
pub trait IntoSubscriberAttributes {
    /// The message type associated with the subscriber.
    type Proto: protobuf::Message;

    /// Converts the type into subscriber attributes (Type name and Topic name).
    fn to_attributes(&self) -> SdvResult<(TypeName, String)>;
}

/// Creates a new polling-based subscriber for a specific subscription topic.
///
/// A [`Subscriber`] allows for manually polling for new messages.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `descriptor` - The subscriber descriptor for the publication.
/// * `options` - Subscription options, which can specify details like `read_on_subscribe`.
///
/// # Returns
/// A `SdvResult` containing a `Subscriber` for the topic's `Proto` type.
pub async fn create_subscriber<Descriptor>(
    comms: &dyn Communicate,
    descriptor: &Descriptor,
    options: &SubscribeOptions,
) -> SdvResult<Subscriber<Descriptor::Proto>>
where
    Descriptor: IntoSubscriberAttributes,
{
    let (type_name_obj, topic) = descriptor.to_attributes()?;

    let subscribe_handle = comms.create_subscribe_handle(&type_name_obj, &topic, options).await?;
    Ok(Subscriber::<Descriptor::Proto>::new(subscribe_handle))
}

/// Creates a new event-driven observer for a specific subscription topic.
///
/// An observer returns a `Stream` that asynchronously yields new messages as they arrive.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `descriptor` - The subscriber descriptor for the publication.
/// * `options` - Subscription options, which can specify details like minimum callback interval.
///
/// # Returns
/// A `SdvResult` containing a pinned, boxed, dynamic `Stream` that yields vectors of the topic's `Proto` type.
pub async fn create_observer<Descriptor>(
    comms: &dyn Communicate,
    descriptor: &Descriptor,
    options: &SubscribeOptions,
) -> SdvResult<Pin<Box<dyn Stream<Item = SdvResult<Vec<Descriptor::Proto>>> + Send>>>
where
    Descriptor: IntoSubscriberAttributes,
{
    let (type_name_obj, topic) = descriptor.to_attributes()?;

    let byte_buffer_stream = comms.create_message_stream(&type_name_obj, &topic, options).await?;
    Ok(observe::<Descriptor::Proto>(byte_buffer_stream))
}

/// Creates a new history reader for a specific subscription topic.
///
/// A [`History`] reader allows for retrieving historical messages from a publication's buffer.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `descriptor` - The subscriber descriptor for the publication.
/// * `options` - Subscription options, which can specify details like the number of messages to cache.
///
/// # Returns
/// A `SdvResult` containing a `History` reader for the topic's `Proto` type.
pub async fn create_history<Descriptor>(
    comms: &dyn Communicate,
    descriptor: &Descriptor,
    options: &SubscribeOptions,
) -> SdvResult<History<Descriptor::Proto>>
where
    Descriptor: IntoSubscriberAttributes,
{
    let (type_name_obj, topic) = descriptor.to_attributes()?;

    let history_handle = comms.create_history_handle(&type_name_obj, &topic, options).await?;
    Ok(History::<Descriptor::Proto>::new(history_handle))
}

/// Creates a new instant reader for a specific subscription topic.
///
/// An [`InstantReader`] allows for retrieving the most recently published message on a topic.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `descriptor` - The subscriber descriptor for the publication.
/// * `options` - Subscription options.
///
/// # Returns
/// A `SdvResult` containing an `InstantReader` for the topic's `Proto` type.
pub async fn create_instant_reader<Descriptor>(
    comms: &dyn Communicate,
    descriptor: &Descriptor,
    _options: &SubscribeOptions,
) -> SdvResult<InstantReader<Descriptor::Proto>>
where
    Descriptor: IntoSubscriberAttributes,
{
    let (type_name_obj, topic) = descriptor.to_attributes()?;

    let instant_reader_handle = comms.create_instant_reader_handle(&type_name_obj, &topic).await?;
    Ok(InstantReader::<Descriptor::Proto>::new(instant_reader_handle))
}
