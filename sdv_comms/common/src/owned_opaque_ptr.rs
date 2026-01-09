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

use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::ptr::NonNull;

use crate::callback::sdk::OnceCallbackReceivable;

/// A wrapper for an opaque owned pointer.
#[repr(transparent)]
pub struct OwnedOpaquePtr<T>(NonNull<T>);

impl<T> OwnedOpaquePtr<T> {
    /// Creates a new `OwnedOpaquePtr` from a non-null pointer.
    ///
    /// The caller must ensure that the pointer is owned and not shared.
    pub fn new(ptr: NonNull<T>) -> Self {
        Self(ptr)
    }

    /// Returns a raw constant pointer to the underlying data.
    ///
    /// The caller must ensure that the pointer is not used after the `OwnedOpaquePtr` is dropped.
    pub fn as_ptr(&self) -> *const T {
        self.0.as_ptr()
    }

    /// Returns a raw mulable pointer to the underlying data.
    ///
    /// The caller must ensure that the pointer is not used after the `OwnedOpaquePtr` is dropped.
    pub fn as_mut_ptr(&self) -> *mut T {
        self.0.as_ptr()
    }

    /// Consumes the `OwnedOpaquePtr` and returns the wrapped raw pointer.
    pub fn into_ptr(self) -> *mut T {
        self.0.as_ptr()
    }
}

// SAFETY:
// It is safe to transfer ownership of a `*mut T` to a `NonNull<T>`.
unsafe impl<T> OnceCallbackReceivable for OwnedOpaquePtr<T> {
    type FfiType = *mut T;

    fn from_ffi_value(val: Self::FfiType) -> SdvResult<Self> {
        let ptr = NonNull::new(val).ok_or(SdvStatus::new(SdvStatusCode::DataLoss))?;
        Ok(Self::new(ptr))
    }
}

// SAFETY:
// OwnedOpaquePtr owns the pointer.
unsafe impl<T> Send for OwnedOpaquePtr<T> {}
