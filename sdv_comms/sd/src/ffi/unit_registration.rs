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

#[cfg(not(comms_stack_stub))]
use bytemuck::TransparentWrapper;
use sdv_comms_common::{ffi_str, try_status};
#[cfg(not(comms_stack_stub))]
use sdv_comms_id::names::ServiceUnitName;
use sdv_comms_stack_ffi_macro::{comms_stack_ffi_fn, comms_stack_ffi_struct};
use sdv_status_rs::{derive, sdv_status_t, SdvStatus, SdvStatusCode};
use std::ffi::{c_char, c_uchar};
use std::slice;
use std::str::FromStr;

use crate::ffi::unit_metadata::{ASDV_PublisherUnitMetadata, ASDV_ServerUnitMetadata};
use crate::ffi::unit_type::ASDV_UnitType;

#[cfg(not(comms_stack_stub))]
use crate::registration::UnitRegistrationArgs;

#[cfg(not(comms_stack_stub))]
use crate::discovery::UnitType;

extern crate libc;

/// Arguments passed to Service Disovery for registration of an individual ServiceUnit.
#[comms_stack_ffi_struct(opaque)]
#[derive(Default)]
pub struct ASDV_UnitRegistrationArgs(UnitRegistrationArgs);

/// Creates a ASDV_UnitRegistrationArgs.
///
/// This function always returns a valid heap-allocated ASDV_UnitRegistrationArgs. It aborts
/// if it fails to allocate the object on the heap. The caller is responsible for freeing the
/// returned object.
///
/// SAFETY: The caller takes ownership of the returned object and is responsible for freeing the
/// memory pointed to by the returned pointer using ASDV_UnitRegistrationArgs_destroy().
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitRegistrationArgs_create() -> Box<ASDV_UnitRegistrationArgs> {
    Box::<ASDV_UnitRegistrationArgs>::default()
}

/// Deletes a ASDV_UnitRegistrationArgs returned by ASDV_UnitRegistrationArgs_create() function.
///
/// Parameters:
///    [IN] ptr: The ASDV_UnitRegistrationArgs value to delete.
///
/// SAFETY: The caller guarantees that `ptr` is a valid pointer created with
/// ASDV_UnitRegistrationArgs_create().
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitRegistrationArgs_destroy(
    ptr: Option<Box<ASDV_UnitRegistrationArgs>>,
) {
    if let Some(sud) = ptr {
        drop(sud)
    }
}

/// Sets value for UnitName of the UnitRegistrationArgs.
///
/// The input value must be a valid ASCII C string with a terminating NUL (\0) character. The
/// string must be at most [ASDV_UNIT_NAME_SIZE_BYTES] bytes (including the NUL (\0) character).
///
/// If the function is called multiple times, the UnitName value associated with the provided
/// UnitRegistrationArgs will be overwritten.
///
/// Parameters:
///    [IN] args: A valid pointer to the UnitRegistrationArgs object for which to set the UnitName
///    [IN] str: Pointer to the NUL-terminated ASCII C string to use as the UnitName
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT in case if `str` is not a valid ASCII C string with a
///        terminating NUL character. The string must be at most [ASDV_UNIT_NAME_SIZE_BYTES] bytes
///        (including the NUL (\0) character).
///
/// # Safety:
///  - `args` must be a valid pointer for the duration of the function call.
///  - `str` must abide by the same safety requirements as parameter for CStr::from_ptr.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitRegistrationArgs_setUnitName(
    args: &mut ASDV_UnitRegistrationArgs,
    #[allow_ptr] str: *const c_uchar,
) -> sdv_status_t {
    try_status! {
        // SAFETY: safe because of the safety requirements for this function.
        let unit_name_str = unsafe { ffi_str::cstr_pointer_to_str(str as *const c_char)? };
        let unit_name = ServiceUnitName::from_str(&unit_name_str)
            .map_err(derive!(SdvStatusCode::InvalidArgument))?;
        args.0.set_unit_name(unit_name);
        Ok(())
    }
}

