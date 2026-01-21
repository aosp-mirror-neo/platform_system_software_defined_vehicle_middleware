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

#![allow(non_camel_case_types)]

use bytemuck::TransparentWrapper;
use core::ffi::c_void;
use sdv_comms_common::callback::ffi::OnceCallback;
use sdv_comms_common::try_status;
use sdv_comms_ctx::ffi::ASDV_Context;
use sdv_comms_id::ffi::ASDV_UnitIdentifier;
use sdv_comms_id::names::ChannelName;
use sdv_comms_id::UnitIdentifier;
use sdv_comms_sd::discovery::UnitType;
use sdv_comms_sd::ffi::{ASDV_PublisherUnitMetadata, ASDV_UnitRegistrationArgs, ASDV_UnitType};
use sdv_status_rs::{sdv_status_t, SdvStatus, SdvStatusCode};
use std::slice;
use std::str::FromStr;

use crate::core::history_internal::HistoryInternal;
use crate::core::instant_reader_internal;
use crate::core::publisher_internal::PublisherInternal;
use crate::core::subscriber_internal::SubscriberInternal;
use crate::ffi::ffi_types;
use crate::ffi::ffi_types::{ASDV_HistoryHandle, ASDV_PublicationHandle, ASDV_SubscriptionHandle};

/// FFI implementation for `ASDV_CreatePublication`
/// Creates a publication/unit with the specified unit registration arguments.
///
/// SAFETY:
///   `context` is a pointer to a valid Context object
///   `nameArgs` is a pointer to a valid ASDV_UnitRegistrationArgs object
///   `callback` is a pointer to a valid OnceCallback<Box<ASDV_PublicationHandle>> object
pub fn create_publication(
    context: &'static ASDV_Context,
    unit_reg_args: ASDV_UnitRegistrationArgs,
    callback: OnceCallback<Box<ASDV_PublicationHandle>>,
) -> sdv_status_t {
    try_status! {
        let context = ASDV_Context::peel_ref(context);
        let unit_reg_args = ASDV_UnitRegistrationArgs::peel(unit_reg_args);

        context.get_executor().spawn(Box::pin(async move {
            let res = PublisherInternal::create_publication(context, unit_reg_args).await;
            callback.invoke(res.map(|publisher| Box::new(ASDV_PublicationHandle { publisher })));
        }));

        Ok(())
    }
}

/// FFI implementation for `ASDV_GetPublisherUnitMetadata`
/// Gets PublicationMetadata from Publisher
///
/// SAFETY:
///   `publication_handle` is a pointer to a valid ASDV_PublicationHandle object
///   `publisher_metadata` is a pointer to a valid mutable ASDV_PublisherUnitMetadata object
pub fn get_publisher_unit_metadata(
    publication_handle: &ffi_types::ASDV_PublicationHandle,
    publisher_metadata: &mut ASDV_PublisherUnitMetadata,
) -> sdv_status_t {
    try_status! {
        let metadata = publication_handle.publisher.get_publisher_unit_metadata();
        let publisher_metadata = ASDV_PublisherUnitMetadata::peel_mut(publisher_metadata);
        publisher_metadata.clone_from(metadata);
        Ok(())
    }
}

/// FFI implementation for `ASDV_GetPublisherUnitMetadataFromSubscriber`
/// Gets PublicationMetadata from Subscriber
///
/// SAFETY:
///   `subscription_handle` is a pointer to a valid ASDV_SubscriptionHandle object
///   `publisher_metadata` is a pointer to a valid mutable ASDV_PublisherUnitMetadata object
pub fn get_publisher_unit_metadata_from_subscriber(
    subscription_handle: &ffi_types::ASDV_SubscriptionHandle,
    publisher_metadata: &mut ASDV_PublisherUnitMetadata,
) -> sdv_status_t {
    try_status! {
        let metadata = subscription_handle.subscriber.get_publisher_unit_metadata();
        let publisher_metadata = ASDV_PublisherUnitMetadata::peel_mut(publisher_metadata);
        publisher_metadata.clone_from(metadata);
        Ok(())
    }
}

