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

use sdv_comms_id::ffi::{
    ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
    ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
    ASDV_UNIT_TYPE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
};

#[cfg(comms_stack_ffi)]
use sdv_comms_common::{ffi_str::str_to_bytes, try_status};
use sdv_comms_stack_ffi_macro::{comms_stack_ffi_fn, comms_stack_ffi_struct};
use sdv_status_rs::{SdvStatus, SdvStatusCode};
use std::ffi::{c_uchar, CStr};
use std::str::FromStr;

use sdv_status_rs::sdv_status_t;

#[cfg(comms_stack_ffi)]
use crate::discovery::UnitType;

/// The maximum size in bytes (including the NUL (\0) character) for the complete unit type.
///
/// Sum all Unit Type part sizes with NUL - 3 NULs + 2 delimiters ('.', '#') + 1 NUL.
pub const ASDV_UNIT_TYPE_MAX_SIZE_BYTES_INCLUDING_NUL: usize =
    ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
        + ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
        + ASDV_UNIT_TYPE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL;

/// Represents the Unit Type Name as declared by a Service Bundle.
#[comms_stack_ffi_struct(shared)]
#[repr(C)]
#[derive(Clone)]
pub struct ASDV_UnitType {
    pub(crate) sdv_package_name: [c_uchar; ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
    pub(crate) service_bundle_name:
        [c_uchar; ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
    pub(crate) unit_type_name: [c_uchar; ASDV_UNIT_TYPE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
}

impl Default for ASDV_UnitType {
    fn default() -> Self {
        Self {
            sdv_package_name: [0; ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
            service_bundle_name: [0; ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
            unit_type_name: [0; ASDV_UNIT_TYPE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
        }
    }
}

/// Convert a NUL-terminated ASCII C string into a `ASDV_UnitType`.
///
/// This function parses the unit type described by `str`, and stores the
/// result in `unit_type`.
///
/// # Errors
///
/// This function returns `ASDV_STATUS_OK` on success. Otherwise,
/// it can return the following errors:
///
/// * `ASDV_STATUS_INVALID_ARGUMENT` in case if `str` is not a valid UnitType string.
/// * `ASDV_STATUS_INTERNAL` if an unexpected error occurs during the function call.
///
/// # Safety
///
/// The caller must guarantee that:
///
/// * `str` is a valid, NUL-terminated ASCII C string.
/// * `unit_type` is a valid, mutable pointer to a `ASDV_UnitType` structure.
/// * Both pointers remain valid for the duration of this function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitType_fromString(
    #[allow_ptr] str: *const c_uchar,
    unit_type: &mut ASDV_UnitType,
) -> sdv_status_t {
    // SAFETY: str is a valid pointer to NUL terminated ASCII C-String that lives for at least
    // the duration of the function call.
    let c_str = unsafe { CStr::from_ptr(str.cast()) };

    try_status! {
        let rust_str = c_str.to_str().map_err(|_| SdvStatus::with_message(
            SdvStatusCode::InvalidArgument,
            format!("Provided UnitType {c_str:?} is not a valid UTF-8 string")))?;

        let ut = UnitType::from_str(rust_str)
            .map_err(|_| SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                format!("Invalid UnitType string provided: {rust_str}")))?;

        ut.clone_into(unit_type)?;

        Ok(())
    }
}

/// Validate a `ASDV_UnitType`.
///
/// This function checks if the `ASDV_UnitType` struct stored in `unit_type` is valid.
///
/// # Errors
///
/// This function returns `ASDV_STATUS_OK` if `unit_type` is valid. Otherwise,
/// it can return the following errors:
///
/// * `ASDV_STATUS_INVALID_ARGUMENT` in case if `unit_type` is not a valid unit_type structure.
/// * `ASDV_STATUS_INTERNAL` if an unexpected error occurs during the function call.
///
/// # Safety
///
/// The caller must guarantee that:
///
/// * `unit_type` is a valid, non-null pointer to a `ASDV_UnitType` structure.
/// * The pointed-to `ASDV_UnitType` remains valid for the duration of this function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitType_validate(unit_type: &ASDV_UnitType) -> sdv_status_t {
    try_status! {
        let _ = UnitType::try_from(unit_type)?;
        Ok(())
    }
}

/// Convert a `ASDV_UnitType` into a human-readable string.
///
/// This functions converts `ASDV_UnitType` stored in `unit_type` into a NUL-terminated ASCII
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
/// * `ASDV_STATUS_INVALID_ARGUMENT` in case if `unit_type` is not a valid unit_type
/// structure or a buffer is too small to hold the string representation.
/// * `ASDV_STATUS_INTERNAL` if an unexpected error occurs during the function call.
///
/// # Safety
///
/// The caller must ensure that:
///
/// * `unit_type` is a valid, non-null pointer to a `ASDV_UnitType` structure.
/// * `buf` is a valid, mutable pointer to a buffer with at least `buf_size` bytes of available
///    space.
/// * `buf_size` is a valid, non-null pointer to a size of a `buf`.
/// * `unit_type`, `buf` and `buf_size` remain valid for the duration of this function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitType_toString(
    unit_type: &ASDV_UnitType,
    #[allow_ptr] buf: *mut c_uchar,
    buf_size: &mut u32,
) -> sdv_status_t {
    try_status! {
        let unit_type = UnitType::try_from(unit_type)?;
        let ut_str = unit_type.to_string();

        // SAFETY: `buf` is valid pointer to a c_char buffer with a `buf_size` size.
        let out_bytes: &mut [u8] = unsafe {
            std::slice::from_raw_parts_mut(buf, *buf_size as usize)
        };
        str_to_bytes(&ut_str, out_bytes)?;

        // Update buffer size and take a NUL-terminator into account.
        *buf_size = ut_str.len() as u32 + 1;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use sdv_comms_id_validator::{
        PACKAGE_NAME_MAX_LENGTH_BYTES, SERVICE_NAME_MAX_LENGTH_BYTES,
        UNIT_TYPE_NAME_MAX_LENGTH_BYTES,
    };

    use crate::discovery::UNIT_TYPE_MAX_LENGTH_BYTES;

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
    fn test_unit_type_name_size() {
        // +1 for NUL terminator
        assert_eq!(
            ASDV_UNIT_TYPE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
            UNIT_TYPE_NAME_MAX_LENGTH_BYTES + 1
        );
    }

    #[test]
    fn test_unit_type_size_from_parts() {
        // unit_type = PACKAGE_NAME + SERVICE_BUNDLE_NAME + UNIT_TYPE_NAME
        //        - 3 NULs + 2 delimiters ('.', '#') + 1 NUL
        assert_eq!(
            ASDV_UNIT_TYPE_MAX_SIZE_BYTES_INCLUDING_NUL,
            ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
                + ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
                + ASDV_UNIT_TYPE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
        );
    }

    #[test]
    fn test_unit_type_size() {
        // +1 for NUL terminator
        assert_eq!(ASDV_UNIT_TYPE_MAX_SIZE_BYTES_INCLUDING_NUL, UNIT_TYPE_MAX_LENGTH_BYTES + 1);
    }
}