/// Returns the set value for UnitName of the UnitRegistrationArgs.
///
/// Parameters:
///    [IN] args: A valid pointer to the UnitRegistrationArgs object for which to get the UnitName
///    [OUT] buf: Pointer to char buffer into which the string will be written as an ASCII encoded
///               NUL-terminated C string
///    [IN/OUT] buf_size: Provides the size (in bytes) of the allocated buffer [buf], and returns
///                       the number of bytes written to the provided buffer (including the NUL
///                       character)
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT if the buf_size is not large enough to hold the UnitName value
///        of the UnitRegistrationArgs.
///    ASDV_STATUS_NOT_FOUND if the UnitName value has not been set for this UnitRegistrationArgs.
///
/// # Safety:
///  - `args`, `buf`, and `buf_size` must be valid pointers that live for at least the duration of
/// the function call.
///  - `buf` must be non-null, even if the `buf_size` is zero.
///  - `buf` must abide by the same safety requirements as parameters for slice::from_raw_parts_mut.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitRegistrationArgs_getUnitName(
    args: &ASDV_UnitRegistrationArgs,
    #[allow_ptr] buf: *mut c_uchar,
    buf_size: &mut u8,
) -> sdv_status_t {
    try_status! {
        let unit_name = args.0.get_unit_name()
            .ok_or_else(||
                SdvStatus::with_message(SdvStatusCode::FailedPrecondition, "UnitMetadata not set")
            )?;
        // SAFETY: safe because of the safety requirements for this function.
        let output_slice = unsafe { std::slice::from_raw_parts_mut(buf, *buf_size as usize) };

        let source_bytes = unit_name.name().as_bytes();
        let source_length = source_bytes.len();

        if source_length > output_slice.len() {
            return Err(SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                format!("UnitName {unit_name} longer than provided buffer")))
        }

        output_slice[..source_length].copy_from_slice(source_bytes);
        Ok(())
    }
}

/// Sets value for UnitType of the UnitRegistrationArgs.
///
/// The input value must be a valid pointer to a ASDV_UnitType struct.
///
/// If the function is called multiple times, the UnitType value associated with the provided
/// UnitRegistrationArgs will be overwritten.
///
/// Parameters:
///    [IN] args: A valid pointer to the UnitRegistrationArgs object for which to set the UnitType
///    [IN] unit_type: A valid pointer to an ASDV_UnitType struct
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT in case if `unit_type` is not a valid pointer.
///
/// SAFETY: The caller guarantees that `args`, `unit_type` are all valid pointers that live for at
/// least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitRegistrationArgs_setUnitType(
    args: &mut ASDV_UnitRegistrationArgs,
    unit_type: &ASDV_UnitType,
) -> sdv_status_t {
    try_status! {
        args.0.set_unit_type(UnitType::try_from(unit_type)?);
        Ok(())
    }
}

/// Returns the set value for UnitType of the UnitRegistrationArgs.
///
/// Parameters:
///    [IN] args: A valid pointer to the UnitRegistrationArgs object for which to get the UnitType
///    [OUT] unit_type: Provides the value of ASDV_UnitType set for this UnitRegistrationArgs
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT if the unit_type is not a valid pointer to struct ASDV_UnitType.
///    ASDV_STATUS_NOT_FOUND if the UnitType value has not been set for this UnitRegistrationArgs.
///
/// SAFETY: The caller guarantees that `args`, and `unit_type` are all valid pointers that live for
/// at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitRegistrationArgs_getUnitType(
    args: &ASDV_UnitRegistrationArgs,
    unit_type: &mut ASDV_UnitType,
) -> sdv_status_t {
    try_status! {
        args.0.get_unit_type().clone_into(unit_type)
    }
}

/// Sets value for ServerUnitMetadata of the UnitRegistrationArgs.
///
/// The input value must be a valid pointer to a ASDV_ServerUnitMetadata struct.
///
/// This function overwrites any PublisherUnitMetadata value set for this UnitRegistrationArgs.
/// Only one of ServerUnitMetadata or PublisherUnitMetadata may be set on any UnitRegistrationArgs.
///
/// If the function is called multiple times, the ServerUnitMetadata value associated with the provided
/// UnitRegistrationArgs will be overwritten.
///
/// Parameters:
///    [IN] args: A valid pointer to the UnitRegistrationArgs object for which to set the ServerUnitMetadata
///    [IN] server: A valid pointer to an ASDV_ServerUnitMetadata struct
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT in case if `server` is not a valid pointer.
///
/// SAFETY: The caller guarantees that `args`, `server` are all valid pointers that live for at
/// least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitRegistrationArgs_setServerUnitMetadata(
    args: &mut ASDV_UnitRegistrationArgs,
    server_metadata: &ASDV_ServerUnitMetadata,
) -> sdv_status_t {
    let server_metadata_internal = ASDV_ServerUnitMetadata::peel(server_metadata.clone());
    let unit_metadata = args.0.get_mut_unit_metadata();
    unit_metadata.set_server_metadata(server_metadata_internal);
    SdvStatusCode::Ok.into_raw()
}

