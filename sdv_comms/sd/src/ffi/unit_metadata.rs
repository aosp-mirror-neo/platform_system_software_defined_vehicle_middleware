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

use core::ffi::c_char;
use sdv_comms_common::ffi_str;
use sdv_comms_stack_ffi_macro::{comms_stack_ffi_fn, comms_stack_ffi_struct};
use sdv_status_rs::{derive, sdv_status_t, SdvStatusCode};

#[cfg(not(comms_stack_stub))]
use crate::metadata::{InferredMetadata, PublisherUnitMetadata, ServerUnitMetadata};

/// Standardised metadata about a Server Unit registered with Service Discovery.
#[comms_stack_ffi_struct(opaque)]
#[derive(Default, Clone, PartialEq)]
pub struct ASDV_ServerUnitMetadata(ServerUnitMetadata);

/// Creates a ASDV_ServerUnitMetadata.
///
/// This function always returns a valid heap-allocated ASDV_ServerUnitMetadata. It aborts
/// if it fails to allocate the object on the heap. The caller is responsible for freeing the
/// returned object.
///
/// SAFETY: The caller takes ownership of the returned object and is responsible for freeing the
/// memory pointed to by the returned pointer using ASDV_ServerUnitMetadata_destroy().
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServerUnitMetadata_create() -> Box<ASDV_ServerUnitMetadata> {
    Box::new(ASDV_ServerUnitMetadata(ServerUnitMetadata::default()))
}

/// Clones a ASDV_ServerUnitMetadata.
///
/// This function always returns a valid heap-allocated ASDV_ServerUnitMetadata. It aborts
/// if it fails to allocate the object on the heap. The caller is responsible for freeing the
/// returned object.
///
/// SAFETY: The caller takes ownership of the returned object and is responsible for freeing the
/// memory pointed to by the returned pointer using ASDV_ServerUnitMetadata_destroy().
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServerUnitMetadata_clone(
    metadata: Option<&ASDV_ServerUnitMetadata>,
) -> Option<Box<ASDV_ServerUnitMetadata>> {
    metadata.map(|metadata| Box::new(metadata.clone()))
}

/// Compares two instances of ASDV_ServerUnitMetadata.
///
/// SAFETY: The caller guarantees that `first` and `second` are valid pointers created with
/// ASDV_ServerUnitMetadata_create().
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServerUnitMetadata_equals(
    first: &ASDV_ServerUnitMetadata,
    second: &ASDV_ServerUnitMetadata,
    result: &mut bool,
) -> sdv_status_t {
    *result = first == second;

    SdvStatusCode::Ok.into_raw()
}

/// Deletes a ASDV_ServerUnitMetadata returned by ASDV_ServerUnitMetadata_create() function.
///
/// Parameters:
///    [IN] ptr: The ASDV_ServerUnitMetadata value to delete.
///
/// SAFETY: The caller guarantees that `ptr` is a valid pointer created with
/// ASDV_ServerUnitMetadata_create().
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServerUnitMetadata_destroy(
    ptr: Option<Box<ASDV_ServerUnitMetadata>>,
) {
    if let Some(metadata) = ptr {
        drop(metadata)
    }
}

/// Sets value for version of the ASDV_ServerUnitMetadata.
///
/// If the function is called multiple times, the version value associated with the provided
/// ASDV_ServerUnitMetadata will be overwritten.
///
/// Parameters:
///    [IN] ptr: A valid pointer to the ASDV_ServerUnitMetadata object for which to set the version
///    [IN] version: The version value to set for this PublisherUnitMetadata
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT in case if `version` is not a valid pointer.
///
/// SAFETY: The caller guarantees that `ptr`, `version` are all valid pointers that live for at
/// least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServerUnitMetadata_setVersion(
    metadata: &mut ASDV_ServerUnitMetadata,
    version: &u32,
) -> sdv_status_t {
    metadata.0.set_version(*version);

    SdvStatusCode::Ok.into_raw()
}

/// Returns the set value for version of the ASDV_ServerUnitMetadata.
///
/// Parameters:
///    [IN] ptr: A valid pointer to the ASDV_ServerUnitMetadata object for which to get the version
///    [OUT] version: The version value to set for this PublisherUnitMetadata
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_NOT_FOUND if the version value has not been set for this ASDV_ServerUnitMetadata.
///
/// SAFETY: The caller guarantees that `ptr`, and `version` are all valid pointers that live for
/// at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServerUnitMetadata_getVersion(
    metadata: &ASDV_ServerUnitMetadata,
    version: &mut u32,
) -> sdv_status_t {
    *version = metadata.0.get_version();

    SdvStatusCode::Ok.into_raw()
}