/// FFI implementation for `ASDV_GetPublisherUnitMetadataFromHistory`
/// Gets PublicationMetadata from History
///
/// SAFETY:
///   `history_handle` is a pointer to a valid ASDV_HistoryHandle object
///   `publisher_metadata` is a pointer to a valid mutable ASDV_PublisherUnitMetadata object
pub fn get_publisher_unit_metadata_from_history(
    history_handle: &ffi_types::ASDV_HistoryHandle,
    publisher_metadata: &mut ASDV_PublisherUnitMetadata,
) -> sdv_status_t {
    try_status! {
        let metadata = history_handle.history.get_publisher_unit_metadata();
        let publisher_metadata = ASDV_PublisherUnitMetadata::peel_mut(publisher_metadata);
        publisher_metadata.clone_from(metadata);
        Ok(())
    }
}

/// FFI implementation for `ASDV_WriteMessages`
/// Writes messages to the publication's buffer
///
/// SAFETY:
///   `publication_handle` is a pointer to a valid ASDV_PublicationHandle object
///   `messages` should refer to an array of size at least message_count*message_size
///   `buffer_size` size of the buffer in bytes referenced by messages not overflow
///   `written` must be a valid pointer to u32
pub unsafe fn write_messages(
    publication_handle: &mut ffi_types::ASDV_PublicationHandle,
    messages: *const c_void,
    buffer_size: u32,
    written: &mut u32,
) -> sdv_status_t {
    try_status! {
        let message_size = publication_handle.publisher.get_message_size();

        let message_count = buffer_size
        .checked_rem(message_size)
        .filter(|&remainder| remainder == 0)
        .map(|_| buffer_size / message_size)
        .ok_or_else(|| SdvStatus::with_message(SdvStatusCode::InvalidArgument, "Invalid buffer size"))?;

        // SAFETY: The caller has guaranteed that buffer_size does not overflow and the length of
        // messages is at least buffer_size making this safe to call.
        let data = unsafe {
            slice::from_raw_parts(messages as *const i8, buffer_size as usize)
        };
        if publication_handle.publisher.write_messages(data, message_count)? {
            *written = message_count;
            Ok(())
        } else {
            Err(SdvStatus::with_message(
                SdvStatusCode::Unknown, "Unable to write messages to fmq"))
        }
    }
}

/// FFI implementation for `ASDV_SubscribeToPublication`
/// Subscribes to a publication with the specified unit name using provided ASDV_UnitIdentifier
/// and returns the newly created subscription as a pointer to ASDV_SubscriptionHandle via
/// callback.
///
/// SAFETY:
///   `context` is a pointer to a valid Context object
///   `unit_identifier` is a pointer to a valid ASDV_UnitIdentifier object
///   `callback` is a pointer to a valid OnceCallback<Box<ASDV_SubscriptionHandle>> object
pub fn subscribe_to_publication(
    context: &'static ASDV_Context,
    unit_identifier: &ASDV_UnitIdentifier,
    subscription_flags: u64,
    callback: OnceCallback<Box<ffi_types::ASDV_SubscriptionHandle>>,
) -> sdv_status_t {
    try_status! {
        let context = ASDV_Context::peel_ref(context);
        let unit_identifier: UnitIdentifier = unit_identifier.try_into()?;

        context.get_executor().spawn(Box::pin(async move {
            let res = SubscriberInternal::subscribe_to_publication(
                context,
                &unit_identifier,
                subscription_flags,
            )
            .await;
            callback.invoke(res.map(|subscriber| Box::new(ASDV_SubscriptionHandle { subscriber })));
        }));

        Ok(())
    }
}

