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
#![allow(missing_docs)]

mod ffi;

pub mod core {
    mod comm_channel;
    pub mod history_internal;
    pub(crate) mod instant_reader_internal;
    pub(crate) mod multiplexer_internal;
    pub mod publisher_internal;
    pub(crate) mod shared_internal;
    pub(crate) mod state;
    pub mod subscriber_internal;
}

#[cfg(test)]
mod test;
