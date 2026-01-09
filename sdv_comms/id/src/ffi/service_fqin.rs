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

use sdv_comms_common::ffi_str::{bytes_to_str, str_to_bytes};
use sdv_comms_common::try_status;
use sdv_comms_stack_ffi_macro::{comms_stack_ffi_fn, comms_stack_ffi_struct};
use sdv_status_rs::{SdvStatus, SdvStatusCode};
use std::ffi::c_uchar;
use std::ffi::CStr;
use std::str::FromStr;

#[allow(non_camel_case_types)]
type sdv_status_t = i32;

#[cfg(comms_stack_ffi)]
use crate::ServiceFqin;

/// The maximum size in bytes (including the NUL (\0) character) for a SDV VM Name string.
pub const ASDV_VM_NAME_MAX_SIZE_BYTES_INCLUDING_NUL: usize = 32;

/// The maximum size in bytes (including the NUL (\0) character) for a SDV Package Name string.
pub const ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL: usize = 128;

/// The maximum size in bytes (including the NUL (\0) character) for a Service Bundle Name string.
pub const ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL: usize = 48;

/// The maximum size in bytes (including the NUL (\0) character) for a Service Instance Name string.
pub const ASDV_SERVICE_INSTANCE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL: usize = 48;

/// The maximum size in bytes (including the NUL (\0) character) for a Service FQIN string.
///
/// Sum all FQIN part sizes with NUL - 4 NULs + 3 delimiters (':', '.', '/') + 1 NUL.
pub const ASDV_SERVICE_FQIN_MAX_SIZE_BYTES_INCLUDING_NUL: usize = 256;

/// The maximum size in bytes (including the NUL (\0) character) for a Unit Name string.
pub const ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL: usize = 128;

/// The maximum size in bytes (including the NUL (\0) character) for a Unit Type Name string.
pub const ASDV_UNIT_TYPE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL: usize = 96;