/// Sets value for PublisherUnitMetadata of the UnitRegistrationArgs.
///
/// The input value must be a valid pointer to a ASDV_PublisherUnitMetadata struct.
///
/// This function overwrites any ServerUnitMetadata value set for this UnitRegistrationArgs.
/// Only one of ServerUnitMetadata or PublisherUnitMetadata may be set on any UnitRegistrationArgs.
///
/// If the function is called multiple times, the PublisherUnitMetadata value associated with the provided
/// UnitRegistrationArgs will be overwritten.
///
/// Parameters:
///    [IN] args: A valid pointer to the UnitRegistrationArgs object for which to set the PublisherUnitMetadata
///    [IN] publisher: A valid pointer to an ASDV_PublisherUnitMetadata struct
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT in case if `publisher` is not a valid pointer.
///
/// SAFETY: The caller guarantees that `args`, `publisher` are all valid pointers that live for at
/// least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitRegistrationArgs_setPublisherUnitMetadata(
    args: &mut ASDV_UnitRegistrationArgs,
    publisher_metadata: &ASDV_PublisherUnitMetadata,
) -> sdv_status_t {
    let publisher_metadata_internal = ASDV_PublisherUnitMetadata::peel(publisher_metadata.clone());
    let unit_metadata = args.0.get_mut_unit_metadata();
    unit_metadata.set_publisher_metadata(publisher_metadata_internal);
    SdvStatusCode::Ok.into_raw()
}

/// Returns value for ApplicationMetadata specific to a ServiceUnit.
///
/// The output value must be a valid pointer to a byte array
///
///
/// Parameters:
///    [IN] args: A valid pointer to the UnitRegistrationArgs object for which to get the
///               metadata
///    [OUT] application_metadata: A valid pointer to a byte array of size buffer_size
///    [IN] buffer_size: size of the array pointed to be application_metadata
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_FAILED_PRECONDITION in case if `args` contains no UnitMetadata.
///
/// SAFETY: The caller guarantees that `args` and `application_metadata` are valid pointers that
/// live for at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitRegistrationArgs_getApplicationMetadata(
    args: &mut ASDV_UnitRegistrationArgs,
    #[allow_ptr] application_metadata: *mut u8,
    #[allow_value] buffer_size: libc::size_t,
) -> sdv_status_t {
    let Some(unit_metadata) = args.0.get_unit_metadata() else {
        return SdvStatusCode::Ok.into_raw();
    };

    // SAFETY: The caller ensures that application_metadata points to a valid byte array of size
    // `buffer_size`. The expected size is determined using
    // ASDV_UnitRegistrationArgs_getApplicationMetadataSize API call.
    let metadata = unsafe { slice::from_raw_parts_mut(application_metadata, buffer_size) };
    metadata.copy_from_slice(unit_metadata.get_application_metadata());

    SdvStatusCode::Ok.into_raw()
}

/// Returns the size of ApplicationMetadata specific to a ServiceUnit.
///
/// Parameters:
///    [IN] args: A valid pointer to the UnitRegistrationArgs object for which to get the
///               metadata size
///    [OUT] buffer_size: Pointer to an usize that will store the size of Application Metadata
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_FAILED_PRECONDITION in case if `args` contains no UnitMetadata.
///
/// SAFETY: The caller guarantees that `args` and `buffer_size` are valid pointers that
/// live for at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitRegistrationArgs_getApplicationMetadataSize(
    args: &mut ASDV_UnitRegistrationArgs,
    buffer_size: &mut libc::size_t,
) -> sdv_status_t {
    *buffer_size = if let Some(unit_metadata) = args.0.get_unit_metadata() {
        unit_metadata.get_application_metadata_size()
    } else {
        0
    };

    SdvStatusCode::Ok.into_raw()
}

/// Sets value for ApplicationMetadata specific to a ServiceUnit.
///
/// The input value must be a valid pointer to a byte array
///
/// This function overwrites any ApplicationMetadata value set for this UnitRegistrationArgs.
///
///
/// Parameters:
///    [IN] args: A valid pointer to the UnitRegistrationArgs object for which to set the
///               ServerUnitMetadata
///    [IN] application_metadata: A valid pointer to a byte array of size buffer_size consisting of
///         ApplicationMetadata
///    [IN] buffer_size: size of the array pointed to be application_metadata
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT in case if `server` is not a valid pointer.
///
/// SAFETY: The caller guarantees that `args`, `application_metadata` are all valid pointers that
/// live for at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitRegistrationArgs_setApplicationMetadata(
    args: &mut ASDV_UnitRegistrationArgs,
    #[allow_ptr] application_metadata: *const u8,
    #[allow_value] buffer_size: libc::size_t,
) -> sdv_status_t {
    // SAFETY: The caller ensures that application_metadata points to a valid byte array of size
    // `buffer_size`
    let metadata = unsafe { slice::from_raw_parts(application_metadata, buffer_size) };
    let unit_metadata = args.0.get_mut_unit_metadata();
    unit_metadata.set_application_metadata(metadata);
    SdvStatusCode::Ok.into_raw()
}