/// FFI implementation for `ASDV_SubscribeToUnitType`
/// Subscribes to all the publications with the specified unit type using provided ASDV_UnitType
/// and returns the newly created subscription as a pointer to ASDV_SubscriptionHandle via
/// callback.
///
/// SAFETY:
///   `context` is a pointer to a valid Context object
///   `unit_identifier` is a pointer to a valid ASDV_UnitIdentifier object
///   `callback` is a pointer to a valid OnceCallback<Box<ASDV_SubscriptionHandle>> object
pub fn subscribe_to_unit_type(
    context: &'static ASDV_Context,
    unit_type: &ASDV_UnitType,
    subscription_flags: u64,
    callback: OnceCallback<Box<ffi_types::ASDV_SubscriptionHandle>>,
) -> sdv_status_t {
    try_status! {
        let context = ASDV_Context::peel_ref(context);
        let unit_type: UnitType = unit_type.try_into()?;

        context.get_executor().spawn(Box::pin(async move {
            let res = SubscriberInternal::subscribe_to_unit_type(
                context,
                &unit_type,
                subscription_flags,
            )
            .await;
            callback.invoke(res.map(|subscriber| Box::new(ASDV_SubscriptionHandle { subscriber })));
        }));

        Ok(())
    }
}

/// FFI implementation for `ASDV_SubscribeToChannel`
/// Subscribes to all the publications with the specified unit type and channel name using provided
/// ASDV_UnitType and channel_name *.
/// Returns the newly created subscription as a pointer to ASDV_SubscriptionHandle via callback.
///
/// SAFETY:
///   `context` is a pointer to a valid Context object
///   `unit_identifier` is a pointer to a valid ASDV_UnitIdentifier object
///   `channel_name` is a pointer to a valid NUL-terminated string
///   `callback` is a pointer to a valid OnceCallback<Box<ASDV_SubscriptionHandle>> object
pub unsafe fn subscribe_to_channel(
    context: &'static ASDV_Context,
    unit_type: &ASDV_UnitType,
    channel_name: *const std::ffi::c_char,
    subscription_flags: u64,
    callback: OnceCallback<Box<ffi_types::ASDV_SubscriptionHandle>>,
) -> sdv_status_t {
    try_status! {
        let context = ASDV_Context::peel_ref(context);
        let unit_type: UnitType = unit_type.try_into()?;
        // SAFETY: `channel_name` is a pointer to a valid NUL-terminated string
        let channel_name = unsafe { std::ffi::CStr::from_ptr(channel_name) }
            .to_str()
            .map_err(|e| {
                SdvStatus::with_message(SdvStatusCode::InvalidArgument, format!("Invalid channel name: {e}"))
            })?;
        let channel_name = ChannelName::from_str(channel_name).map_err(|e| {
            SdvStatus::with_message(SdvStatusCode::InvalidArgument, format!("Invalid channel name: {e}"))
        })?;

        context.get_executor().spawn(Box::pin(async move {
            let res = SubscriberInternal::subscribe_to_channel(
                context,
                &unit_type,
                &channel_name,
                subscription_flags,
            )
            .await;
            callback.invoke(res.map(|subscriber| Box::new(ASDV_SubscriptionHandle { subscriber })));
        }));

        Ok(())
    }
}

/// FFI implementation for `ASDV_AvailableToRead`
/// Reports the number of new messages available to read
///
/// SAFETY:
///   `subscription_handle` is a pointer to a valid ASDV_SubscriptionHandle object
///   `message_count` must be a valid pointer to u32
pub unsafe fn available_to_read(
    subscription_handle: &ffi_types::ASDV_SubscriptionHandle,
    message_count: &mut u32,
) -> sdv_status_t {
    try_status! {
        *message_count = subscription_handle.subscriber.available_to_read()? as u32;
        Ok(())
    }
}

