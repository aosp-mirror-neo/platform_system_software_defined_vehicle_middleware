// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use bytemuck::TransparentWrapper;
use sdv_comms_bindgen::{ASDV_CallbackVTable_SubscribeToPublication, ASDV_SubscriptionHandle};
use sdv_comms_common::{callback::sdk::OnceCallbackWrapper, owned_opaque_ptr::OwnedOpaquePtr};
use sdv_status_rs::{derive, SdvResult, SdvStatus, SdvStatusCode};
use std::ffi::c_void;

use crate::dt::SubscriberOptions;
use crate::id::UnitIdentifier;
use crate::sd::{PublisherUnitMetadata, UnitType};
use crate::ContextRef;

/// Data Tunnel Subscriber
/// This class is intended for reading messages from a subscription using active
/// polling. See the SubscriberStream class for asynchronous reading.
#[repr(transparent)]
pub struct Subscriber(pub(super) OwnedOpaquePtr<ASDV_SubscriptionHandle>);

static_assertions::assert_impl_all!(Subscriber: Send);
static_assertions::assert_not_impl_any!(Subscriber: Sync);

impl Subscriber {
    /// Non blocking.
    /// Subscribes to a publication specified in unit_identifier and creates
    /// a DataTunnel subscription. Creating more than one Subscriber instance for
    /// the same UnitIdentifier will create independent objects with separate
    /// subscription related resources.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_identifier: Unit Identifier of the publication to subscribe
    ///    [IN] options: see SubscriberOptions for details
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains SdvStatus of the
    ///   method call and, if method succeeded, a new Subscriber instance.
    #[allow(useless_deprecated)]
    #[allow(unused_attributes)]
    #[deprecated(since = "2.0.0", note = "Use Subscriber.create_for_topic instead")]
    pub async fn create(
        context: ContextRef,
        unit_identifier: &UnitIdentifier,
        options: SubscriberOptions,
    ) -> SdvResult<Subscriber> {
        type SubscribeToPublisherCb = OnceCallbackWrapper<OwnedOpaquePtr<ASDV_SubscriptionHandle>>;

        let (cb, fut) = SubscribeToPublisherCb::new_async();
        let vtable = ASDV_CallbackVTable_SubscribeToPublication {
            func: Some(SubscribeToPublisherCb::trampoline_func),
            clear: Some(SubscribeToPublisherCb::trampoline_clear),
            cb_data: cb.into_void_ptr(),
        };

        // SAFETY: `context` and `unit_identifier` are valid pointers to corresponding types.
        // `vtable` represents the correct Vtable: ASDV_CallbackVTable_SubscribeToPublication.
        // The pointer returned by the callback will be cleared through the Drop trait
        // implementation of struct [Subscriber].
        unsafe {
            sdv_comms_bindgen::ASDV_SubscribeToPublication(
                context.as_raw(),
                UnitIdentifier::peel_ref(unit_identifier),
                options.as_raw_flags(),
                vtable,
            );
        }

        let handle = fut.await.map_err(derive!(SdvStatusCode::Internal))??;
        Ok(Subscriber(handle))
    }

    /// Non blocking.
    /// Subscribes to all publications of the specified unit_type and wires them into a single
    /// DataTunnel subscription. The subscription is made dynamically to all current and future
    /// publications of the specified UnitType. As a result, the object is created even if there
    /// are no publications of this UnitType at the moment. Creating more than one Subscriber
    /// instance for the same UnitType will create independent objects with separate
    /// subscription related resources.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_type: Unit Type of the publications to subscribe
    ///    [IN] options: see SubscriberOptions for details
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains SdvStatus of the
    ///   method call and, if method succeeded, a new Subscriber instance.
    #[allow(useless_deprecated)]
    #[allow(unused_attributes)]
    #[deprecated(since = "2.0.0", note = "Use Subscriber.create_for_topic instead")]
    pub async fn create_for_multipub(
        context: ContextRef,
        unit_type: &UnitType,
        options: SubscriberOptions,
    ) -> SdvResult<Subscriber> {
        type SubscribeToUnitTypeCb = OnceCallbackWrapper<OwnedOpaquePtr<ASDV_SubscriptionHandle>>;

        let (cb, fut) = SubscribeToUnitTypeCb::new_async();
        let vtable = ASDV_CallbackVTable_SubscribeToPublication {
            func: Some(SubscribeToUnitTypeCb::trampoline_func),
            clear: Some(SubscribeToUnitTypeCb::trampoline_clear),
            cb_data: cb.into_void_ptr(),
        };

        // SAFETY: `context` and `unit_type` are valid pointers to corresponding types.
        // `vtable` represents the correct Vtable: ASDV_CallbackVTable_SubscribeToPublication.
        // The pointer returned by the callback will be cleared through the Drop trait
        // implementation of struct [Subscriber].
        unsafe {
            sdv_comms_bindgen::ASDV_SubscribeToUnitType(
                context.as_raw(),
                UnitType::peel_ref(unit_type),
                options.as_raw_flags(),
                vtable,
            );
        }

        let handle = fut.await.map_err(derive!(SdvStatusCode::Internal))??;
        Ok(Subscriber(handle))
    }