/// Standardised metadata about a Publisher Unit registered with Service Discovery.
#[comms_stack_ffi_struct(opaque)]
#[derive(Default, Clone, PartialEq)]
pub struct ASDV_PublisherUnitMetadata(PublisherUnitMetadata);

/// Creates a ASDV_PublisherUnitMetadata.
///
/// This function always returns a valid heap-allocated ASDV_PublisherUnitMetadata. It aborts
/// if it fails to allocate the object on the heap. The caller is responsible for freeing the
/// returned object.
///
/// SAFETY: The caller takes ownership of the returned object and is responsible for freeing the
/// memory pointed to by the returned pointer using ASDV_PublisherUnitMetadata_destroy().
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_PublisherUnitMetadata_create() -> Box<ASDV_PublisherUnitMetadata> {
    Box::new(ASDV_PublisherUnitMetadata(PublisherUnitMetadata::default()))
}

/// Clones a ASDV_PublisherUnitMetadata.
///
/// This function always returns a valid heap-allocated ASDV_PublisherUnitMetadata. It aborts
/// if it fails to allocate the object on the heap. The caller is responsible for freeing the
/// returned object.
///
/// SAFETY: The caller takes ownership of the returned object and is responsible for freeing the
/// memory pointed to by the returned pointer using ASDV_PublisherUnitMetadata_destroy().
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_PublisherUnitMetadata_clone(
    metadata: Option<&ASDV_PublisherUnitMetadata>,
) -> Option<Box<ASDV_PublisherUnitMetadata>> {
    metadata.map(|metadata| Box::new(metadata.clone()))
}

/// Compares two instances of ASDV_PublisherUnitMetadata.
///
/// SAFETY: The caller guarantees that `first` and `second` are valid pointers created with
/// ASDV_PublisherUnitMetadata_create().
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_PublisherUnitMetadata_equals(
    first: &ASDV_PublisherUnitMetadata,
    second: &ASDV_PublisherUnitMetadata,
    result: &mut bool,
) -> sdv_status_t {
    *result = first == second;

    SdvStatusCode::Ok.into_raw()
}

/// Deletes a ASDV_PublisherUnitMetadata returned by ASDV_PublisherUnitMetadata_create() function.
///
/// Parameters:
///    [IN] ptr: The ASDV_PublisherUnitMetadata value to delete.
///
/// SAFETY: The caller guarantees that `ptr` is a valid pointer created with
/// ASDV_PublisherUnitMetadata_create().
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_PublisherUnitMetadata_destroy(
    ptr: Option<Box<ASDV_PublisherUnitMetadata>>,
) {
    if let Some(metadata) = ptr {
        drop(metadata)
    }
}

/// Sets value for version of the ASDV_PublisherUnitMetadata.
///
/// If the function is called multiple times, the version value associated with the provided
/// ASDV_PublisherUnitMetadata will be overwritten.
///
/// Parameters:
///    [IN] ptr: A valid pointer to the ASDV_PublisherUnitMetadata object for which to set the version
///    [IN] version: The version value to set for this PublisherUnitMetadata
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT in case if `version` is not a valid pointer.
///
/// SAFETY: The caller guarantees that `ptr`, `version` are all valid pointers that live for at
/// least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_PublisherUnitMetadata_setVersion(
    metadata: &mut ASDV_PublisherUnitMetadata,
    version: &u32,
) -> sdv_status_t {
    metadata.0.set_version(*version);

    SdvStatusCode::Ok.into_raw()
}

/// Returns the set value for version of the ASDV_PublisherUnitMetadata.
///
/// Parameters:
///    [IN] ptr: A valid pointer to the ASDV_PublisherUnitMetadata object for which to get the version
///    [OUT] version: The version value to set for this PublisherUnitMetadata
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_NOT_FOUND if the version value has not been set for this ASDV_PublisherUnitMetadata.
///
/// SAFETY: The caller guarantees that `ptr`, and `version` are all valid pointers that live for
/// at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_PublisherUnitMetadata_getVersion(
    metadata: &ASDV_PublisherUnitMetadata,
    version: &mut u32,
) -> sdv_status_t {
    *version = metadata.0.get_version();

    SdvStatusCode::Ok.into_raw()
}

/// Sets value for MessageSize of the ASDV_PublisherUnitMetadata.
///
/// If the function is called multiple times, the MessageSize value associated with the provided
/// ASDV_PublisherUnitMetadata will be overwritten.
///
/// Parameters:
///    [IN] ptr: A valid pointer to the ASDV_PublisherUnitMetadata object for which to set the field
///    [IN] message_size: The MessageSize value to set for this PublisherUnitMetadata
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT in case if `message_size` is not a valid pointer.
///
/// SAFETY: The caller guarantees that `ptr`, `message_size` are all valid pointers that live for
/// at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_PublisherUnitMetadata_setMessageSize(
    metadata: &mut ASDV_PublisherUnitMetadata,
    message_size: &u32,
) -> sdv_status_t {
    metadata.0.set_message_size(*message_size);

    SdvStatusCode::Ok.into_raw()
}

