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

use sdv_comms_stack_ffi_macro::{comms_stack_ffi_fn, comms_stack_ffi_struct};
use sdv_status_rs::{sdv_status_t, SdvStatusCode};

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
