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

//! This file parses and validates the Instance Name.

use thiserror::Error;

use super::alphanumhyphen_validation::{validate_alphanumhyphen, AlphanumHyphenValidationError};

/// Maximum number of the Instance Name in bytes.
///
/// Decrease by 1 since Rust strings do not have a NUL terminator at the end compare to C strings.
pub const INSTANCE_NAME_MAX_LENGTH_BYTES: usize = 48 - 1;

/// Error that can arise during Instance Name validation.
#[derive(Error, Debug, PartialEq)]
#[error("{0}")]
pub struct InstanceNameValidationError(#[from] pub AlphanumHyphenValidationError);

/// Validates whether `s` is a valid Instance Name.
pub fn validate_instance_name(s: &str) -> Result<(), InstanceNameValidationError> {
    validate_alphanumhyphen(s, INSTANCE_NAME_MAX_LENGTH_BYTES)
        .map_err(InstanceNameValidationError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_name_is_valid() {
        assert_eq!(validate_instance_name("instance"), Ok(()));
        assert_eq!(validate_instance_name("abcdefghijklmnopqrstuvwxyz"), Ok(()));
        assert_eq!(validate_instance_name("instance-0-1-2-3-4-5-6-7-8-9"), Ok(()));
        assert_eq!(
            validate_instance_name("instance-has-47-symbols-90123456789012345678901"),
            Ok(())
        );
    }

    #[test]
    fn test_instance_name_exceeds_max_length() {
        let result = validate_instance_name("instance-has-48-symbols-90123456789012345678901x");
        let alphanum_err = AlphanumHyphenValidationError::ExceedsMaxLength(47, 48);
        assert_eq!(result, Err(InstanceNameValidationError(alphanum_err)));
        // Verify that error formatting is correct.
        assert_eq!(
            result.unwrap_err().to_string(),
            "should not be longer than 47 bytes, found 48 bytes"
        );
    }
}
