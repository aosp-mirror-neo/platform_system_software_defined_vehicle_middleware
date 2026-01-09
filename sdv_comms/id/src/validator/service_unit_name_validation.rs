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

//! This file parses and validates the Unit Name.

use thiserror::Error;

use super::alphanumhyphen_validation::{
    ends_with_valid_character, has_no_consecutive_hyphens, has_valid_char_only, is_not_empty,
    is_within_max_length, starts_with_valid_character, validate_alphanumhyphen,
    AlphanumHyphenValidationError,
};

/// Maximum length of the Service Unit Name in bytes.
///
/// Decrease by 1 since Rust strings do not have a NUL terminator at the end compare to C strings.
pub const SERVICE_UNIT_NAME_MAX_LENGTH_BYTES: usize = 128 - 1;

/// Error that can arise during Service Unit Name validation.
#[derive(Error, Debug, PartialEq)]
#[error("{0}")]
pub struct ServiceUnitNameValidationError(#[from] pub AlphanumHyphenValidationError);

/// validates that s is not empty
pub fn validate_non_empty(s: &str) -> Result<(), ServiceUnitNameValidationError> {
    is_not_empty(s).map_err(ServiceUnitNameValidationError::from)
}

/// validates if length of `s` is within limit
pub fn validate_length(s: &str) -> Result<(), ServiceUnitNameValidationError> {
    is_within_max_length(s, SERVICE_UNIT_NAME_MAX_LENGTH_BYTES)
        .map_err(ServiceUnitNameValidationError::from)
}

/// validates if s contains only valid character
pub fn validate_valid_chars(s: &str) -> Result<(), ServiceUnitNameValidationError> {
    has_valid_char_only(s).map_err(ServiceUnitNameValidationError::from)
}

/// validate if s starts with valid character
pub fn validate_first_char(s: &str) -> Result<(), ServiceUnitNameValidationError> {
    starts_with_valid_character(s).map_err(ServiceUnitNameValidationError::from)
}

/// validate if s ends with valid character
pub fn validate_last_char(s: &str) -> Result<(), ServiceUnitNameValidationError> {
    ends_with_valid_character(s).map_err(ServiceUnitNameValidationError::from)
}

/// validates that s has no consecutive hyphens
pub fn validate_consecutive_hyphens(s: &str) -> Result<(), ServiceUnitNameValidationError> {
    has_no_consecutive_hyphens(s).map_err(ServiceUnitNameValidationError::from)
}

/// validates whether `s` is a valid Service Unit Name.
/// this function validates every constraints for a valid name
pub fn validate_service_unit_name(s: &str) -> Result<(), ServiceUnitNameValidationError> {
    validate_alphanumhyphen(s, SERVICE_UNIT_NAME_MAX_LENGTH_BYTES)
        .map_err(ServiceUnitNameValidationError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_unit_name_is_valid() {
        assert_eq!(validate_service_unit_name("unit-name"), Ok(()));
        assert_eq!(validate_service_unit_name("abcdefghijklmnopqrstuvwxyz"), Ok(()));
        assert_eq!(validate_service_unit_name("unit-name-0-1-2-3-4-5-6-7-8-9"), Ok(()));
        assert_eq!(
            validate_service_unit_name("unit-name-has-127-symbols-78901234567890123401234567890123456789012345678901234012345678901234567890123456789012340123456789012"),
            Ok(())
        );
    }

    #[test]
    fn test_exceed_max_length_ok() {
        assert_eq!(validate_length("unit-name"), Ok(()));
        assert_eq!(validate_length("abcdefghijklmnopqrstuvwxyz"), Ok(()));
        assert_eq!(validate_length("unit-name-0-1-2-3-4-5-6-7-8-9"), Ok(()));
        assert_eq!(
            validate_length("unit-name-has-127-symbols-78901234567890123401234567890123456789012345678901234012345678901234567890123456789012340123456789012"),
            Ok(())
        );
    }

    #[test]
    fn test_validate_non_empty() {
        let alphanum_err = AlphanumHyphenValidationError::IsEmpty;
        assert_eq!(validate_non_empty("unit-name"), Ok(()));
        assert_eq!(validate_non_empty(""), Err(ServiceUnitNameValidationError(alphanum_err)));
    }

    #[test]
    fn test_exceed_max_length_error() {
        let result = validate_length(
            "unit-name-has-128-symbols-78901234567890123401234567890123456789012345678901234012345678901234567890123456789012340123456789012x",
        );
        let alphanum_err = AlphanumHyphenValidationError::ExceedsMaxLength(127, 128);

        assert_eq!(result, Err(ServiceUnitNameValidationError(alphanum_err)));

        assert_eq!(
            result.unwrap_err().to_string(),
            "should not be longer than 127 bytes, found 128 bytes"
        );
    }

    #[test]
    fn test_validate_valid_chars() {
        let alphanum_err = AlphanumHyphenValidationError::HasInvalidCharacter('!');
        assert_eq!(validate_valid_chars("abcdefghijklmnopqrstuvwxyz"), Ok(()));
        assert_eq!(validate_valid_chars("unit-name-0-1-2-3-4-5-6-7-8-9"), Ok(()));
        assert_eq!(
            validate_valid_chars("unit-name!5"),
            Err(ServiceUnitNameValidationError(alphanum_err))
        );
    }

    #[test]
    fn test_validate_first_char() {
        let alphanum_err = AlphanumHyphenValidationError::StartsWithInvalidCharacter('5');
        assert_eq!(validate_first_char("abcdefghijklmnopqrstuvwxyz"), Ok(()));
        assert_eq!(validate_first_char("5abc"), Err(ServiceUnitNameValidationError(alphanum_err)));
    }

    #[test]
    fn test_validate_last_char() {
        let alphanum_err = AlphanumHyphenValidationError::EndsWithInvalidCharacter('-');
        assert_eq!(validate_last_char("abcdefghijklmnopqrstuvwxyz"), Ok(()));
        assert_eq!(validate_last_char("abcde-"), Err(ServiceUnitNameValidationError(alphanum_err)));
    }

    #[test]
    fn test_validate_consecutive_hyphens() {
        let alphanum_err = AlphanumHyphenValidationError::ContainsConsecutiveHyphens;
        assert_eq!(validate_consecutive_hyphens("abcdefghijklmnopqrstuvwxyz"), Ok(()));
        assert_eq!(validate_consecutive_hyphens("unit-name-5"), Ok(()));
        assert_eq!(
            validate_consecutive_hyphens("unit-name--"),
            Err(ServiceUnitNameValidationError(alphanum_err))
        );
    }
}