/// Discriminant for UnitMetadata types.
#[repr(u8)]
pub enum ASDV_UnitMetadataType {
    /// Metadata type for ASDV_ServerUnitMetadata.
    ASDV_UNIT_METADATA_TYPE_SERVER = 1,
    /// Metadata type for ASDV_PublisherUnitMetadata.
    ASDV_UNIT_METADATA_TYPE_PUBLISHER = 2,
}

/// Returns the type of metadata set for the UnitRegistrationArgs.
///
/// Parameters:
///    [IN] args: A valid pointer to the UnitRegistrationArgs object for which to get the
///               ServerUnitMetadata
///    [OUT] metadata_type: The [UnitTypeMetadata] for the provided UnitRegistrationArgs.
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT in case if `publisher` is not a valid pointer.
///
/// SAFETY: The caller guarantees that `args` are all valid pointers that live for at least the
/// duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitRegistrationArgs_getUnitMetadataType(
    args: &ASDV_UnitRegistrationArgs,
    metadata_type: &mut ASDV_UnitMetadataType,
) -> sdv_status_t {
    try_status! {
        let unit_metadata = args.0.get_unit_metadata()
            .ok_or_else(||
                SdvStatus::with_message(SdvStatusCode::FailedPrecondition, "UnitMetadata not set")
            )?;
        *metadata_type = unit_metadata.get_type();
        Ok(())
    }
}

/// Returns the set value for ServerUnitMetadata of the UnitRegistrationArgs.
///
/// Parameters:
///    [IN] args: A valid pointer to the UnitRegistrationArgs object for which to get the ServerUnitMetadata
///    [OUT] server: Provides the value of ASDV_ServerUnitMetadata set for this UnitRegistrationArgs
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT if the server is not a valid pointer to struct ASDV_ServerUnitMetadata.
///    ASDV_STATUS_NOT_FOUND if the ServerUnitMetadata value has not been set for this UnitRegistrationArgs.
///
/// SAFETY: The caller guarantees that `args`, and `server` are all valid pointers that live for
/// at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitRegistrationArgs_getServerUnitMetadata(
    args: &ASDV_UnitRegistrationArgs,
    server_metadata: &mut ASDV_ServerUnitMetadata,
) -> sdv_status_t {
    try_status! {
        let unit_metadata = args.0.get_unit_metadata()
            .ok_or_else(||
                SdvStatus::with_message(SdvStatusCode::FailedPrecondition, "UnitMetadata not set")
            )?;
        if let Some(ref metadata_internal) = unit_metadata.get_server_metadata() {
            let server_metadata = ASDV_ServerUnitMetadata::peel_mut(server_metadata);
            server_metadata.clone_from(metadata_internal);
            Ok(())
        } else {
            Err(SdvStatus::with_message(
                SdvStatusCode::FailedPrecondition, "UnitMetadata::Server not set"))
        }
    }
}

/// Returns the set value for PublisherUnitMetadata of the UnitRegistrationArgs.
///
/// Parameters:
///    [IN] args: A valid pointer to the UnitRegistrationArgs object for which to get the PublisherUnitMetadata
///    [OUT] publisher: Provides the value of ASDV_PublisherUnitMetadata set for this UnitRegistrationArgs
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT if the publisher is not a valid pointer to struct ASDV_PublisherUnitMetadata.
///    ASDV_STATUS_NOT_FOUND if the PublisherUnitMetadata value has not been set for this UnitRegistrationArgs.
///
/// SAFETY: The caller guarantees that `args`, and `publisher` are all valid pointers that live for
/// at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnitRegistrationArgs_getPublisherUnitMetadata(
    args: &ASDV_UnitRegistrationArgs,
    publisher_metadata: &mut ASDV_PublisherUnitMetadata,
) -> sdv_status_t {
    try_status! {
        let unit_metadata = args.0.get_unit_metadata()
            .ok_or_else(||
                SdvStatus::with_message(SdvStatusCode::FailedPrecondition, "UnitMetadata not set")
            )?;
        if let Some(ref metadata_internal) = unit_metadata.get_publisher_metadata() {
            let publisher_metadata = ASDV_PublisherUnitMetadata::peel_mut(publisher_metadata);
            publisher_metadata.clone_from(metadata_internal);
            Ok(())
        } else {
            Err(SdvStatus::with_message(
                SdvStatusCode::FailedPrecondition, "UnitMetadata::Publisher not set"))
        }
    }
}
