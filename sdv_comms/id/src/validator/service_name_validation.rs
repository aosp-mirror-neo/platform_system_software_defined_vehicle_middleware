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

//! This file parses and validates the Service Name.

use thiserror::Error;

/// Max length of the Service Name in bytes.
///
/// Decrease by 1 since Rust strings do not have a NUL terminator at the end compare to C strings.
pub const SERVICE_NAME_MAX_LENGTH_BYTES: usize = 48 - 1;

/// Error that can arise during Service Name validation.
#[derive(Error, Debug, PartialEq)]
pub enum ServiceNameValidationError {
    /// Indicates that Service Name is empty.
    #[error("should not be empty")]
    IsEmpty,
    /// Indicates that Service Name exceeds max length in bytes.
    #[error("should not be longer than {0} bytes, found {1} bytes")]
    ExceedsMaxLength(usize, usize),
    /// Indicates that Service Name contains invalid character.
    #[error("should consist of [A-Z, a-z, 0-9, _] characters, found '{0}'")]
    HasInvalidCharacter(char),
    /// Indicates that Service Name starts with invalid character.
    #[error("should start with [A-Z] character, found '{0}'")]
    StartsWithInvalidCharacter(char),
    /// Indicates that Service Name ends with invalid character.
    #[error("should end with [A-Z, a-z, 0-9] character, found '{0}'")]
    EndsWithInvalidCharacter(char),
    /// Indicates that Service Name contains consecutive underscores.
    #[error("should not contain consecutive underscores (__)")]
    ContainsConsecutiveUnderscores,
}

/// Validates whether `s` is a valid Service Name.
pub fn validate_service_name(s: &str) -> Result<(), ServiceNameValidationError> {
    if let Some(first_char) = s.chars().next() {
        if !first_char.is_ascii_uppercase() {
            return Err(ServiceNameValidationError::StartsWithInvalidCharacter(first_char));
        }
    } else {
        return Err(ServiceNameValidationError::IsEmpty);
    }

    if let Some(last_char) = s.chars().next_back() {
        if !last_char.is_ascii_alphanumeric() {
            return Err(ServiceNameValidationError::EndsWithInvalidCharacter(last_char));
        }
    } else {
        return Err(ServiceNameValidationError::IsEmpty);
    }

    if s.len() > SERVICE_NAME_MAX_LENGTH_BYTES {
        return Err(ServiceNameValidationError::ExceedsMaxLength(
            SERVICE_NAME_MAX_LENGTH_BYTES,
            s.len(),
        ));
    }

    let mut is_prev_underscore = false;
    for ch in s.chars() {
        const UNDERSCORE: char = '_';
        if !ch.is_ascii_alphanumeric() && ch != UNDERSCORE {
            return Err(ServiceNameValidationError::HasInvalidCharacter(ch));
        }
        if ch == UNDERSCORE && is_prev_underscore {
            return Err(ServiceNameValidationError::ContainsConsecutiveUnderscores);
        }
        is_prev_underscore = ch == UNDERSCORE;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_name_is_valid() {
        assert_eq!(validate_service_name("SomeService"), Ok(()));
        assert_eq!(validate_service_name("WindowMonitor777"), Ok(()));
        assert_eq!(validate_service_name("Window_Monitor_777"), Ok(()));
        assert_eq!(validate_service_name("Win_dow_Moni_tor_7_7_7"), Ok(()));
        assert_eq!(
            validate_service_name("ServiceWith_47_Symbols0123456789012345678901234"),
            Ok(())
        );
    }

    #[test]
    fn test_service_name_is_empty() {
        assert_eq!(validate_service_name(""), Err(ServiceNameValidationError::IsEmpty));
    }

    #[test]
    fn test_service_name_exceeds_max_length() {
        let result = validate_service_name("ServiceWith_48_Symbols0123456789012345678901234x");
        assert_eq!(result, Err(ServiceNameValidationError::ExceedsMaxLength(47, 48)));
        // Verify that error formatting is correct.
        assert_eq!(
            result.unwrap_err().to_string(),
            "should not be longer than 47 bytes, found 48 bytes"
        );
    }

    #[test]
    fn test_service_name_has_invalid_character() {
        for ch in ['!', '@', '*', '/', ':', '\\', '.', ';', '[', ']', '(', ')', 'ї', 'ü'] {
            let result = validate_service_name(&format!("ServiceNameWith{ch}InvalidChar"));
            assert_eq!(result, Err(ServiceNameValidationError::HasInvalidCharacter(ch)));
            // Verify that error formatting is correct.
            assert_eq!(
                result.unwrap_err().to_string(),
                format!("should consist of [A-Z, a-z, 0-9, _] characters, found '{ch}'")
            );
        }
    }

    #[test]
    fn test_service_name_starts_with_invalid_character() {
        for ch in [
            '_', '0', '8', 'a', 'l', 'z', '!', '@', '*', '/', ':', '\\', '.', ';', '[', ']', '(',
            ')', 'ї', 'ü',
        ] {
            let result = validate_service_name(&format!("{ch}StartWithInvalidChar"));
            assert_eq!(result, Err(ServiceNameValidationError::StartsWithInvalidCharacter(ch)));
            // Verify that error formatting is correct.
            assert_eq!(
                result.unwrap_err().to_string(),
                format!("should start with [A-Z] character, found '{ch}'")
            );
        }
    }

    #[test]
    fn test_service_name_starts_with_2_underscores() {
        assert_eq!(
            validate_service_name("__HasTwoUnderscores"),
            Err(ServiceNameValidationError::StartsWithInvalidCharacter('_'))
        );
    }

    #[test]
    fn test_service_name_ends_with_invalid_character() {
        for ch in ['_', '!', '@', '*', '/', ':', '\\', '.', ';', '[', ']', '(', ')', 'ї', 'ü'] {
            let result = validate_service_name(&format!("EndWithInvalidChar{ch}"));
            assert_eq!(result, Err(ServiceNameValidationError::EndsWithInvalidCharacter(ch)));
            // Verify that error formatting is correct.
            assert_eq!(
                result.unwrap_err().to_string(),
                format!("should end with [A-Z, a-z, 0-9] character, found '{ch}'")
            );
        }
    }

    #[test]
    fn test_service_name_ends_with_2_underscores() {
        assert_eq!(
            validate_service_name("HasTwoUnderscores__"),
            Err(ServiceNameValidationError::EndsWithInvalidCharacter('_'))
        );
    }

    #[test]
    fn test_service_name_has_2_consecutive_underscores() {
        assert_eq!(
            validate_service_name("HasTwo__Underscores"),
            Err(ServiceNameValidationError::ContainsConsecutiveUnderscores)
        );
        assert_eq!(
            validate_service_name("Has__TwoUnderscores"),
            Err(ServiceNameValidationError::ContainsConsecutiveUnderscores)
        );
        assert_eq!(
            validate_service_name("HasTwoUnders__cores"),
            Err(ServiceNameValidationError::ContainsConsecutiveUnderscores)
        );
    }
}
