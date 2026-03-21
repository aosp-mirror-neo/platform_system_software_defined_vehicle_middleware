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

//! Clientlib methods related to creating publications.

use sdv_mw_rs_comms::{Communicate, Publisher, PublisherAttributes};
use sdv_status_rs::SdvResult;

/// A trait for types that can be converted into publisher attributes.
pub trait IntoPublisherAttributes {
    /// The message type associated with the publisher.
    type Proto: protobuf::Message;

    /// Converts the type into publisher attributes.
    fn to_attributes(&self) -> SdvResult<PublisherAttributes>;
}

/// Creates a publisher for a specific publication topic.
///
/// This function initializes a publisher that can be used to send messages on the
/// specified topic. The publisher descriptor provides the necessary identity and metadata
/// for creating the publication.
///
/// # Arguments
/// * `comms` - A reference to the `Communicate` trait object for middleware communication.
/// * `descriptor` - The publisher descriptor for the publication.
///
/// # Returns
/// A `SdvResult` containing a `Publisher` for the topic's `Proto` type.
pub async fn create_publisher<Descriptor>(
    comms: &dyn Communicate,
    descriptor: &Descriptor,
) -> SdvResult<Publisher<Descriptor::Proto>>
where
    Descriptor: IntoPublisherAttributes,
{
    let attributes = descriptor.to_attributes()?;
    let publish_handle = comms.create_publish(&attributes).await?;
    Ok(Publisher::<Descriptor::Proto>::new(publish_handle))
}
