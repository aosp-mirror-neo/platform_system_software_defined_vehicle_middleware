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
use core::ffi::c_void;
use sdv_comms_bindgen::{
    ASDV_CallbackVTable_GetLastPublicationMessage, ASDV_MaxInstantReadMessageSize,
};
use sdv_comms_common::callback::sdk::OnceCallbackWrapper;
use sdv_status_rs::{derive, SdvResult, SdvStatusCode};
use std::mem::ManuallyDrop;
use std::sync::Mutex;

use crate::dt::HistoryReader;
use crate::id::UnitIdentifier;
use crate::sd::UnitType;
use crate::ContextRef;

/// Data Tunnel Instant Reader
///
/// This struct provides a way to fetch the last message published to a
/// specific topic, potentially by multiple publishers.
///
/// **Note:** This implementation uses a HistoryReader with a buffer size of 1
/// internally. This means that the first call to `read_last_message` on a new
/// topic might not return the absolute latest message if messages were published
/// *before* this `InstantReader` instance was created. It will return
/// the latest message received *since the underlying HistoryReader started listening*.
pub struct InstantReader {
    // TODO(b/465019104) - use per message metadata to dynamically retrieve the latest message
    // instead of rely on HistoryReader.
    history_reader: Mutex<HistoryReader>,
}

static_assertions::assert_impl_all!(InstantReader: Send, Sync);

impl InstantReader {
    /// Non blocking.
    /// Returns the last published message in the specified publication without
    /// subscribing to it.
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_identifier: Unit Identifier of the publication to read data from
    /// Return value:
    ///    Returns a Future of SdvResult that contains SdvStatus of the
    ///    method call and, if method succeeded, an array with the last
    ///    message of the specified publication. If the specified publication
    ///    has no messages, the method will return Ok with an empty buffer.
    ///
    #[allow(useless_deprecated)]
    #[allow(unused_attributes)]
    #[deprecated(since = "2.0.0", note = "Use InstantReader.read_last_message instead")]
    pub async fn get_last_message(
        context: ContextRef,
        unit_identifier: &UnitIdentifier,
    ) -> SdvResult<Vec<u8>> {
        type GetLastPublicationMessageCb = OnceCallbackWrapper<u32>;

        let (cb, fut) = GetLastPublicationMessageCb::new_async();
        let vtable = ASDV_CallbackVTable_GetLastPublicationMessage {
            func: Some(GetLastPublicationMessageCb::trampoline_func),
            clear: Some(GetLastPublicationMessageCb::trampoline_clear),
            cb_data: cb.into_void_ptr(),
        };

        // The maximum size of a message that can be received via InstantRead is limited to the
        // value of ASDV_MaxInstantReadMessageSize due to Binder limitations and overall system
        // performance reasons.
        // TODO(b/368703036): Splitting message into smaller chunks could solve this limitation
        //                    problem.
        const BUFFER_SIZE: u32 = ASDV_MaxInstantReadMessageSize;
        let mut bytes = ManuallyDrop::new(vec![0u8; BUFFER_SIZE as usize]);

        // SAFETY: Invoking ASDV_ function that crossing FFI layer
        // see `sdv_comms/dt/libsdv_comms_dt/src/ffi/ffi_impl.rs` for details
        unsafe {
            sdv_comms_bindgen::ASDV_GetLastPublicationMessage(
                context.as_raw(),
                UnitIdentifier::peel_ref(unit_identifier),
                bytes.as_mut_ptr() as *mut c_void,
                BUFFER_SIZE,
                vtable,
            );
        }

        let message_size = fut.await.map_err(derive!(SdvStatusCode::Internal))??;
        bytes.truncate(message_size as usize);
        // TODO(b/369137307): Using ManuallyDrop as a temporary workaround to avoid potential
        //                    use-after-free.
        Ok(ManuallyDrop::into_inner(bytes))
    }

    /// Creates a new `InstantReader` for the given UnitType and topic name.
    ///
    /// This function is non-blocking and will return even if no publishers
    /// are currently active on the topic.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object.
    ///    [IN] unit_type: Unit Type of the publications to monitor.
    ///    [IN] topic: The topic name to monitor.
    pub async fn create_for_topic(
        context: ContextRef,
        unit_type: &UnitType,
        topic: &str,
    ) -> SdvResult<InstantReader> {
        // Create an internal HistoryReader with a buffer size of 1.
        let history_reader = HistoryReader::create_for_topic(context, unit_type, topic, 1).await?;
        Ok(InstantReader { history_reader: Mutex::new(history_reader) })
    }

    /// Reads the last message on the topic.
    ///
    /// This method will return the last message published on the topic.
    /// If no message has been published since the creation of this InstantReader,
    /// it will return an empty `Vec<u8>`.
    ///
    /// Return value:
    ///    SdvResult that contains SdvStatus of the method call and, if method
    ///    succeeded, a Vec<u8> containing the message or an empty Vec<u8> if no
    ///    message is available.
    pub async fn read_last_message(&self) -> SdvResult<Vec<u8>> {
        let history_reader = self.history_reader.lock().unwrap();
        if history_reader.available_to_read()? == 0 {
            return Ok(Vec::new());
        }

        let metadata = history_reader.get_publisher_unit_metadata();
        let message_size = metadata.get_message_size() as usize;

        let mut buffer = vec![0u8; message_size];
        let num_read = history_reader.read_last_messages(&mut buffer)?;
        if num_read == 1 {
            Ok(buffer)
        } else {
            // num_read cannot return more than one message since the history size is 1.
            // however, it could return no messages if nothing has ever been published
            Ok(Vec::new())
        }
    }
}
