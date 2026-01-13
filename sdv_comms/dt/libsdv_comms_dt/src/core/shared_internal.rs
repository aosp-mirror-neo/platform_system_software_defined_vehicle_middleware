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

//! SDV COMMS DT internal shared routines

use crate::ffi::ffi_types;

use sdv_fmq_rust::FmqWrapperRust;
use sdv_status_rs::SdvResult;

pub fn adjust_read_pointer(
    fmq: &mut FmqWrapperRust,
    message_size: usize,
    message_count: usize,
    subscription_flags: u64,
) -> SdvResult<usize> {
    let mut available_bytes = fmq.available_to_read();
    if available_bytes > message_size * message_count {
        // Overflow, triggering a reset of the reading pointer
        fmq.skip_messages(0);
        available_bytes = fmq.available_to_read();
    }
    let mut messages_to_skip = available_bytes / message_size;
    if messages_to_skip > 0
        && (subscription_flags & ffi_types::ASDV_SubscriptionFlags_FetchLastMessage) != 0
    {
        // Keep the last message
        messages_to_skip -= 1;
    }

    if messages_to_skip > 0 {
        fmq.skip_messages(messages_to_skip * message_size);
    }

    // return available_to_read after adjusting the reader pointer
    Ok(fmq.available_to_read())
}
