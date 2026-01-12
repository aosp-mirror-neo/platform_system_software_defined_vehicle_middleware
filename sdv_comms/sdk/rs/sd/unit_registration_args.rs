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

use sdv_comms_bindgen::ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL;
use sdv_comms_bindgen::{ASDV_UnitMetadataType, ASDV_UnitRegistrationArgs};
use sdv_comms_common::ffi_str;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};

use bytemuck::TransparentWrapper;

use std::mem;

use crate::sd::{
    PublisherUnitMetadata, ServerUnitMetadata,
    UnitMetadata::{self, *},
    UnitType,
};

/// Defines args for Service Unit registration.
/// Owns a pointer of type sdv_comms_bindgen::ASDV_UnitRegistrationArgs.
pub struct UnitRegistrationArgs(*mut ASDV_UnitRegistrationArgs);

static_assertions::assert_impl_all!(UnitRegistrationArgs: Send, Sync);

/// Safety: A `UnitRegistrationArgs` is a wrapper around `ASDV_UnitRegistrationArgs` which should be threadsafe.
unsafe impl Send for UnitRegistrationArgs {}

/// Safety: A `UnitRegistrationArgs` is a wrapper around `ASDV_UnitRegistrationArgs` which should be threadsafe.
unsafe impl Sync for UnitRegistrationArgs {}

impl Default for UnitRegistrationArgs {
    fn default() -> Self {
        UnitRegistrationArgs::new()
    }
}

impl Drop for UnitRegistrationArgs {
    fn drop(&mut self) {
        // Safety: It is safe to call the FFI function takes ownership  and destroy
        // an ASDV_UnitRegistrationArgs pointer created by ASDV_UnitRegistrationArgs_create().
        unsafe {
            sdv_comms_bindgen::ASDV_UnitRegistrationArgs_destroy(self.0);
        }
    }
}

impl UnitRegistrationArgs {
    /// # Safety
    /// This function borrows a reference to the UnitRegistrationArgs as the underlying pointer.
    /// The returned pointer MUST NOT be mutated by the caller, and MUST NOT be deleted by the caller.
    #[inline]
    pub unsafe fn as_raw(&self) -> *const ASDV_UnitRegistrationArgs {
        self.0 as _
    }

    /// # Safety
    /// This function takes ownership of the underlying pointer.
    #[inline]
    pub unsafe fn into_raw(self) -> *mut ASDV_UnitRegistrationArgs {
        let ptr = self.0;
        mem::forget(self);

        ptr as _
    }

    /// Create a new [UnitRegistrationArgs].
    pub fn new() -> UnitRegistrationArgs {
        // SAFETY:
        // It is safe to call the FFI function that creates and transfers ownership of
        // an ASDV_UnitRegistrationArgs pointer.
        UnitRegistrationArgs(unsafe { sdv_comms_bindgen::ASDV_UnitRegistrationArgs_create() })
    }

    /// Sets the unit_name field of this [UnitRegistrationArgs].
    pub fn set_unit_name(&mut self, unit_name: &str) {
        let buf_size = ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as u8;
        let mut buf = vec![0; buf_size.into()];

        ffi_str::str_to_bytes(unit_name, &mut buf).expect("Invalid unit name");
        // SAFETY: `self.0` and `buf` are guaranteed to be valid pointers.
        // `buf` is NUL-terminated.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_UnitRegistrationArgs_setUnitName(self.0, buf.as_ptr())
        };