/// FFI implementation for `ASDV_ReadMessages`
/// Reads up to "message_count" messages from the publication's buffer
///
/// Important Note:
///   This function may modify the contents of the `messages`` buffer even
///   when returning an error; for performance reasons it cannot detect all
///   error conditions in advance of any writes.
///
/// SAFETY:
///   `subscription_handle` is a pointer to a valid ASDV_SubscriptionHandle object
///   `messages` should refer to an array of size at least message_count*message_size
///   `buffer_size` size of the buffer in bytes referenced by messages not overflow
///   `read` must be a valid pointer to u32
pub unsafe fn read_messages(
    subscription_handle: &mut ffi_types::ASDV_SubscriptionHandle,
    messages: *mut c_void,
    buffer_size: u32,
    read: &mut u32,
) -> sdv_status_t {
    try_status! {
        let message_size = subscription_handle.subscriber.get_message_size();
        let messages_to_read = buffer_size
        .checked_rem(message_size)
        .filter(|&remainder| remainder == 0)
        .map(|_| buffer_size / message_size)
        .ok_or_else(|| SdvStatus::with_message(SdvStatusCode::InvalidArgument, "Invalid buffer size"))?;

        // SAFETY: The caller has guaranteed that buffer_size does not overflow and the length of
        // messages is at least buffer_size making this safe to call.
        let mut data = unsafe {
            slice::from_raw_parts_mut(messages as *mut i8, buffer_size as usize)
        };

        *read = subscription_handle.subscriber.read(data, messages_to_read as usize)? as u32;
        Ok(())
    }
}

/// FFI implementation for `ASDV_SetNotificationHandler`
/// Sets callback for all notifications for the specified subscription
///
/// SAFETY:
///   `subscription_handle` is a pointer to a valid ASDV_SubscriptionHandle object
///   `cb_vtable` is a pointer to a valid ffi_types::ASDV_CallbackVTable_SetNotificationHandler
///               object
pub fn set_notification_handler(
    subscription_handle: &mut ffi_types::ASDV_SubscriptionHandle,
    cb_vtable: ffi_types::ASDV_CallbackVTable_SetNotificationHandler,
    min_interval_ms: u32,
) -> sdv_status_t {
    try_status! {
        subscription_handle.subscriber.set_notification_handler(cb_vtable.into(), min_interval_ms)?;
        Ok(())
    }
}

/// FFI implementation for `ASDV_ClearNotificationHandler`
/// Disables notifications callback for the specified subscription
///
/// SAFETY:
///   `subscription_handle` is a pointer to a valid ASDV_SubscriptionHandle object
pub fn clear_notification_handler(
    subscription_handle: &mut ffi_types::ASDV_SubscriptionHandle,
) -> sdv_status_t {
    try_status! {
        subscription_handle.subscriber.clear_notification_handler()?;
        Ok(())
    }
}

/// FFI implementation for `ASDV_SubscribeToHistory`
/// Creates a "history_buffer_size"-size history aggregator for a publication with the given name and
/// subscribes to it. Stores the newly created history handle.
///
/// SAFETY:
///   `context` is a pointer to a valid Context object
///   `unit_identifier` is a pointer to a valid ASDV_UnitIdentifier object
///   `callback` is a pointer to a valid OnceCallback<Box<ffi_types::ASDV_HistoryHandle>> object
pub fn subscribe_to_history(
    context: &'static ASDV_Context,
    unit_identifier: &ASDV_UnitIdentifier,
    history_buffer_size: u32,
    callback: OnceCallback<Box<ffi_types::ASDV_HistoryHandle>>,
) -> sdv_status_t {
    try_status! {
        let context = ASDV_Context::peel_ref(context);
        let unit_identifier: UnitIdentifier = unit_identifier.try_into()?;

        context.get_executor().spawn(Box::pin(async move {
            let res =
                HistoryInternal::subscribe_to_history(context, &unit_identifier, history_buffer_size).await;
            callback.invoke(res.map(|history| Box::new(ASDV_HistoryHandle { history })));
        }));

        Ok(())
    }
}

