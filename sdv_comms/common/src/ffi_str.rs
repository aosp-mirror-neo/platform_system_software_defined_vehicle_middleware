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
#![allow(dead_code)]
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::ffi::{c_char, CStr};

/// Copies `&str` into a NUL-terminated ASCII encoded byte array.
pub fn str_to_bytes(in_str: &str, out_bytes: &mut [u8]) -> SdvResult<()> {
    if !in_str.is_ascii() {
        return Err(SdvStatus::with_message(
            SdvStatusCode::InvalidArgument,
            format!("Invalid string {in_str}: containing non ASCII characters"),
        ));
    }

    // Take into account the NUL (\0) character.
    if in_str.len() + 1 > out_bytes.len() {
        return Err(SdvStatus::with_message(
            SdvStatusCode::InvalidArgument,
            format!("Invalid input {in_str}: input string is too long for output buffer"),
        ));
    }

    out_bytes[..in_str.len()].copy_from_slice(in_str.as_bytes());
    out_bytes[in_str.len()] = b'\0';

    Ok(())
}

/// Converts the provided NUL-terminated ASCII C string into a `&str`.
pub fn bytes_to_str(bytes: &[u8]) -> SdvResult<&str> {
    let c_str = CStr::from_bytes_until_nul(bytes).map_err(|_| {
        SdvStatus::with_message(
            SdvStatusCode::InvalidArgument,
            format!(
                "Byte array {bytes:?} cannot be converted to a C string: NUL-terminated byte not found"
            ),
        )
    })?;
    c_str.to_str().map_err(|_| {
        SdvStatus::with_message(
            SdvStatusCode::InvalidArgument,
            format!("Byte array {bytes:?} cannot be converted to a C string: containing non UTF-8 characters"),
        )
    })
}

/// Converts the provided pointer to NUL-terminated ASCII C string into a `&str`.
///
/// # Safety
///  - `ptr` must point at a NUL-terminated string.
///  - `ptr` must abide by the safety requirements of `CStr::from_ptr`.
pub unsafe fn cstr_pointer_to_str(ptr: *const c_char) -> SdvResult<String> {
    // SAFETY: safe according to the safety requirements of this function.
    let c_str = unsafe { CStr::from_ptr(ptr) };
    c_str.to_str().map(|str| str.to_string()).map_err(|_| {
        SdvStatus::with_message(
            SdvStatusCode::InvalidArgument,
            format!("Invalid C string {c_str:?}: containing non UTF-8 string"),
        )
    })
}
