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

use sdv_comms_bindgen::ASDV_SubscriptionFlags_FetchLastMessage;

/// Wraps Subscriber and SubscriberStream options
pub struct SubscriberOptions {
    /// If true, the subscriber will receive the last message that was
    /// published to the topic. Otherwise, the subscriber will only receive
    /// messages published after the subscription created.
    pub fetch_last_message: bool,
}

impl SubscriberOptions {
    pub(crate) fn as_raw_flags(&self) -> u64 {
        let mut flags = 0u64;

        if self.fetch_last_message {
            flags |= ASDV_SubscriptionFlags_FetchLastMessage;
        }

        flags
    }
}