/// FFI implementation for `ASDV_AvailableInHistory`
/// Reports the number of messages available in the history buffer
///
/// SAFETY:
///   `history_handle` is a pointer to a valid ASDV_HistoryHandle object
///   `message_count` must be a valid pointer to u32
pub unsafe fn available_in_history(
    history_handle: &ffi_types::ASDV_HistoryHandle,
    message_count: *mut u32,
) -> sdv_status_t {
    try_status! {
        // SAFETY: The caller has guaranteed that message_count points to a valid mutable u32
        unsafe {
            *message_count = history_handle.history.available_in_history()? as u32;
        }
        Ok(())
    }
}

/// FFI implementation for `ASDV_ReadHistoryMessages`
/// Reads "message_count" messages from the publication's history buffer
///
/// SAFETY:
///   `history_handle` is a pointer to a valid ASDV_HistoryHandle object
///   `messages` should refer to an array of size at least buffer_size
///   `buffer_size` size of the buffer in bytes referenced by messages not overflow
///   `read` must be a valid pointer to u32
pub unsafe fn read_history_messages(
    history_handle: &mut ffi_types::ASDV_HistoryHandle,
    messages: *mut c_void,
    buffer_size: u32,
    read: *mut u32,
) -> sdv_status_t {
    try_status! {
        let message_size = history_handle.history.get_message_size();

        let message_count = buffer_size
        .checked_rem(message_size)
        .filter(|&remainder| remainder == 0)
        .map(|_| buffer_size / message_size)
        .ok_or_else(|| SdvStatus::with_message(SdvStatusCode::InvalidArgument, "Invalid buffer size"))?;

        // SAFETY: The caller has guaranteed that buffer_size does not overflow and the length of
        // messages is at least buffer_size making this safe to call.
        let mut data = unsafe {
            slice::from_raw_parts_mut(messages as *mut i8, buffer_size as usize)
        };

        if let Ok(r) = history_handle.history.read(data, message_count) {
            // SAFETY: caller ensures read points to a valid mutable u32
            unsafe {
                *read = r;
            }
            Ok(())
        } else {
            Err(SdvStatus::with_message(
                SdvStatusCode::Unknown, "Unable to read messages from history buffer"))
        }
    }
}

/// FFI implementation for `ASDV_GetLastPublicationMessage`
/// Returns the last published message that was written to the specified publication without
/// subscribing to it or returns an error if the publication is empty.
///
/// SAFETY:
///   `context` is a pointer to a valid Context object
///   `unit_identifier` is a pointer to a valid ASDV_UnitIdentifier object
///   `messages` should refer to an array of size at least buffer_size. The buffer must remain
///              valid until the callback is inoked
///   `buffer_size` size of the buffer in bytes referenced by messages not overflow
///   `callback` is a pointer to a valid
///              OnceCallback<Box<ffi_types::ASDV_CallbackVTable_GetLastPublicationMessage>>
///              object
pub unsafe fn get_last_publication_message(
    context: &'static ASDV_Context,
    unit_identifier: &ASDV_UnitIdentifier,
    messages: *mut c_void,
    buffer_size: u32,
    callback: OnceCallback<u32>,
) -> sdv_status_t {
    try_status! {
        let context = ASDV_Context::peel_ref(context);
        let unit_identifier: UnitIdentifier = unit_identifier.try_into()?;
        // SAFETY: The caller has guaranteed that buffer_size does not overflow and the length of
        // messages is at least buffer_size making this safe to call.
        let mut data = unsafe {
            slice::from_raw_parts_mut(messages as *mut i8, buffer_size as usize)
        };

        context.get_executor().spawn(Box::pin(async move {
            let res =
                instant_reader_internal::get_last_publication_message(context, &unit_identifier, data).await;
            callback.invoke(res);
        }));

        Ok(())
    }
}
