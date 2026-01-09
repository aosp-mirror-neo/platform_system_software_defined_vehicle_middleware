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

//! This file parses and validates the UnitTypeName.

use thiserror::Error;

/// The maximum number bytes in Unit Type Name.
///
/// Decrease by 1 since Rust strings do not have a NUL terminator at the end comparing to C strings.
pub const UNIT_TYPE_NAME_MAX_LENGTH_BYTES: usize = 96 - 1;

/// Error that can arise during Unit Type Name validation.
#[derive(Error, Debug, PartialEq)]
pub enum UnitTypeNameValidationError {
    /// Indicates that either Subpackage or Type Name is empty.
    #[error("Subpackage or type name should not be empty")]
    IsEmpty,
    /// Indicates that Unit Type Name exceeds max length in bytes.
    #[error("should not be longer than {0} bytes, found {1} bytes")]
    ExceedsMaxLength(usize, usize),
    /// Indicates that Unit Type Name contains an invalid character.
    #[error("should consist of [a-z, 0-9, _] characters, found '{0}'")]
    SubpackageNameHasInvalidCharacter(char),
    /// Indicates that Unit Type Name contains an invalid character.
    #[error("should consist of [A-Z, a-z, 0-9, _] characters, found '{0}'")]
    TypeNameHasInvalidCharacter(char),
    /// Indicates that Unit Type Name starts with an invalid character.
    #[error("should start with [a-z] character, found '{0}'")]
    SubpackageNameStartsWithInvalidCharacter(char),
    /// Indicates that Unit Type Name starts with an invalid character.
    #[error("should start with [A-Z] character, found '{0}'")]
    TypeNameStartsWithInvalidCharacter(char),
    /// Indicates that Unit Type Name ends with an invalid character.
    #[error("should end with [A-Z, a-z, 0-9] character, found '{0}'")]
    EndsWithInvalidCharacter(char),
    /// Indicates that Unit Type Name contains consecutive underscores.
    #[error("should not contain consecutive underscores (__)")]
    ContainsConsecutiveUnderscores,
}

/// Validates whether `s` is a valid Unit Type Name.
/// `s` should match the following pattern <subpackage>.<type_name>
pub fn validate_unit_type_name(s: &str) -> Result<(), UnitTypeNameValidationError> {
    if s.len() > UNIT_TYPE_NAME_MAX_LENGTH_BYTES {
        return Err(UnitTypeNameValidationError::ExceedsMaxLength(
            UNIT_TYPE_NAME_MAX_LENGTH_BYTES,
            s.len(),
        ));
    }

    let parts: Vec<&str> = s.split('.').collect();
    // Validate the last part as the type name
    validate_type_name(parts.last().unwrap())?;

    // Validate the remaining parts as subpackages
    for part in &parts[..parts.len() - 1] {
        validate_subpackage(part)?;
    }

    Ok(())
}

fn validate_type_name_string(s: &str) -> Result<(), UnitTypeNameValidationError> {
    let mut is_prev_underscore = false;
    const UNDERSCORE: char = '_';
    for ch in s.chars() {
        if !ch.is_ascii_alphanumeric() && ch != UNDERSCORE {
            return Err(UnitTypeNameValidationError::TypeNameHasInvalidCharacter(ch));
        }
        if ch == UNDERSCORE && is_prev_underscore {
            return Err(UnitTypeNameValidationError::ContainsConsecutiveUnderscores);
        }
        is_prev_underscore = ch == UNDERSCORE;
    }

    Ok(())
}

fn validate_subpackage_string(s: &str) -> Result<(), UnitTypeNameValidationError> {
    let mut is_prev_underscore = false;
    const UNDERSCORE: char = '_';
    for ch in s.chars() {
        if ch.is_ascii_uppercase() || (!ch.is_ascii_alphanumeric() && ch != '_') {
            return Err(UnitTypeNameValidationError::SubpackageNameHasInvalidCharacter(ch));
        }
        if ch == UNDERSCORE && is_prev_underscore {
            return Err(UnitTypeNameValidationError::ContainsConsecutiveUnderscores);
        }
        is_prev_underscore = ch == UNDERSCORE;
    }

    Ok(())
}

fn validate_last_char(s: &str) -> Result<(), UnitTypeNameValidationError> {
    if let Some(last_char) = s.chars().next_back() {
        if !last_char.is_ascii_alphanumeric() {
            return Err(UnitTypeNameValidationError::EndsWithInvalidCharacter(last_char));
        }
    } else {
        return Err(UnitTypeNameValidationError::IsEmpty);
    }

    Ok(())
}