/// Returns the set value for MessageSize of the ASDV_PublisherUnitMetadata.
///
/// Parameters:
///    [IN] ptr: A valid pointer to the ASDV_PublisherUnitMetadata object for which to get the
///              MessageSize field value
///    [OUT] message_size: The MessageSize value to set for this PublisherUnitMetadata
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_NOT_FOUND if the MessageSize value has not been set for this PublisherUnitMetadata.
///
/// SAFETY: The caller guarantees that `ptr`, and `message_size` are all valid pointers that live
/// for at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_PublisherUnitMetadata_getMessageSize(
    metadata: &ASDV_PublisherUnitMetadata,
    message_size: &mut u32,
) -> sdv_status_t {
    *message_size = metadata.0.get_message_size();

    SdvStatusCode::Ok.into_raw()
}

/// Sets value for MessageCount of the ASDV_PublisherUnitMetadata.
///
/// If the function is called multiple times, the MessageCount value associated with the provided
/// ASDV_PublisherUnitMetadata will be overwritten.
///
/// Parameters:
///    [IN] ptr: A valid pointer to the ASDV_PublisherUnitMetadata object for which to set the
///              MessageCount value
///    [IN] message_count: The MessageCount value to set for this PublisherUnitMetadata
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT in case if `message_count` is not a valid pointer.
///
/// SAFETY: The caller guarantees that `ptr`, `message_count` are all valid pointers that live for
/// at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_PublisherUnitMetadata_setMessageCount(
    metadata: &mut ASDV_PublisherUnitMetadata,
    message_count: &u32,
) -> sdv_status_t {
    metadata.0.set_message_count(*message_count);

    SdvStatusCode::Ok.into_raw()
}

/// Returns the set value for MessageCount of the ASDV_PublisherUnitMetadata.
///
/// Parameters:
///    [IN] ptr: A valid pointer to the ASDV_PublisherUnitMetadata object for which to get the
///              MessageCount value
///    [OUT] message_count: The MessageCount value to set for this PublisherUnitMetadata
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_NOT_FOUND if the MessageCount value has not been set for this PublisherUnitMetadata.
///
/// SAFETY: The caller guarantees that `ptr`, and `message_count` are all valid pointers that live
/// for at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_PublisherUnitMetadata_getMessageCount(
    metadata: &ASDV_PublisherUnitMetadata,
    message_count: &mut u32,
) -> sdv_status_t {
    *message_count = metadata.0.get_message_count();

    SdvStatusCode::Ok.into_raw()
}

/// Sets topic name of the ASDV_PublisherUnitMetadata.
///
/// If the function is called multiple times, the topic name associated with the provided
/// ASDV_PublisherUnitMetadata will be overwritten.
///
/// Parameters:
///    [IN] metadata: A valid pointer to the ASDV_PublisherUnitMetadata object for which to set the
//              topic name
///    [IN] topic: The topic name to set for this PublisherUnitMetadata. Must be a valid
///                 ASCII string up to 127 bytes.
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT in case if `topic` is not a valid pointer or the string
///                                 is not a valid topic name.
///
/// # Safety
///
/// The caller guarantees that `metadata` and `topic` are all valid pointers that live for
/// at least the duration of the function call.
/// - `topic` must point at a NUL-terminated string.
/// - `topic` must abide by the safety requirements of CStr::from_ptr.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_PublisherUnitMetadata_setTopic(
    metadata: &mut ASDV_PublisherUnitMetadata,
    #[allow_ptr] topic: *const std::os::raw::c_char,
) -> sdv_status_t {
    sdv_comms_common::try_status! {
        // # SAFETY: The caller guarantees that `topic` is a valid pointer that lives for
        // at least the duration of the function call.
        // cstr_pointer_to_str creates a new String which is then used in `set_topic_name`,
        // therefore it's not necessary for `topic` to outlive the function call.
        let topic_string = unsafe { ffi_str::cstr_pointer_to_str(topic as *const c_char)? };
        metadata.0.set_topic_name(topic_string.try_into().map_err(|e| {
            sdv_status_rs::SdvStatus::with_message(SdvStatusCode::InvalidArgument, format!("Invalid topic name: {e}"))
        })?);
        Ok(())
    }
}

