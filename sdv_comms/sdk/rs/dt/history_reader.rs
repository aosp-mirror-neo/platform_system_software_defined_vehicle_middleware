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
use sdv_comms_bindgen::{ASDV_CallbackVTable_SubscribeToHistory, ASDV_HistoryHandle};
use sdv_comms_common::{callback::sdk::OnceCallbackWrapper, owned_opaque_ptr::OwnedOpaquePtr};
use sdv_status_rs::{derive, SdvResult, SdvStatus, SdvStatusCode};
use std::ffi::c_void;

use crate::id::UnitIdentifier;
use crate::sd::{PublisherUnitMetadata, UnitType};
use crate::ContextRef;

/// Data Tunnel History Reader
#[repr(transparent)]
pub struct HistoryReader(OwnedOpaquePtr<ASDV_HistoryHandle>);

static_assertions::assert_impl_all!(HistoryReader: Send);
static_assertions::assert_not_impl_any!(HistoryReader: Sync);

impl HistoryReader {
    /// Non blocking.
    /// Subscribes to a publication specified by unit_identifier and creates
    /// a "history_buffer_size"-size history aggregator for the publication.
    /// Creating more than one HistoryReader instance for the same
    /// ServiceUnitIdentity will create independent objects with separate
    /// buffers and subscription related resources.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_identifier: Unit Identifier of the publication to subscribe
    ///    [IN] history_buffer_size: Number if messages to store in history
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains SdvStatus of the
    ///   method call and, if method succeeded, a new HistoryReader instance.
    pub async fn create(
        context: ContextRef,
        unit_identifier: &UnitIdentifier,
        history_buffer_size: u32,
    ) -> SdvResult<HistoryReader> {
        type SubscribeToHistoryCb = OnceCallbackWrapper<OwnedOpaquePtr<ASDV_HistoryHandle>>;

        let (cb, fut) = SubscribeToHistoryCb::new_async();
        let vtable = ASDV_CallbackVTable_SubscribeToHistory {
            func: Some(SubscribeToHistoryCb::trampoline_func),
            clear: Some(SubscribeToHistoryCb::trampoline_clear),
            cb_data: cb.into_void_ptr(),
        };

        // SAFETY: TODO
        unsafe {
            sdv_comms_bindgen::ASDV_SubscribeToHistory(
                context.as_raw(),
                UnitIdentifier::peel_ref(unit_identifier),
                history_buffer_size,
                vtable,
            );
        }

        let handle = fut.await.map_err(derive!(SdvStatusCode::Internal))??;
        Ok(HistoryReader(handle))
    }

    /// Non blocking.
    /// Subscribes to all publications of specified unit_type, wires them into a single
    /// DataTunnel publication and creates a "history_buffer_size"-size history aggregator
    /// for it. The subscription is made dynamically to all current and future
    /// publications of the specified UnitType. As a result, the object is created even if there
    /// are no publications of this UnitType at the moment. Creating more than one HistoryReader
    /// instance for the same UnitType will create independent objects with separate buffers and
    /// subscription related resources.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_type: Unit Type of the publications to subscribe
    ///    [IN] history_buffer_size: Number if messages to store in history
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains SdvStatus of the
    ///   method call and, if method succeeded, a new HistoryReader instance.
    pub async fn create_for_multipub(
        _context: ContextRef,
        _unit_type: &UnitType,
        _history_buffer_size: u32,
    ) -> SdvResult<HistoryReader> {
        unimplemented!()
    }