/// Describes the Service Fully Qualified Name.
///
/// This name comprises the following components:
/// 1) SDV VM Name: The SDV VM Instance on which this Service is registered.
/// 2) SDV Package Name: The Package Name within which the Service Bundle was shipped.
/// 3) Service Bundle Name: The Service Bundle Name as declared in the Package Manifest.
/// 4) Service Instance Name: The Service Instance Name as assigned by the Orchestrator
///    when the Service Bundle was loaded.
///
/// Each name field in this struct MUST be a valid ASCII encoded sequence of bytes ending with the
/// NUL (\0) character. Individual name fields MUST match the requirements for [Service Naming
/// Conventions](https://docs.partner.android.com/automotive/sdv/core-areas/naming-conventions)
#[comms_stack_ffi_struct(shared)]
#[repr(C)]
pub struct ASDV_ServiceFqin {
    pub(crate) sdv_vm_name: [c_uchar; ASDV_VM_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
    pub(crate) sdv_package_name: [c_uchar; ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
    pub(crate) service_bundle_name:
        [c_uchar; ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
    pub(crate) service_instance_name:
        [c_uchar; ASDV_SERVICE_INSTANCE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
}

// Implement Default so that types that contain this as a field can easily implement Default;
// Can't use derive(Default) because Default isn't implemented for arrays larger than 32 entries.
impl Default for ASDV_ServiceFqin {
    fn default() -> ASDV_ServiceFqin {
        ASDV_ServiceFqin {
            sdv_vm_name: [0; ASDV_VM_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
            sdv_package_name: [0; ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
            service_bundle_name: [0; ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
            service_instance_name: [0; ASDV_SERVICE_INSTANCE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
        }
    }
}

/// Convert a NUL-terminated ASCII C string into a `ASDV_ServiceFqin`.
///
/// This function parses the service described by `str`, and stores the
/// result in `fqin`.
///
/// # Errors
///
/// This function returns `ASDV_STATUS_OK` on success. Otherwise,
/// it can return the following errors:
///
/// * `ASDV_STATUS_INVALID_ARGUMENT` in case if `str` is not a valid FQIN string.
/// * `ASDV_STATUS_INTERNAL` if an unexpected error occurs during the function call.
///
/// # Safety
///
/// The caller must guarantee that:
///
/// * `str` is a valid, NUL-terminated ASCII C string.
/// * `fqin` is a valid, mutable pointer to a `ASDV_ServiceFqin` structure.
/// * Both pointers remain valid for the duration of this function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceFqin_fromString(
    #[allow_ptr] str: *const c_uchar,
    fqin: &mut ASDV_ServiceFqin,
) -> sdv_status_t {
    // SAFETY: str is a valid pointer to NUL terminated ASCII C-String that lives for at least
    // the duration of the function call.
    let c_str = unsafe { CStr::from_ptr(str.cast()) };

    try_status! {
        let rust_str = c_str.to_str().map_err(|_| SdvStatus::with_message(
            SdvStatusCode::InvalidArgument,
            format!("Provided FQIN {c_str:?} is not a valid UTF-8 string")))?;

        let service_fqin = ServiceFqin::from_str(rust_str)
            .map_err(|_| SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                format!("Invalid FQIN {rust_str:?} string provided")))?;

        service_fqin.clone_into(fqin)?;

        Ok(())
    }
}

/// Validate a `ASDV_ServiceFqin`.
///
/// This function checks if the `ASDV_ServiceFqin` struct stored in `fqin` is valid.
///
/// # Errors
///
/// This function returns `ASDV_STATUS_OK` if `fqin` is valid. Otherwise,
/// it can return the following errors:
///
/// * `ASDV_STATUS_INVALID_ARGUMENT` in case if `fqin` is not a valid FQIN structure.
/// * `ASDV_STATUS_INTERNAL` if an unexpected error occurs during the function call.
///
/// # Safety
///
/// The caller must guarantee that:
///
/// * `fqin` is a valid, non-null pointer to a `ASDV_ServiceFqin` structure.
/// * The pointed-to `ASDV_ServiceFqin` remains valid for the duration of this function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceFqin_validate(fqin: &ASDV_ServiceFqin) -> sdv_status_t {
    try_status! {
        let _ = ServiceFqin::try_from(fqin)?;
        Ok(())
    }
}

/// Convert a `ASDV_ServiceFqin` into a human-readable string.
///
/// This functions converts `ASDV_ServiceFqin` stored in `fqin` into a NUL-terminated ASCII
/// encoded C string and writes the result to the buffer `buf` of size `buf_size`.
///
/// On success, this function stores the number of bytes written to the buffer `buf`
/// (including NUL-terminator) in `buf_size`.
///
/// # Errors
///
/// This function returns `ASDV_STATUS_OK` on success. Otherwise,
/// it can return the following errors:
///
/// * `ASDV_STATUS_INVALID_ARGUMENT` in case if `fqin` is not a valid FQIN structure or a buffer
///   is too small to hold the string representation.
/// * `ASDV_STATUS_INTERNAL` if an unexpected error occurs during the function call.
///
/// # Safety
///
/// The caller must ensure that:
///
/// * `fqin` is a valid, non-null pointer to a `ASDV_ServiceFqin` structure.
/// * `buf` is a valid, mutable pointer to a buffer with at least `buf_size` bytes of available
///    space.
/// * `buf_size` is a valid, non-null pointer to a size of a `buf`.
/// * `fqin`, `buf` and `buf_size` remain valid for the duration of this function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceFqin_toString(
    fqin: &ASDV_ServiceFqin,
    #[allow_ptr] buf: *mut c_uchar,
    buf_size: &mut u32,
) -> sdv_status_t {
    try_status! {
        let service_fqin = ServiceFqin::try_from(fqin)?;
        let fqin_string = service_fqin.to_string();

        // SAFETY: `buf` is valid pointer to a c_char buffer with a `buf_size` size.
        let out_bytes: &mut [u8] = unsafe {
            std::slice::from_raw_parts_mut(buf, *buf_size as usize)
        };
        str_to_bytes(&fqin_string, out_bytes)?;

        // Update buffer size and take a NUL-termanitor into account.
        *buf_size = fqin_string.len() as u32 + 1;

        Ok(())
    }
}

/// Check whether two `ASDV_ServiceFqin`s are equal.
///
/// # Errors
///
/// This function always returns `ASDV_STATUS_OK`. Checks for the format
/// validity of `ASDV_ServiceFqin`s are deprecated to avoid unnecessary overhead.
///
/// # Safety
///
/// The caller must guarantee that:
///
/// * `fqin_lhs` is a valid, non-null pointer to a `ASDV_ServiceFqin` structure, and the pointed-to
/// instance remains valid for the duration of this function call.
/// * `fqin_rhs` is a valid, non-null pointer to a `ASDV_ServiceFqin` structure, and the pointed-to
/// instance remains valid for the duration of this function call.
/// * `result` is a valid, non-null pointer to a boolean.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceFqin_equals(
    fqin_lhs: &ASDV_ServiceFqin,
    fqin_rhs: &ASDV_ServiceFqin,
    result: &mut bool,
) -> sdv_status_t {
    try_status! {
        *result = fqin_lhs.sdv_vm_name == fqin_rhs.sdv_vm_name
            && fqin_lhs.sdv_package_name == fqin_rhs.sdv_package_name
            && fqin_lhs.service_bundle_name == fqin_rhs.service_bundle_name
            && fqin_lhs.service_instance_name == fqin_rhs.service_instance_name;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::SERVICE_FQIN_MAX_LENGTH_BYTES;
    use sdv_comms_id_validator::{
        INSTANCE_NAME_MAX_LENGTH_BYTES, PACKAGE_NAME_MAX_LENGTH_BYTES,
        SERVICE_NAME_MAX_LENGTH_BYTES, VM_NAME_MAX_LENGTH_BYTES,
    };

    #[test]
    fn test_vm_name_size() {
        // +1 for NUL terminator
        assert_eq!(ASDV_VM_NAME_MAX_SIZE_BYTES_INCLUDING_NUL, VM_NAME_MAX_LENGTH_BYTES + 1);
    }

    #[test]
    fn test_package_name_size() {
        // +1 for NUL terminator
        assert_eq!(
            ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
            PACKAGE_NAME_MAX_LENGTH_BYTES + 1
        );
    }

    #[test]
    fn test_service_bundle_name_size() {
        // +1 for NUL terminator
        assert_eq!(
            ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
            SERVICE_NAME_MAX_LENGTH_BYTES + 1
        );
    }

    #[test]
    fn test_service_instance_name_size() {
        // +1 for NUL terminator
        assert_eq!(
            ASDV_SERVICE_INSTANCE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
            INSTANCE_NAME_MAX_LENGTH_BYTES + 1
        );
    }

    #[test]
    fn test_fqin_size() {
        // FQIN = VM_NAME + PACKAGE_NAME + SERVICE_BUNDLE_NAME + SERVICE_INSTANCE_NAME
        //        - 4 NULs + 3 delimiters (':', '.', '/') + 1 NUL
        //
        // Which makes
        //
        // FQIN = VM_NAME + PACKAGE_NAME + SERVICE_BUNDLE_NAME + SERVICE_INSTANCE_NAME
        assert_eq!(
            ASDV_SERVICE_FQIN_MAX_SIZE_BYTES_INCLUDING_NUL,
            ASDV_VM_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
                + ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
                + ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
                + ASDV_SERVICE_INSTANCE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
        );

        // +1 for NUL terminator
        assert_eq!(
            ASDV_SERVICE_FQIN_MAX_SIZE_BYTES_INCLUDING_NUL,
            SERVICE_FQIN_MAX_LENGTH_BYTES + 1
        );
    }
}
