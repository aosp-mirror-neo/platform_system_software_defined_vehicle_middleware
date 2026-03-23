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

//! Mapping VSIDL-defined Topics to Publications.

use crate::publish::IntoPublisherAttributes;
use crate::subscribe::IntoSubscriberAttributes;
use sdv_mw_rs_comms::{PublisherAttributes, PublisherMetadata, TypeName};
use sdv_proto_reflection_rs::message_metadata::MessageMetadata;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::marker::PhantomData;

/// A descriptor that binds a specific topic to a publication (Code-gen use case).
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct PublisherDescriptor<Proto> {
    /// The name of the topic.
    pub(crate) topic: &'static str,
    /// The fully qualified name of the message type (e.g., "com.android.sdv.sample.Pressure").
    pub(crate) message: &'static str,
    /// The capacity of the publication queue.
    pub(crate) capacity: u32,
    /// The maximum size of a message on this topic.
    pub(crate) max_message_size: u32,
    /// The name of the service unit.
    pub(crate) service_unit_name: &'static str,
    /// Whether message metadata is required.
    pub(crate) is_message_metadata_required: bool,
    /// A function pointer to generate the metadata at runtime
    pub(crate) message_metadata_getter: fn() -> MessageMetadata,
    pub(crate) _marker: PhantomData<Proto>,
}

impl<Proto> PublisherDescriptor<Proto> {
    /// Creates a new publisher descriptor with mandatory fields.
    pub const fn new(topic: &'static str) -> Self {
        Self {
            topic,
            message: "",
            capacity: 0,
            max_message_size: 0,
            service_unit_name: "",
            is_message_metadata_required: false,
            message_metadata_getter: default_message_metadata,
            _marker: PhantomData,
        }
    }

    /// Sets the fully qualified message name.
    pub const fn with_message(mut self, name: &'static str) -> Self {
        self.message = name;
        self
    }

    /// Sets the capacity of the publication.
    pub const fn with_capacity(mut self, capacity: u32) -> Self {
        self.capacity = capacity;
        self
    }

    /// Sets the maximum message size.
    pub const fn with_max_message_size(mut self, size: u32) -> Self {
        self.max_message_size = size;
        self
    }

    /// Sets the service unit name.
    pub const fn with_service_unit_name(mut self, name: &'static str) -> Self {
        self.service_unit_name = name;
        self
    }

    /// Sets whether message metadata is required.
    pub const fn with_message_metadata_required(mut self, required: bool) -> Self {
        self.is_message_metadata_required = required;
        self
    }

    /// Sets the message metadata for this topic.
    pub const fn with_message_metadata(mut self, getter: fn() -> MessageMetadata) -> Self {
        self.message_metadata_getter = getter;
        self
    }
}

impl<Proto: protobuf::Message> IntoPublisherAttributes for PublisherDescriptor<Proto> {
    type Proto = Proto;

    fn to_attributes(&self) -> SdvResult<PublisherAttributes> {
        let (package, type_name) = self.message.rsplit_once('.').ok_or_else(|| {
            SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                format!("Invalid FQN: {}", self.message),
            )
        })?;
        let type_name_obj =
            TypeName::builder().package_name(package).type_name(type_name).build()?;

        PublisherAttributes::builder()
            .publication_name(self.service_unit_name)
            .type_name(type_name_obj)
            .metadata(
                PublisherMetadata::builder()
                    .message_size(self.max_message_size)
                    .message_count(self.capacity)
                    .message_metadata((self.message_metadata_getter)())
                    .message_metadata_required(self.is_message_metadata_required)
                    .build()?,
            )
            .topic_name(self.topic)
            .build()
    }
}

/// A descriptor that binds a specific topic to a publication (Manual use case).
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct PublisherDescriptorFull<Proto> {
    /// The name of the topic.
    pub(crate) topic: String,
    /// The capacity of the publication.
    pub(crate) capacity: u32,
    /// The name of the service unit.
    pub(crate) service_unit_name: String,

    pub(crate) _marker: PhantomData<Proto>,
}

impl<Proto: protobuf::MessageFull> PublisherDescriptorFull<Proto> {
    /// Creates a new publisher descriptor with the topic name.
    pub fn new(topic: impl Into<String>) -> Self {
        Self {
            topic: topic.into(),
            capacity: 8,
            service_unit_name: String::new(),
            _marker: PhantomData,
        }
    }

    /// Sets the capacity of the publication.
    pub fn with_capacity(mut self, capacity: u32) -> Self {
        self.capacity = capacity;
        self
    }

    /// Sets the service unit name.
    pub fn with_service_unit_name(mut self, name: impl Into<String>) -> Self {
        self.service_unit_name = name.into();
        self
    }
}

impl<Proto: protobuf::MessageFull> IntoPublisherAttributes for PublisherDescriptorFull<Proto> {
    type Proto = Proto;

    fn to_attributes(&self) -> SdvResult<PublisherAttributes> {
        let descriptor = Proto::descriptor();
        let fqn = descriptor.full_name();
        let (package, type_name) = fqn.rsplit_once('.').ok_or_else(|| {
            SdvStatus::with_message(SdvStatusCode::InvalidArgument, format!("Invalid FQN: {}", fqn))
        })?;
        let type_name_obj =
            TypeName::builder().package_name(package).type_name(type_name).build()?;

        let message_size = sdv_proto_utils::get_message_size::<Proto>()
            .map_err(|e| SdvStatus::with_message(SdvStatusCode::InvalidArgument, e.to_string()))?
            as u32;

        PublisherAttributes::builder()
            .publication_name(&self.service_unit_name)
            .type_name(type_name_obj)
            .metadata(
                PublisherMetadata::builder()
                    .message_size(message_size)
                    .message_count(self.capacity)
                    .build()?,
            )
            .topic_name(&self.topic)
            .build()
    }
}

