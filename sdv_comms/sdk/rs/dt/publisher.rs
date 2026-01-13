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

use sdv_comms_bindgen::{ASDV_CallbackVTable_PublicationCreate, ASDV_PublicationHandle};
use sdv_comms_common::{callback::sdk::OnceCallbackWrapper, owned_opaque_ptr::OwnedOpaquePtr};
use sdv_status_rs::{derive, SdvResult, SdvStatus, SdvStatusCode};
use std::ffi::c_void;

use crate::id::UnitIdentifier;
use crate::sd::PublisherUnitMetadata;
use crate::sd::UnitRegistrationArgs;
use crate::ContextRef;

/// Data Tunnel Publisher
#[repr(transparent)]
pub struct Publisher(OwnedOpaquePtr<ASDV_PublicationHandle>);

static_assertions::assert_impl_all!(Publisher: Send);
static_assertions::assert_not_impl_any!(Publisher: Sync);

impl Publisher {
    /// Non blocking.
    /// Registers a new publication for the specified UnitRegistrationArgs and
    /// and create a DataTunnel publication.
    ///
    /// Parameters:
    ///   [IN] context: Reference to the local SDV Comm Stack Context object
    ///   [IN] unit_reg_args: Unit registration arguments
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains SdvStatus of the
    ///   method call and, if method succeeded, a new Publisher instance.
    pub async fn create(
        context: ContextRef,
        unit_reg_args: UnitRegistrationArgs,
    ) -> SdvResult<Publisher> {
        type PublisherCreateCb = OnceCallbackWrapper<OwnedOpaquePtr<ASDV_PublicationHandle>>;
        let (cb, fut) = PublisherCreateCb::new_async();
        let vtable = ASDV_CallbackVTable_PublicationCreate {
            func: Some(PublisherCreateCb::trampoline_func),
            clear: Some(PublisherCreateCb::trampoline_clear),
            cb_data: cb.into_void_ptr(),
        };

        // SAFETY: `context` and `unit_reg_args` are valid pointers to corresponding types.
        // `vtable` represents the correct Callback Vtable: ASDV_CallbackVTable_PublicationCreate.
        // The pointer returned by the callback will be cleared through the Drop trait
        // implementation of struct [Publisher].
        unsafe {
            sdv_comms_bindgen::ASDV_CreatePublication(
                context.as_raw(),
                unit_reg_args.into_raw(),
                vtable,
            );
        }

        let handle = fut.await.map_err(derive!(SdvStatusCode::Internal))??;
        Ok(Publisher(handle))
    }

    /// Non blocking.
    /// Claims publisher access to an existing DataTunnel publication associated
    /// with the specified publication name.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_identifier: Unit Identifier of the publication to claim
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains SdvStatus of the
    ///   method call and, if method succeeded, a new Publisher instance.
    pub async fn claim(
        _context: ContextRef,
        _unit_identifier: &UnitIdentifier,
    ) -> SdvResult<Publisher> {
        unimplemented!();
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
            sdv_comms_bindgen::ASDV_GetPublisherUnitMetadata(self.0.as_ptr(), metadata.as_raw_mut())
        };

        SdvStatusCode::from(status)
            .expect("sdv_comms_bindgen::ASDV_GetPublisherUnitMetadata() failed");

        metadata
    }

    /// Non blocking.
    /// Writes messages to the publication's buffer.
    ///
    /// Parameters:
    ///   [IN] messages: An array of bytes containing messages to be sent.
    ///                  The size of the array must be divisible by the size
    ///                  of a message.
    ///
    /// Return value:
    ///    SdvResult that contains SdvStatus of the method call and, if
    ///    method succeeded, the number of messages that have been written to
    ///    the publication.
    #[tracing::instrument(skip(self, messages))]
    pub fn write_messages(&self, messages: &[u8]) -> SdvResult<u32> {
        let mut write_count = 0;
        // SAFETY: self.0 is a valid pointer to [ASDV_SubscriptionHandle].
        let status = unsafe {
            sdv_comms_bindgen::ASDV_WriteMessages(
                self.0.as_mut_ptr(),
                messages.as_ptr() as *const c_void,
                messages.len() as u32,
                &mut write_count,
            )
        };

        match SdvStatusCode::from(status) {
            SdvStatusCode::Ok => Ok(write_count),
            sdv_code => Err(SdvStatus::new(sdv_code)),
        }
    }
}

/// Destructor automatically invokes ASDV_DestroyPublication()
impl Drop for Publisher {
    fn drop(&mut self) {
        // SAFETY: self.0 is a valid pointer to [ASDV_PublicationHandle].
        let status = unsafe { sdv_comms_bindgen::ASDV_DestroyPublication(self.0.as_mut_ptr()) };

        SdvStatusCode::from(status).expect("Publisher could not be destroyed");
    }
}
