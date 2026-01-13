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
//! Common utilities for sdv_comms libraries.

/// Provides utilities to create a C ABI safe callback.
pub mod callback;

/// Common implementation for conversions between [&str] and NUL-terminated ASCII encoded byte array
pub mod ffi_str;

/// A wrapper for an opaque owned pointer.
pub mod owned_opaque_ptr;

/// Helper functions for working with raw pointers.
pub mod ptr;

/// Macro for converting SdvResult to an sdv_status_t for passing into the C ABI layer.
pub mod try_status;
