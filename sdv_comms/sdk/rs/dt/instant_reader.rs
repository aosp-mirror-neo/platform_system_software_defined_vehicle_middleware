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

use crate::id::UnitIdentifier;
use crate::ContextRef;

/// Data Tunnel Instant Reader
pub struct InstantReader;

static_assertions::assert_impl_all!(InstantReader: Send);
// TODO: uncomment when the type is implemented.
//static_assertions::assert_not_impl_any!(InstantReader: Sync);

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
}
