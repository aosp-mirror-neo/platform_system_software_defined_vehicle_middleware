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

//! This file parses and validates the VM Name.

use thiserror::Error;

use super::alphanumhyphen_validation::{validate_alphanumhyphen, AlphanumHyphenValidationError};

/// Maximum length of the VM Name in bytes.
///
/// Decrease by 1 since Rust strings do not have a NUL terminator at the end compare to C strings.
pub const VM_NAME_MAX_LENGTH_BYTES: usize = 32 - 1;

/// Error that can arise during VM Name validation.
#[derive(Error, Debug, PartialEq)]
#[error("{0}")]
pub struct VmNameValidationError(#[from] pub AlphanumHyphenValidationError);

/// Validates whether `s` is a valid VM Name.
pub fn validate_vm_name(s: &str) -> Result<(), VmNameValidationError> {
    validate_alphanumhyphen(s, VM_NAME_MAX_LENGTH_BYTES).map_err(VmNameValidationError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_name_is_valid() {
        assert_eq!(validate_vm_name("vm"), Ok(()));
        assert_eq!(validate_vm_name("abcdefghijklmnopqrstuvwxyz"), Ok(()));
        assert_eq!(validate_vm_name("vm-0-1-2-3-4-5-6-7-8-9"), Ok(()));
        assert_eq!(validate_vm_name("vm-has-31-symbols-9012345678901"), Ok(()));
    }

    #[test]
    fn test_vm_name_exceeds_max_length() {
        let result = validate_vm_name("vm-has-32-symbols-9012345678901x");
        let alphanum_err = AlphanumHyphenValidationError::ExceedsMaxLength(31, 32);

        assert_eq!(result, Err(VmNameValidationError(alphanum_err)));
        // Verify that error formatting is correct.
        assert_eq!(
            result.unwrap_err().to_string(),
            "should not be longer than 31 bytes, found 32 bytes"
        );
    }
}
