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

//! Mapping VSIDL-defined Channels to Publications.

// --- Publisher-specific Channel Traits ---

/// Contains traits related to publication channels.
///
/// The traits in this module are implemented by the `vsidlc` code generator
/// for channel types. Service bundle authors interact with types that implement
/// these traits, but should not implement them manually.
pub mod publisher {
    use crate::ToTypeName;
    use sdv_proto_reflection_rs::message_metadata::MessageMetadata;
    use std::fmt::Debug;

    /// The Channel trait represents a specific publication endpoint.
    ///
    /// Service bundle authors should treat types implementing this trait as opaque handles to be passed to the middleware clientlib.
    pub trait Channel: Identity + Metadata + ToTypeName + Debug {}
    impl<T: Identity + Metadata + ToTypeName + Debug> Channel for T {}

    /// A trait for vsidlc-generated publisher channel types to provide identity information.
    ///
    /// # Stability
    /// The implementation details of this trait (methods and associated constants) are not part of the public API and are unstable.
    /// They are subject to breaking changes or removal in any future version. Service bundle authors must not rely on these internal details.
    pub trait Identity {
        /// Associated type required to link a Channel to its Proto Message.
        type ProtoMessage: ::protobuf::Message;
        #[doc(hidden)]
        fn get_name(&self) -> &str;
        #[doc(hidden)]
        fn get_service_unit_name(&self) -> &str;
        #[doc(hidden)]
        fn get_capacity(&self) -> u32;
    }

    /// A trait for vsidlc-generated publisher channel types to provide metadata about the physical channel configuration.
    ///
    /// # Stability
    /// The implementation details of this trait (methods and associated constants) are not part of the public API and are unstable.
    /// They are subject to breaking changes or removal in any future version. Service bundle authors must not rely on these internal details.
    pub trait Metadata {
        #[doc(hidden)]
        const MESSAGE_SIZE: u32 = 0;
        #[doc(hidden)]
        const IS_MULTIPUB: bool = false;
        #[doc(hidden)]
        const IS_MESSAGE_METADATA_REQUIRED: bool = false;

        #[doc(hidden)]
        fn get_message_metadata() -> MessageMetadata {
            MessageMetadata::default()
        }
    }
}

// --- Subscriber-specific Channel Traits ---

/// Contains traits related to subscription channels.
///
/// The traits in this module are implemented by the `vsidlc` code generator
/// for channel types. Service bundle authors interact with types that implement
/// these traits, but should not implement them manually.
pub mod subscriber {
    use crate::ToTypeName;
    use std::fmt::Debug;

    /// The Channel trait represents a specific subscription endpoint.
    ///
    /// Service bundle authors should treat types implementing this trait as opaque handles to be passed to the middleware clientlib.
    pub trait Channel: Identity + Metadata + ToTypeName + Debug {}
    impl<T: Identity + Metadata + ToTypeName + Debug> Channel for T {}

    /// A trait for vsidlc-generated subscriber channel types to provide identity information.
    ///
    /// # Stability
    /// The implementation details of this trait (methods and associated constants) are not part of the public API and are unstable.
    /// They are subject to breaking changes or removal in any future version. Service bundle authors must not rely on these internal details.
    pub trait Identity {
        /// Associated type required to link a Channel to its Proto Message.
        type ProtoMessage: ::protobuf::Message;
        #[doc(hidden)]
        fn get_name(&self) -> &str;
        #[doc(hidden)]
        fn get_service_unit_name(&self) -> &str;
        #[doc(hidden)]
        fn get_service_bundle_package(&self) -> &str;
        #[doc(hidden)]
        fn get_service_bundle_name(&self) -> &str;
    }

    /// A trait for vsidlc-generated subscriber channel types to provide metadata about the physical channel configuration.
    ///
    /// # Stability
    /// The implementation details of this trait (methods and associated constants) are not part of the public API and are unstable.
    /// They are subject to breaking changes or removal in any future version. Service bundle authors must not rely on these internal details.
    pub trait Metadata {
        #[doc(hidden)]
        const IS_MULTIPUB: bool = false;
    }
}