/// A descriptor that binds a specific topic to a subscription (Code-gen use case).
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct SubscriberDescriptor<Proto> {
    /// The name of the topic.
    pub(crate) topic: &'static str,
    /// The fully qualified name of the message type (e.g., "sdv.sample.Pressure").
    pub(crate) message: &'static str,

    pub(crate) _marker: PhantomData<Proto>,
}

impl<Proto> SubscriberDescriptor<Proto> {
    /// Creates a new subscriber descriptor with mandatory fields.
    pub const fn new(topic: &'static str) -> Self {
        Self { topic, message: "", _marker: PhantomData }
    }

    /// Sets the fully qualified message name.
    pub const fn with_message(mut self, name: &'static str) -> Self {
        self.message = name;
        self
    }
}

impl<Proto: protobuf::Message> IntoSubscriberAttributes for SubscriberDescriptor<Proto> {
    type Proto = Proto;

    fn to_attributes(&self) -> SdvResult<(TypeName, String)> {
        let type_name_obj = TypeName::from_fqn(self.message)?;
        Ok((type_name_obj, self.topic.to_string()))
    }
}

/// A descriptor that binds a specific topic to a subscription (Manual use case).
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct SubscriberDescriptorFull<Proto> {
    /// The name of the topic.
    pub topic: String,
    pub(crate) _marker: PhantomData<Proto>,
}

impl<Proto: protobuf::MessageFull> SubscriberDescriptorFull<Proto> {
    /// Creates a new subscriber descriptor with the topic name.
    pub fn new(topic: impl Into<String>) -> Self {
        Self { topic: topic.into(), _marker: PhantomData }
    }
}

impl<Proto: protobuf::MessageFull> IntoSubscriberAttributes for SubscriberDescriptorFull<Proto> {
    type Proto = Proto;

    fn to_attributes(&self) -> SdvResult<(TypeName, String)> {
        let descriptor = Proto::descriptor();
        let fqn = descriptor.full_name();
        let type_name_obj = TypeName::from_fqn(fqn)?;
        Ok((type_name_obj, self.topic.clone()))
    }
}

fn default_message_metadata() -> MessageMetadata {
    MessageMetadata::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdv_mw_commstests_protos::test_messages::Pressure;

    #[test]
    fn test_publisher_descriptor_to_attributes() {
        let desc = PublisherDescriptor::<Pressure>::new("topic")
            .with_message("sdv.sample.Pressure")
            .with_capacity(10)
            .with_max_message_size(1024)
            .with_service_unit_name("service")
            .with_message_metadata_required(true);

        let attrs = desc.to_attributes().expect("Failed to convert to attributes");
        assert_eq!(attrs.get_topic_name(), "topic");
        assert_eq!(attrs.get_publication_name(), "service");
        assert_eq!(attrs.get_type_name().get_package_name(), "sdv.sample");
        assert_eq!(attrs.get_type_name().get_type_name(), "Pressure");
        assert_eq!(attrs.get_metadata().get_message_count(), 10);
        assert_eq!(attrs.get_metadata().get_message_size(), 1024);
        assert!(attrs.get_metadata().is_message_metadata_required());
    }

    #[test]
    fn test_publisher_descriptor_full_to_attributes() {
        let desc = PublisherDescriptorFull::<Pressure>::new("topic")
            .with_service_unit_name("service_full")
            .with_capacity(12);

        let attrs = desc.to_attributes().expect("Failed to convert to attributes");
        assert_eq!(attrs.get_topic_name(), "topic");
        assert_eq!(attrs.get_publication_name(), "service_full");
        // Pressure's FQN is sdv.mw.commstests.Pressure in the rust crate
        assert_eq!(attrs.get_type_name().get_package_name(), "sdv.mw.commstests");
        assert_eq!(attrs.get_type_name().get_type_name(), "Pressure");
        assert_eq!(attrs.get_metadata().get_message_count(), 12);
        // message_size is auto-calculated from Pressure proto
        assert!(attrs.get_metadata().get_message_size() > 0);
    }

    #[test]
    fn test_publisher_descriptor_full_defaults() {
        let desc = PublisherDescriptorFull::<Pressure>::new("topic");
        assert_eq!(desc.topic, "topic");
        assert_eq!(desc.capacity, 8); // default capacity
        assert_eq!(desc.service_unit_name, "");
    }

    #[test]
    fn test_subscriber_descriptor_to_attributes() {
        let desc =
            SubscriberDescriptor::<Pressure>::new("topic").with_message("sdv.sample.Pressure");

        let (type_name, topic) = desc.to_attributes().expect("Failed to convert to attributes");
        assert_eq!(topic, "topic");
        assert_eq!(type_name.get_package_name(), "sdv.sample");
        assert_eq!(type_name.get_type_name(), "Pressure");
    }

    #[test]
    fn test_subscriber_descriptor_full_to_attributes() {
        let desc = SubscriberDescriptorFull::<Pressure>::new("topic_full");

        let (type_name, topic) = desc.to_attributes().expect("Failed to convert to attributes");
        assert_eq!(topic, "topic_full");
        // Pressure's FQN is sdv.mw.commstests.Pressure in the rust crate
        assert_eq!(type_name.get_package_name(), "sdv.mw.commstests");
        assert_eq!(type_name.get_type_name(), "Pressure");
    }
}
