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

//! This module defines helpers for using Callbacks in the Comms Stack. See docs on sdk.rs and ffi.rs

pub mod ffi;
pub mod sdk;

mod maybe_used;
use maybe_used::MaybeUsed;

#[cfg(test)]
mod once_callback_tests;

#[cfg(test)]
mod callback_tests;
