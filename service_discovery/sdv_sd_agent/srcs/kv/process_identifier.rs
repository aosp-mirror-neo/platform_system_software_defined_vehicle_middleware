// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//! The VM specific unique identifier for a Service Identity.
//! Each Service Bundle is registered with the local ProcessIdentifier so
//!  callers to various Service Discovery APIs can be identified.

use binder::ThreadState;
use google_sdv_identity::aidl::google::sdv::identity::{
    ProcessIdentifier::ProcessIdentifier,
    //
};

/// Returns the calling process identifier for a Binder client.
pub(crate) fn get_calling_process_identifier() -> ProcessIdentifier {
    ProcessIdentifier {
        pid: ThreadState::get_calling_pid() as i64,
        uid: ThreadState::get_calling_uid() as i64,
    }
}
