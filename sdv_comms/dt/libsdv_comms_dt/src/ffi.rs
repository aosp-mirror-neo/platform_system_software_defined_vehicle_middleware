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
//! SDV COMMS DT implementation of C API
#![allow(non_camel_case_types)]
#![allow(missing_docs)]
#![cfg_attr(not(any(comms_stack_ffi, comms_stack_stub)), allow(unused_imports))]

use sdv_comms_ctx::ffi::ASDV_Context;
use sdv_comms_id::ffi::ASDV_UnitIdentifier;
use sdv_comms_sd::ffi::{ASDV_PublisherUnitMetadata, ASDV_UnitRegistrationArgs, ASDV_UnitType};
use sdv_comms_stack_ffi_macro::comms_stack_ffi_fn;
use sdv_status_rs::{sdv_status_t, SdvStatusCode};

#[cfg(comms_stack_ffi)]
pub(crate) mod ffi_impl;
pub(crate) mod ffi_types;

/********************
 *  Publisher API   *
 ********************/
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_CreatePublication(
    context: &'static ASDV_Context,
    unit_reg_args: Box<ASDV_UnitRegistrationArgs>,
    #[allow_value] cb_vtable: ffi_types::ASDV_CallbackVTable_PublicationCreate,
) -> sdv_status_t {
    ffi_impl::create_publication(context, *unit_reg_args, cb_vtable.into())
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_GetPublisherUnitMetadata(
    publication_handle: &ffi_types::ASDV_PublicationHandle,
    publisher_metadata: &mut ASDV_PublisherUnitMetadata,
) -> sdv_status_t {
    ffi_impl::get_publisher_unit_metadata(publication_handle, publisher_metadata)
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_GetPublisherUnitMetadataFromSubscriber(
    subscription_handle: &ffi_types::ASDV_SubscriptionHandle,
    publisher_metadata: &mut ASDV_PublisherUnitMetadata,
) -> sdv_status_t {
    ffi_impl::get_publisher_unit_metadata_from_subscriber(subscription_handle, publisher_metadata)
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_GetPublisherUnitMetadataFromHistory(
    history_handle: &ffi_types::ASDV_HistoryHandle,
    publisher_metadata: &mut ASDV_PublisherUnitMetadata,
) -> sdv_status_t {
    ffi_impl::get_publisher_unit_metadata_from_history(history_handle, publisher_metadata)
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_WriteMessages(
    publication_handle: &mut ffi_types::ASDV_PublicationHandle,
    #[allow_ptr] messages: *const std::ffi::c_void,
    buffer_size: u32,
    written: &mut u32,
) -> sdv_status_t {
    // SAFETY:
    // `buffer_size` size of the buffer in bytes referenced by messages
    // `messages` should refer to a valid array
    // `written` must point to a valid mutable i32
    unsafe { ffi_impl::write_messages(publication_handle, messages, buffer_size, written) }
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_DestroyPublication(
    publication_handle: Box<ffi_types::ASDV_PublicationHandle>,
) -> sdv_status_t {
    drop(publication_handle);
    SdvStatusCode::Ok.into_raw()
}

/********************
 *  Subscriber API  *
 ********************/

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_SubscribeToPublication(
    context: &'static ASDV_Context,
    unit_identifier: &ASDV_UnitIdentifier,
    subscription_flags: u64,
    #[allow_value] cb_vtable: ffi_types::ASDV_CallbackVTable_SubscribeToPublication,
) -> sdv_status_t {
    ffi_impl::subscribe_to_publication(
        context,
        unit_identifier,
        subscription_flags,
        cb_vtable.into(),
    )
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_SubscribeToUnitType(
    context: &'static ASDV_Context,
    unit_type: &ASDV_UnitType,
    subscription_flags: u64,
    #[allow_value] cb_vtable: ffi_types::ASDV_CallbackVTable_SubscribeToPublication,
) -> sdv_status_t {
    ffi_impl::subscribe_to_unit_type(context, unit_type, subscription_flags, cb_vtable.into())
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_SubscribeToChannel(
    context: &'static ASDV_Context,
    unit_type: &ASDV_UnitType,
    #[allow_ptr] channel_name: *const std::ffi::c_char,
    subscription_flags: u64,
    #[allow_value] cb_vtable: ffi_types::ASDV_CallbackVTable_SubscribeToPublication,
) -> sdv_status_t {
    // SAFETY:
    // `channel_name` is a pointer to a valid NUL-terminated string
    unsafe {
        ffi_impl::subscribe_to_channel(
            context,
            unit_type,
            channel_name,
            subscription_flags,
            cb_vtable.into(),
        )
    }
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_AvailableToRead(
    subscription_handle: &ffi_types::ASDV_SubscriptionHandle,
    message_count: &mut u32,
) -> sdv_status_t {
    // SAFETY:
    // `message_count` must be such that the buffer length (message_count * size of message) does
    // not overflow
    unsafe { ffi_impl::available_to_read(subscription_handle, message_count) }
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ReadMessages(
    subscription_handle: &mut ffi_types::ASDV_SubscriptionHandle,
    #[allow_ptr] messages: *mut std::ffi::c_void,
    buffer_size: u32,
    read: &mut u32,
) -> sdv_status_t {
    // SAFETY:
    // `buffer_size` size of the buffer in bytes
    // `messages` should refer to a valid array of size at least buffer_size
    unsafe { ffi_impl::read_messages(subscription_handle, messages, buffer_size, read) }
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_SetNotificationHandler(
    subscription_handle: &mut ffi_types::ASDV_SubscriptionHandle,
    #[allow_value] cb_vtable: ffi_types::ASDV_CallbackVTable_SetNotificationHandler,
    min_interval_ms: u32,
) -> sdv_status_t {
    ffi_impl::set_notification_handler(subscription_handle, cb_vtable, min_interval_ms)
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ClearNotificationHandler(
    subscription_handle: &mut ffi_types::ASDV_SubscriptionHandle,
) -> sdv_status_t {
    ffi_impl::clear_notification_handler(subscription_handle)
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnsubscribeFromPublication(
    subscription_handle: Box<ffi_types::ASDV_SubscriptionHandle>,
) -> sdv_status_t {
    drop(subscription_handle);
    SdvStatusCode::Ok.into_raw()
}

/************************
 *  History Reader API  *
 ************************/

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_SubscribeToHistory(
    context: &'static ASDV_Context,
    unit_identifier: &ASDV_UnitIdentifier,
    history_buffer_size: u32,
    #[allow_value] cb_vtable: ffi_types::ASDV_CallbackVTable_SubscribeToHistory,
) -> sdv_status_t {
    ffi_impl::subscribe_to_history(context, unit_identifier, history_buffer_size, cb_vtable.into())
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_AvailableInHistory(
    history_handle: &ffi_types::ASDV_HistoryHandle,
    message_count: &mut u32,
) -> sdv_status_t {
    // SAFETY:
    // `message_count` must be such that the buffer length (message_count * size of message) does
    // not overflow
    unsafe { ffi_impl::available_in_history(history_handle, message_count) }
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ReadHistoryMessages(
    history_handle: &mut ffi_types::ASDV_HistoryHandle,
    #[allow_ptr] messages: *mut std::ffi::c_void,
    buffer_size: u32,
    read: &mut u32,
) -> sdv_status_t {
    // SAFETY:
    // `buffer_size` size of the buffer in bytes
    // `messages` should refer to a valid array of size at least buffer_size
    // `read` should point to a valid mutable u32
    unsafe { ffi_impl::read_history_messages(history_handle, messages, buffer_size, read) }
}

#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_UnsubscribeFromHistory(
    history_handle: Box<ffi_types::ASDV_HistoryHandle>,
) -> sdv_status_t {
    drop(history_handle);
    SdvStatusCode::Ok.into_raw()
}

/**********************
 *  Instant Read API  *
 **********************/
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_GetLastPublicationMessage(
    context: &'static ASDV_Context,
    unit_identifier: &ASDV_UnitIdentifier,
    #[allow_ptr] buffer: *mut std::ffi::c_void,
    buffer_size: u32,
    #[allow_value] cb_vtable: ffi_types::ASDV_CallbackVTable_GetLastPublicationMessage,
) -> sdv_status_t {
    // TODO(b/369137307): For safety reasons the `buffer` ownership must be transferred to the
    //                    callback.
    //
    // SAFETY:
    //   `messages` should refer to an array of size at least buffer_size. The buffer must remain
    //              valid until the callback is inoked
    //   `buffer_size` size of the buffer in bytes referenced by messages not overflow
    //   `callback` is a pointer to a valid
    //              OnceCallback<Box<ffi_types::ASDV_CallbackVTable_GetLastPublicationMessage>>
    //              object
    unsafe {
        ffi_impl::get_last_publication_message(
            context,
            unit_identifier,
            buffer,
            buffer_size,
            cb_vtable.into(),
        )
    }
}