/// Returns the topic name of the ASDV_PublisherUnitMetadata.
///
/// Parameters:
///    [IN] ptr: A valid pointer to the ASDV_PublisherUnitMetadata object for which to get the
///              topic name
///    [IN/OUT] buf: Pointer to char buffer into which the string will be written as a UTF-8 encoded
///                  NUL-terminated C string. It's expected that the input buffer has a size of 128
///                  bytes, which is the max size of topic.
///    [IN/OUT] buf_size: Represent the original size of the buffer pointed by `buf` and returns the number
///                       of bytes written to the provided buffer (including the NUL character)
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT if the buf_size is not large enough to hold the topic name.
///
/// # Safety
///
/// The caller guarantees that `ptr`, `buf`, and `buf_size` are all valid pointers that live
/// for at least the duration of the function call.
/// The caller guarantees that `buf` points to a valid buffer of size at least `buf_size` bytes
///
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_PublisherUnitMetadata_getTopic(
    metadata: &ASDV_PublisherUnitMetadata,
    #[allow_ptr] buf: *mut std::os::raw::c_char,
    buf_size: &mut usize,
) -> sdv_status_t {
    sdv_comms_common::try_status! {
        let topic_str = metadata.0.get_topic_name();
        let topic_bytes = topic_str.as_bytes();
        // validate that the topic name fits in the provided buf_size
        if topic_bytes.len() + 1 > *buf_size {
            return Err(sdv_status_rs::SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                "Buffer size is too small to hold the topic string",
            ));
        }
        // SAFETY: The caller guarantees that `buf` is a valid pointer to a buffer of at least
        // buf_size bytes that live for at least the duration of the function call.
        let output_slice: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(buf.cast::<u8>(), *buf_size) };
        output_slice[..topic_bytes.len()].copy_from_slice(topic_bytes);
        output_slice[topic_bytes.len()] = 0;
        *buf_size = topic_bytes.len() + 1;
        Ok(())
    }
}

/// Representation of RelativeLocation enum.
#[derive(Debug, PartialEq)]
#[repr(u8)]
pub enum ASDV_RelativeLocation {
    /// Representation of InferredMetadata::RelativeLocation::LOCATION_UNKNOWN.
    ASDV_LOCATION_UNKNOWN = 0,
    /// Representation of InferredMetadata::RelativeLocation::LOCATION_LOCAL_SDV_INSTANCE.
    ASDV_LOCATION_LOCAL_SDV_INSTANCE = 1,
    /// Representation of InferredMetadata::RelativeLocation::LOCATION_LOCAL_HARDWARE.
    ASDV_LOCATION_LOCAL_HARDWARE = 2,
    /// Representation of InferredMetadata::RelativeLocation::LOCATION_REMOTE.
    ASDV_LOCATION_REMOTE = 3,
}

/// Inferred metadata of the Service Unit registered with Service Discovery.
#[comms_stack_ffi_struct(opaque)]
#[derive(Default, Clone, PartialEq)]
pub struct ASDV_InferredMetadata(InferredMetadata);

/// Returns the set value for MessageCount of the ASDV_PublisherUnitMetadata.
///
/// Parameters:
///    [IN] ptr: A valid pointer to the ASDV_InferredMetadata object for which to get the
///              RelativeLocation value
///    [OUT] message_count: The RelativeLocation value to set for this ASDV_InferredMetadata
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_NOT_FOUND if the RelativeLocation value has not been set for this
///                          ASDV_InferredMetadata.
///
/// SAFETY: The caller guarantees that `ptr`, and `location` are valid pointers that live for at
/// least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_InferredMetadata_getLocation(
    metadata: &ASDV_InferredMetadata,
    location: &mut ASDV_RelativeLocation,
) -> sdv_status_t {
    *location = metadata.0.get_location();

    SdvStatusCode::Ok.into_raw()
}

/// Creates a ASDV_InferredMetadata.
///
/// This function always returns a valid heap-allocated ASDV_InferredMetadata. It aborts
/// if it fails to allocate the object on the heap. The caller is responsible for freeing the
/// returned object.
///
/// SAFETY: The caller takes ownership of the returned object and is responsible for freeing the
/// memory pointed to by the returned pointer using ASDV_InferredMetadata_destroy().
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_InferredMetadata_create() -> Box<ASDV_InferredMetadata> {
    Box::new(ASDV_InferredMetadata(InferredMetadata::default()))
}

/// Deletes a ASDV_InferredMetadata returned by ASDV_InferredMetadata_create() function.
///
/// Parameters:
///    [IN] ptr: The ASDV_InferredMetadata value to delete.
///
/// SAFETY: The caller guarantees that `ptr` is a valid pointer created with
/// ASDV_InferredMetadata_create().
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_InferredMetadata_destroy(ptr: Option<Box<ASDV_InferredMetadata>>) {
    if let Some(metadata) = ptr {
        drop(metadata)
    }
}