    /// Non blocking.
    /// Subscribes to all publications of specified unit_type and topic_name, wires them into a
    /// single DataTunnel publication and creates a "history_buffer_size"-size history aggregator
    /// for it. The subscription is made dynamically to all current and future publications of
    /// the specified UnitType and topic_name. Creating more than one HistoryReader instance for
    /// the same UnitType and topic_name will create independent objects with separate buffers and
    /// subscription related resources.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_type: Unit Type of the publications to subscribe
    ///    [IN] topic_name: The topic_name to subscribe to
    ///    [IN] history_buffer_size: Number of messages to store in history
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains SdvStatus of the
    ///   method call and, if method succeeded, a new HistoryReader instance.
    pub async fn create_for_topic(
        context: ContextRef,
        unit_type: &UnitType,
        topic_name: &str,
        history_buffer_size: u32,
    ) -> SdvResult<HistoryReader> {
        type SubscribeToHistoryCb = OnceCallbackWrapper<OwnedOpaquePtr<ASDV_HistoryHandle>>;

        let (cb, fut) = SubscribeToHistoryCb::new_async();
        let vtable = ASDV_CallbackVTable_SubscribeToHistory {
            func: Some(SubscribeToHistoryCb::trampoline_func),
            clear: Some(SubscribeToHistoryCb::trampoline_clear),
            cb_data: cb.into_void_ptr(),
        };

        let topic_name_cstr = std::ffi::CString::new(topic_name).unwrap();

        // SAFETY: `context.as_raw()`: `context` is a `ContextRef`, which wraps a `&'static
        //   ASDV_Context`. This guarantees the pointer is valid for the program's lifetime.
        // `topic_name_cstr.as_ptr()`: `topic_name_cstr` is a `std::ffi::CString` created from the
        //   `topic_name` string slice. This ensures it's a valid NUL-terminated C string, and it
        //   lives for the duration of this function scope.
        unsafe {
            sdv_comms_bindgen::ASDV_SubscribeToHistoryByTopic(
                context.as_raw(),
                UnitType::peel_ref(unit_type),
                topic_name_cstr.as_ptr(),
                history_buffer_size,
                vtable,
            );
        }

        let handle = fut.await.map_err(derive!(SdvStatusCode::Internal))??;
        Ok(HistoryReader(handle))
    }

    /// Non blocking.
    /// Returns the publication metadata that contains the message size in bytes
    /// along with other publication related metadata.
    ///
    /// Parameters:
    ///
    /// Return value:
    ///     publisher unit metadata struct object.
    pub fn get_publisher_unit_metadata(&self) -> PublisherUnitMetadata {
        let mut metadata = PublisherUnitMetadata::new();

        // SAFETY: self.0 is a valid pointer to [ASDV_PublicationHandle] and metadata is a newly
        // constructed struct.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_GetPublisherUnitMetadataFromHistory(
                self.0.as_ptr(),
                metadata.as_raw_mut(),
            )
        };

        SdvStatusCode::from(status)
            .expect("sdv_comms_bindgen::ASDV_GetPublisherUnitMetadataFromHistory() failed");

        metadata
    }

    /// Non blocking.
    /// Reports the number of messages available in the history buffer.
    ///
    /// Parameters:
    ///
    /// Return value:
    ///    SdvResult that contains SdvStatus of the method call and, if
    ///    method succeeded, the amount of messages available to read.
    ///    See ASDV_AvailableToRead() in libsdv_dt API for more details
    ///    regarding buffer overflow handling.
    pub fn available_to_read(&self) -> SdvResult<u32> {
        let mut count = 0;
        // SAFETY: self.0 is a valid pointer to [ASDV_HistoryHandle].
        let status =
            unsafe { sdv_comms_bindgen::ASDV_AvailableInHistory(self.0.as_ptr(), &mut count) };

        match SdvStatusCode::from(status) {
            SdvStatusCode::Ok => Ok(count),
            sdv_code => Err(SdvStatus::new(sdv_code)),
        }
    }

    /// Non blocking.
    /// Reads messages from the publication's history buffer.
    /// Reading the last messages is non-destructive, i.e. the same message
    /// can be read multiple times.
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
    ///    the history and stored to the "messages" buffer.
    ///    See ASDV_ReadMessages() in libsdv_dt API for more details regarding
    ///    buffer overflow handling.
    pub fn read_last_messages(&self, messages: &mut [u8]) -> SdvResult<u32> {
        let mut read_count = 0;
        // SAFETY: self.0 is a valid pointer to [ASDV_SubscriptionHandle].
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ReadHistoryMessages(
                self.0.as_ptr(),
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

/// Destructor automatically invokes ASDV_UnsubscribeFromHistory()
impl Drop for HistoryReader {
    fn drop(&mut self) {
        // SAFETY: self.0 is a valid pointer to [ASDV_HistoryHandle].
        let status = unsafe { sdv_comms_bindgen::ASDV_UnsubscribeFromHistory(self.0.as_ptr()) };

        SdvStatusCode::from(status).expect("ASDV_UnsubscribeFromHistory() fails!");
    }
}