        SdvStatusCode::from(status).expect("UnitRegistrationArgs::set_unit_name() failed");
    }

    /// Sets the unit_type field of this [UnitRegistrationArgs].
    pub fn set_unit_type(&mut self, unit_type: &UnitType) {
        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_UnitRegistrationArgs_setUnitType(
                self.0,
                UnitType::peel_ref(unit_type),
            )
        };

        SdvStatusCode::from(status).expect("UnitRegistrationArgs::set_unit_type() failed");
    }

    /// Sets the unit_metadata field of this [UnitRegistrationArgs].
    pub fn set_unit_metadata(&mut self, unit_metadata: &UnitMetadata) {
        match unit_metadata {
            Server(server_unit_metadata) => {
                self.set_server_unit_metadata(server_unit_metadata.clone());
            }
            Publisher(publisher_unit_metadata) => {
                self.set_publisher_unit_metadata(publisher_unit_metadata.clone());
            }
        }
    }

    fn set_server_unit_metadata(&mut self, server_unit_metadata: ServerUnitMetadata) {
        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_UnitRegistrationArgs_setServerUnitMetadata(
                self.0,
                server_unit_metadata.as_raw(),
            )
        };

        SdvStatusCode::from(status)
            .expect("UnitRegistrationArgs::set_server_unit_metadata() failed");
    }

    fn set_publisher_unit_metadata(&mut self, publisher_unit_metadata: PublisherUnitMetadata) {
        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_UnitRegistrationArgs_setPublisherUnitMetadata(
                self.0,
                publisher_unit_metadata.as_raw(),
            )
        };

        SdvStatusCode::from(status).expect(
            "UnitRegistrationArgs::set_publisher_unit_metadata() failed setting publisher metadata",
        );
    }

    /// Returns application metadata associated with Unit Registration
    pub fn get_application_metadata(&self) -> SdvResult<Vec<u8>> {
        let mut size: usize = 0;
        // SAFETY: `self.0` is guaranteed to be a valid pointer to [ASDV_UnitRegistrationArgs].
        let status = unsafe {
            sdv_comms_bindgen::ASDV_UnitRegistrationArgs_getApplicationMetadataSize(
                self.0, &mut size,
            )
        };

        let status_code = SdvStatus::from_raw_code(status);
        if !status_code.is_ok() {
            return Err(status_code);
        }

        let mut application_metadata: Vec<u8> = vec![0; size];
        // SAFETY: `self.0` is guaranteed to be a valid pointer to [ASDV_UnitRegistrationArgs]
        // and application_metadata is a pointer to an array of the correct size.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_UnitRegistrationArgs_getApplicationMetadata(
                self.0,
                application_metadata.as_mut_ptr(),
                size,
            )
        };

        let status_code = SdvStatus::from_raw_code(status);
        if status_code.is_ok() {
            Ok(application_metadata)
        } else {
            Err(status_code)
        }
    }

    /// Sets the application metadata field required for Unit Registration with metadata
    pub fn set_application_metadata(&mut self, application_metadata: &[u8]) {
        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_UnitRegistrationArgs_setApplicationMetadata(
                self.0,
                application_metadata.as_ptr(),
                application_metadata.len(),
            )
        };

        SdvStatusCode::from(status)
            .expect("UnitRegistrationArgs::set_application_metadata() failed");
    }

    /// Returns the unit_name field of this [UnitRegistrationArgs].
    pub fn get_unit_name(&self) -> String {
        let mut buf_size = ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as u8;
        let mut buf = vec![0; buf_size.into()];

        // SAFETY: `self.as_raw()` is guaranteed to be a valid pointer.
        // Guarantee from `Vec`: `buf.as_mut_ptr()` is guaranteed to be non-null and a valid
        // pointer for the length of `buf_size`.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_UnitRegistrationArgs_getUnitName(
                self.as_raw(),
                buf.as_mut_ptr(),
                &mut buf_size,
            )
        };

        SdvStatusCode::from(status).expect("UnitRegistrationArgs::get_unit_name() failed");

        ffi_str::bytes_to_str(buf.as_slice()).expect("Invalid unit name").to_string()
    }

    /// Returns the unit_type field of this [UnitRegistrationArgs].
    pub fn get_unit_type(&self) -> UnitType {
        let mut unit_type = UnitType::default();

        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_UnitRegistrationArgs_getUnitType(
                self.0,
                UnitType::peel_mut(&mut unit_type),
            )
        };

        SdvStatusCode::from(status).expect("UnitRegistrationArgs::get_unit_type() failed");

        unit_type
    }

    /// Returns the unit_metadata field of this [UnitRegistrationArgs].
    pub fn get_unit_metadata(&self) -> UnitMetadata {
        let mut metadata_type = ASDV_UnitMetadataType::default();
        // SAFETY: `self.0` is guaranteed to be a valid pointer
        let status = unsafe {
            sdv_comms_bindgen::ASDV_UnitRegistrationArgs_getUnitMetadataType(
                self.as_raw(),
                &mut metadata_type,
            )
        };
        SdvStatusCode::from(status).expect("UnitRegistrationArgs::get_unit_metadata_type() failed");

        match metadata_type {
            sdv_comms_bindgen::ASDV_UnitMetadataType_ASDV_UNIT_METADATA_TYPE_SERVER => {
                Server(self.get_server_unit_metadata())
            }
            sdv_comms_bindgen::ASDV_UnitMetadataType_ASDV_UNIT_METADATA_TYPE_PUBLISHER => {
                Publisher(self.get_publisher_unit_metadata())
            }
            _ => panic!("UnitRegistrationArgs::get_unit_metadata() failed: unknown metadata type"),
        }
    }

    fn get_publisher_unit_metadata(&self) -> PublisherUnitMetadata {
        let mut metadata = PublisherUnitMetadata::new();

        // SAFETY: safe because safety requirements of the function are satisfied.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_UnitRegistrationArgs_getPublisherUnitMetadata(
                self.as_raw(),
                metadata.as_raw_mut(),
            )
        };

        SdvStatusCode::from(status)
            .expect("UnitRegistrationArgs::get_publisher_unit_metadata() failed");

        metadata
    }

    fn get_server_unit_metadata(&self) -> ServerUnitMetadata {
        let mut metadata = ServerUnitMetadata::new();

        // SAFETY: safe because safety requirements of the function are satisfied.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_UnitRegistrationArgs_getServerUnitMetadata(
                self.as_raw(),
                metadata.as_raw_mut(),
            )
        };

        SdvStatusCode::from(status)
            .expect("UnitRegistrationArgs::get_server_unit_metadata() failed");

        metadata
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::{
        assert_that,
        prelude::{eq, matches_pattern},
    };

    #[test]
    fn test_create_destroy() {
        let _args = UnitRegistrationArgs::default();

        // Implicit drop is happening here:
        //
        // drop(args);
    }

    #[test]
    fn test_get_set_unit_name() {
        let mut args = UnitRegistrationArgs::default();
        args.set_unit_name("test-unit-name");

        assert_that!(args.get_unit_name(), eq("test-unit-name"));
    }

    #[test]
    #[should_panic]
    fn test_get_unit_name_without_set_is_error() {
        let args = UnitRegistrationArgs::default();
        let _ = args.get_unit_name();
    }

    #[test]
    fn test_get_set_unit_type() {
        let mut args = UnitRegistrationArgs::default();
        let unit_type = create_unit_type();

        args.set_unit_type(&unit_type);

        assert_that!(args.get_unit_type(), eq(&unit_type));
    }

    fn create_unit_type() -> UnitType {
        UnitType::builder()
            .set_sdv_package_name("com.sdv.unit_test")
            .set_service_bundle_name("TestBundleName")
            .set_unit_type_name("TestUnitType")
            .build()
            .unwrap()
    }

    #[test]
    #[should_panic]
    fn test_get_unit_metadata_without_set_is_error() {
        let args = UnitRegistrationArgs::default();
        let _ = args.get_unit_metadata();
    }

    #[test]
    fn test_get_set_server_unit_metadata() {
        let mut args = UnitRegistrationArgs::default();
        let server_metadata = create_server_metadata();

        args.set_server_unit_metadata(server_metadata);

        assert_that!(
            args.get_unit_metadata(),
            matches_pattern!(UnitMetadata::Server(eq(&create_server_metadata())))
        );
        assert_that!(args.get_server_unit_metadata(), eq(&create_server_metadata()));
    }

    #[test]
    fn test_get_set_publisher_unit_metadata() {
        let mut args = UnitRegistrationArgs::default();
        let publisher_metadata = create_publisher_metadata();

        args.set_publisher_unit_metadata(publisher_metadata);

        assert_that!(
            args.get_unit_metadata(),
            matches_pattern!(UnitMetadata::Publisher(eq(&create_publisher_metadata())))
        );
        assert_that!(args.get_publisher_unit_metadata(), eq(&create_publisher_metadata()));
    }

    fn create_server_metadata() -> ServerUnitMetadata {
        let mut metadata = ServerUnitMetadata::new();
        metadata.set_version(1);
        metadata
    }

    fn create_publisher_metadata() -> PublisherUnitMetadata {
        let mut metadata = PublisherUnitMetadata::new();
        metadata.set_version(2);
        metadata.set_message_size(10);
        metadata
    }
}