fn validate_subpackage(s: &str) -> Result<(), UnitTypeNameValidationError> {
    if let Some(first_char) = s.chars().next() {
        if !first_char.is_ascii_lowercase() {
            return Err(UnitTypeNameValidationError::SubpackageNameStartsWithInvalidCharacter(
                first_char,
            ));
        }
    } else {
        return Err(UnitTypeNameValidationError::IsEmpty);
    }

    validate_last_char(s)?;
    validate_subpackage_string(s)?;

    Ok(())
}

fn validate_type_name(s: &str) -> Result<(), UnitTypeNameValidationError> {
    if let Some(first_char) = s.chars().next() {
        if !first_char.is_ascii_uppercase() {
            return Err(UnitTypeNameValidationError::TypeNameStartsWithInvalidCharacter(
                first_char,
            ));
        }
    } else {
        return Err(UnitTypeNameValidationError::IsEmpty);
    }

    validate_last_char(s)?;
    validate_type_name_string(s)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const INVALID_LAST_CHARS: &[char] =
        &['_', '!', '@', '*', '/', ':', '\\', ';', '[', ']', '(', ')', 'ї', 'ü'];

    #[test]
    fn test_unit_type_name_is_valid() {
        assert_eq!(validate_unit_type_name("UnitType"), Ok(()));
        assert_eq!(validate_unit_type_name("WindowMonitor777"), Ok(()));
        assert_eq!(validate_unit_type_name("Window_Monitor_777"), Ok(()));
        assert_eq!(validate_unit_type_name("Win_dow_Moni_tor_7_7_7"), Ok(()));
        assert_eq!(validate_unit_type_name("Win_dow_Moni_tor_7_7_7"), Ok(()));
        assert_eq!(validate_unit_type_name("windows.monitor.Model7"), Ok(()));
        assert_eq!(validate_unit_type_name("windows.monitor.Model7_7"), Ok(()));
        assert_eq!(validate_unit_type_name("windows.hp.monitor.Model7"), Ok(()));
        assert_eq!(validate_unit_type_name("windows.Model7"), Ok(()));
        assert_eq!(
            validate_unit_type_name("UnitTypeWith47_Symbols0123456789012345678901234"),
            Ok(())
        );
    }

    #[test]
    fn test_unit_type_name_is_empty() {
        assert_eq!(validate_unit_type_name(""), Err(UnitTypeNameValidationError::IsEmpty));
    }

    #[test]
    fn test_subpackage_name_is_empty() {
        assert_eq!(validate_unit_type_name(".UnitType"), Err(UnitTypeNameValidationError::IsEmpty));
    }

    #[test]
    fn test_type_name_is_empty() {
        assert_eq!(
            validate_unit_type_name("subpackage."),
            Err(UnitTypeNameValidationError::IsEmpty)
        );
    }

    #[test]
    fn test_unit_type_name_exceeds_max_length() {
        let result = validate_unit_type_name("UnitTypeWith96_Symbols0123456789012345678901234000011110000111100001111000011110000111100001111x");
        assert_eq!(
            result,
            Err(UnitTypeNameValidationError::ExceedsMaxLength(
                UNIT_TYPE_NAME_MAX_LENGTH_BYTES,
                UNIT_TYPE_NAME_MAX_LENGTH_BYTES + 1
            ))
        );
        // TODO(sgurun): Verifying error string is unnecessary. This line should be removed. Keeping for consistency.
        assert_eq!(
            result.unwrap_err().to_string(),
            "should not be longer than 95 bytes, found 96 bytes"
        );
    }

    #[test]
    fn test_unit_type_name_has_invalid_character() {
        for ch in ['!', '@', '*', '/', ':', '\\', ';', '[', ']', '(', ')', 'ї', 'ü'] {
            let result = validate_unit_type_name(&format!("UnitTypeWith{ch}InvalidChar"));
            assert_eq!(result, Err(UnitTypeNameValidationError::TypeNameHasInvalidCharacter(ch)));
            // Verify that error formatting is correct.
            assert_eq!(
                result.unwrap_err().to_string(),
                format!("should consist of [A-Z, a-z, 0-9, _] characters, found '{ch}'")
            );
        }
    }

    #[test]
    fn test_subpackage_name_has_invalid_character() {
        for ch in ['!', '@', '*', '/', ':', '\\', ';', '[', ']', '(', ')', 'ї', 'ü', 'A', 'Z'] {
            let result =
                validate_unit_type_name(&format!("subpackagewith{ch}invalidchar.TypeName"));
            assert_eq!(
                result,
                Err(UnitTypeNameValidationError::SubpackageNameHasInvalidCharacter(ch))
            );
            // Verify that error formatting is correct.
            assert_eq!(
                result.unwrap_err().to_string(),
                format!("should consist of [a-z, 0-9, _] characters, found '{ch}'")
            );
        }
    }

    #[test]
    fn test_unit_type_name_starts_with_invalid_character() {
        for ch in [
            '_', '0', '8', 'a', 'l', 'z', '!', '@', '*', '/', ':', '\\', ';', '[', ']', '(', ')',
            'ї', 'ü',
        ] {
            let result = validate_unit_type_name(&format!("{ch}StartWithInvalidChar"));
            assert_eq!(
                result,
                Err(UnitTypeNameValidationError::TypeNameStartsWithInvalidCharacter(ch))
            );
            // Verify that error formatting is correct.
            assert_eq!(
                result.unwrap_err().to_string(),
                format!("should start with [A-Z] character, found '{ch}'")
            );
        }
    }

    #[test]
    fn test_subpackage_name_starts_with_invalid_character() {
        for ch in [
            '_', '0', '8', 'A', 'L', 'Z', '!', '@', '*', '/', ':', '\\', ';', '[', ']', '(', ')',
            'I',
        ] {
            let result = validate_unit_type_name(&format!("{ch}startwith.InvalidChar"));
            assert_eq!(
                result,
                Err(UnitTypeNameValidationError::SubpackageNameStartsWithInvalidCharacter(ch))
            );
            // Verify that error formatting is correct.
            assert_eq!(
                result.unwrap_err().to_string(),
                format!("should start with [a-z] character, found '{ch}'")
            );
        }
    }

    #[test]
    fn test_unit_type_name_starts_with_2_underscores() {
        assert_eq!(
            validate_unit_type_name("__HasTwoUnderscores"),
            Err(UnitTypeNameValidationError::TypeNameStartsWithInvalidCharacter('_'))
        );
    }

    #[test]
    fn test_subpackage_name_ends_with_invalid_character() {
        for ch in INVALID_LAST_CHARS {
            let result = validate_unit_type_name(&format!("endwithinvalidchar{ch}.UnitType"));
            assert_eq!(result, Err(UnitTypeNameValidationError::EndsWithInvalidCharacter(*ch)));
            // Verify that error formatting is correct.
            assert_eq!(
                result.unwrap_err().to_string(),
                format!("should end with [A-Z, a-z, 0-9] character, found '{ch}'")
            );
        }
    }

    #[test]
    fn test_unit_type_name_ends_with_invalid_character() {
        for ch in INVALID_LAST_CHARS {
            let result = validate_unit_type_name(&format!("EndWithInvalidChar{ch}"));
            assert_eq!(result, Err(UnitTypeNameValidationError::EndsWithInvalidCharacter(*ch)));
            // Verify that error formatting is correct.
            assert_eq!(
                result.unwrap_err().to_string(),
                format!("should end with [A-Z, a-z, 0-9] character, found '{ch}'")
            );
        }
    }

    #[test]
    fn test_subpackage_name_ends_with_2_underscores() {
        assert_eq!(
            validate_unit_type_name("hastwounderscores__.UnitType"),
            Err(UnitTypeNameValidationError::EndsWithInvalidCharacter('_'))
        );
    }

    #[test]
    fn test_subpackage_name_ends_with_2_dots() {
        assert_eq!(
            validate_unit_type_name("hastwodots..UnitType"),
            Err(UnitTypeNameValidationError::IsEmpty)
        );
    }

    #[test]
    fn test_unit_type_name_ends_with_2_underscores() {
        assert_eq!(
            validate_unit_type_name("HasTwoUnderscores__"),
            Err(UnitTypeNameValidationError::EndsWithInvalidCharacter('_'))
        );
    }

    #[test]
    fn test_subpackage_name_has_2_consecutive_underscores() {
        assert_eq!(
            validate_unit_type_name("hastwo__underscores.UnitType"),
            Err(UnitTypeNameValidationError::ContainsConsecutiveUnderscores)
        );
        assert_eq!(
            validate_unit_type_name("has__twounderscores.UnitType"),
            Err(UnitTypeNameValidationError::ContainsConsecutiveUnderscores)
        );
        assert_eq!(
            validate_unit_type_name("hastwounders__cores.UnitType"),
            Err(UnitTypeNameValidationError::ContainsConsecutiveUnderscores)
        );
    }

    #[test]
    fn test_unit_type_name_has_2_consecutive_underscores() {
        assert_eq!(
            validate_unit_type_name("HasTwo__Underscores"),
            Err(UnitTypeNameValidationError::ContainsConsecutiveUnderscores)
        );
        assert_eq!(
            validate_unit_type_name("Has__TwoUnderscores"),
            Err(UnitTypeNameValidationError::ContainsConsecutiveUnderscores)
        );
        assert_eq!(
            validate_unit_type_name("HasTwoUnders__cores"),
            Err(UnitTypeNameValidationError::ContainsConsecutiveUnderscores)
        );
    }
}
