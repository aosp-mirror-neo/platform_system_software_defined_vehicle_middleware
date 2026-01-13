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

#![allow(dead_code, non_camel_case_types, non_upper_case_globals)]

use sdv_comms_common::define_callback_ffi_struct;
use sdv_comms_common::define_once_callback_ffi_struct;
use sdv_comms_stack_ffi_macro::comms_stack_ffi_struct;

#[comms_stack_ffi_struct(opaque)]
pub struct ASDV_PublicationHandle {
    pub(crate) publisher: crate::core::publisher_internal::PublisherInternal,
}

#[comms_stack_ffi_struct(opaque)]
pub struct ASDV_SubscriptionHandle {
    pub(crate) subscriber: crate::core::subscriber_internal::SubscriberInternal,
}

#[comms_stack_ffi_struct(opaque)]
pub struct ASDV_HistoryHandle {
    pub(crate) history: crate::core::history_internal::HistoryInternal,
}

// Notification Types
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(i32)]
pub enum ASDV_NotificationType {
    // New data available
    DataAvailable = 0x1i32,
    // An exception occurred
    Exception = 0x2i32,
}

// Subscription Flags

// If ASDV_SubscriptionFlags_FetchLastMessage flag is set, the subscriber will receive the last
// message that was published to the topic. Otherwise, the subscriber will only receive messages
// published after it subscribes.
pub const ASDV_SubscriptionFlags_None: u64 = 0x0;
pub const ASDV_SubscriptionFlags_FetchLastMessage: u64 = 0x1;

// The maximum size of a message that can be received via InstantRead is limited to 64KB
// due to Binder limitations and overall system performance reasons.
pub const ASDV_MaxInstantReadMessageSize: u32 = 64 * 1024;

// Define the struct ASDV_CallbackVTable_PublicationCreate.
define_once_callback_ffi_struct!(PublicationCreate, Box<ASDV_PublicationHandle>);
// Define the struct ASDV_CallbackVTable_SubscribeToPublication.
define_once_callback_ffi_struct!(SubscribeToPublication, Box<ASDV_SubscriptionHandle>);

// Define the struct ASDV_CallbackVTable_SetNotificationHandler.
define_callback_ffi_struct!(SetNotificationHandler, ASDV_NotificationType);

#[cfg(test)]
type NotificationCallbackWrapper =
    sdv_comms_common::callback::sdk::CallbackWrapper<ASDV_NotificationType>;

impl ASDV_CallbackVTable_SetNotificationHandler {
    #[cfg(test)]
    pub fn new(cb: Box<NotificationCallbackWrapper>) -> Self {
        Self {
            func: NotificationCallbackWrapper::trampoline_func,
            clear: Some(NotificationCallbackWrapper::trampoline_clear),
            cb_data: cb.into_void_ptr(),
        }
    }
}

// Define the struct ASDV_CallbackVTable_SubscribeToHistory.
define_once_callback_ffi_struct!(SubscribeToHistory, Box<ASDV_HistoryHandle>);

// Define the struct ASDV_CallbackVTable_GetLastPublicationMessage.
define_once_callback_ffi_struct!(GetLastPublicationMessage, u32);