    /// Non blocking.
    /// Subscribes to all publications of the specified unit_type and cannel_name and wires them into a single
    /// DataTunnel subscription. The subscription is made dynamically to all current and future
    /// publications of the specified UnitType and cannel_name. Creating more than one Subscriber
    /// instance for the same UnitType and cannel_name will create independent objects with separate
    /// subscription related resources.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_type: Unit Type of the publications to subscribe
    ///    [IN] topic_name: The name of the topic to subscribe to
    ///    [IN] options: see SubscriberOptions for details
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains SdvStatus of the
    ///   method call and, if method succeeded, a new Subscriber instance.
    pub async fn create_for_topic(
        context: ContextRef,
        unit_type: &UnitType,
        topic_name: &str,
        options: SubscriberOptions,
    ) -> SdvResult<Subscriber> {
        type SubscribeToTopicCb = OnceCallbackWrapper<OwnedOpaquePtr<ASDV_SubscriptionHandle>>;

        let (cb, fut) = SubscribeToTopicCb::new_async();
        let vtable = ASDV_CallbackVTable_SubscribeToPublication {
            func: Some(SubscribeToTopicCb::trampoline_func),
            clear: Some(SubscribeToTopicCb::trampoline_clear),
            cb_data: cb.into_void_ptr(),
        };

        let topic_name_cstr =
            std::ffi::CString::new(topic_name).map_err(derive!(SdvStatusCode::Internal))?;
        // SAFETY: `context`, `unit_type` and `topic_name_cstr` are valid pointers to
        // corresponding types.
        // `vtable` represents the correct Vtable: ASDV_CallbackVTable_SubscribeToPublication.
        // The pointer returned by the callback will be cleared through the Drop trait
        // implementation of struct [Subscriber].
        unsafe {
            sdv_comms_bindgen::ASDV_SubscribeToTopic(
                context.as_raw(),
                UnitType::peel_ref(unit_type),
                topic_name_cstr.as_ptr(),
                options.as_raw_flags(),
                vtable,
            );
        }

        let handle = fut.await.map_err(derive!(SdvStatusCode::Internal))??;
        Ok(Subscriber(handle))
    }

    /// Non blocking.
    /// Returns the publication metadata that contains the message size in bytes
    /// along with other publication related metadata.
    ///
    /// Parameters:
    ///
    /// Return value:
    ///    publisher unit metadata struct object.
    pub fn get_publisher_unit_metadata(&self) -> PublisherUnitMetadata {
        let mut metadata = PublisherUnitMetadata::new();

        // SAFETY: self.0 is a valid pointer to [ASDV_PublicationHandle] and metadata is a newly
        // constructed struct.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_GetPublisherUnitMetadataFromSubscriber(
                self.0.as_ptr(),
                metadata.as_raw_mut(),
            )
        };

        SdvStatusCode::from(status)
            .expect("sdv_comms_bindgen::ASDV_GetPublisherUnitMetadataFromSubscriber() failed");

        metadata
    }

    /// Non blocking.
    /// Reports the number of new messages available to read.
    ///
    /// Parameters:
    ///
    /// Return value:
    ///   SdvResult that contains SdvStatus of the method call and, if
    ///   method succeeded, the amount of messages available to read.
    ///   See ASDV_AvailableToRead() in libsdv_dt API for more details
    ///   regarding buffer overflow handling.
    pub fn available_to_read(&self) -> SdvResult<u32> {
        let mut count = 0;
        // SAFETY: self.0 is a valid pointer to [ASDV_SubscriptionHandle].
        let status =
            unsafe { sdv_comms_bindgen::ASDV_AvailableToRead(self.0.as_ptr(), &mut count) };

        match SdvStatusCode::from(status) {
            SdvStatusCode::Ok => Ok(count),
            sdv_code => Err(SdvStatus::new(sdv_code)),
        }
    }

    /// Non blocking.
    /// Reads messages from the publication's buffer.
    ///
    /// Parameters:
    ///    [IN] messages: An array of bytes where the received messages will
    ///                  be written.
    ///                  The size of the array must be divisible by the size
    ///                  of a message.
    ///
    /// Return value:
    ///    SdvResult that contains SdvStatus of the method call and, if
    ///    method succeeded, the number of messages that were read from
    ///    the publication and stored to the "messages" buffer.
    ///    See ASDV_ReadMessages() in libsdv_dt API for more details regarding
    ///    buffer overflow handling.
    ///
    /// Important Note:
    ///    This function may modify the contents of the `messages` buffer
    ///    even when returning an error; for performance reasons it cannot
    ///    detect all error conditions in advance of any writes.  Callers
    ///    should not make any assumptions about the buffer contents on error.
    pub fn read_messages(&self, messages: &mut [u8]) -> SdvResult<u32> {
        let mut read_count = 0;
        // SAFETY: self.0 is a valid pointer to [ASDV_SubscriptionHandle].
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ReadMessages(
                self.0.as_mut_ptr(),
                messages.as_mut_ptr() as *mut c_void,
                messages.len() as u32,
                &mut read_count,
            )
        };

        match SdvStatusCode::from(status) {
            SdvStatusCode::Ok => Ok(read_count),
            sdv_code => Err(SdvStatus::new(sdv_code)),
        }
    }
}

/// Destructor automatically invokes ASDV_UnsubscribeFromPublication()
impl Drop for Subscriber {
    fn drop(&mut self) {
        // SAFETY: self.0 is a valid pointer to [ASDV_SubscriptionHandle].
        let status =
            unsafe { sdv_comms_bindgen::ASDV_UnsubscribeFromPublication(self.0.as_mut_ptr()) };

        SdvStatusCode::from(status).expect("Subscriber could not be destroyed");
    }
}
