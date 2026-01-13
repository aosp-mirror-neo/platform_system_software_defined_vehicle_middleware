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

/// Ensures that the provided pointer is not null, and returns it as a ref.
///
/// Returns [SdvStatus] with [SdvStatusCode::IllegalArgument] if the pointer is null.
///
/// # Safety
/// `ptr` must be valid for use as a reference of type [T] or a nullptr.
#[inline]
pub unsafe fn check_not_null<'a, T: Sized>(ptr: *const T) -> SdvResult<&'a T> {
    // SAFETY: The pointer is expected to b eproperly aligned and deferencable to an instance of [T].
    let opt_ref = unsafe { ptr.as_ref() };
    opt_ref.ok_or(SdvStatus::with_message(SdvStatusCode::InvalidArgument, "Null pointer"))
}

/// Ensures that the provided pointer is not null, and returns it as a mutable ref.
///
/// Returns [SdvStatus] with [SdvStatusCode::IllegalArgument] if the pointer is null.
///
/// # Safety
/// `ptr` must be valid for use as a mutable reference of type [T] or a nullptr.
#[inline]
pub unsafe fn check_not_null_mut<'a, T: Sized>(ptr: *mut T) -> SdvResult<&'a mut T> {
    // SAFETY: The pointer is expected to b eproperly aligned and deferencable to an instance of [T].
    let opt_ref = unsafe { ptr.as_mut() };
    opt_ref.ok_or(SdvStatus::with_message(SdvStatusCode::InvalidArgument, "Null pointer"))
}
