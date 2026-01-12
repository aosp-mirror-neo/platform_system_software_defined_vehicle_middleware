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

//! This file parses and validates the alpha-numeric-with-hyphens name.

use thiserror::Error;

/// Error that can arise during alpha-numeric-with-hyphens name validation.
#[derive(Error, Debug, PartialEq)]
pub enum AlphanumHyphenValidationError {
    /// Indicates that name is empty.
    #[error("should not be empty")]
    IsEmpty,
    /// Indicates that name exceeds max length in bytes.
    #[error("should not be longer than {0} bytes, found {1} bytes")]
    ExceedsMaxLength(usize, usize),
    /// Indicates that name contains invalid character.
    #[error("should consist of [a-z, 0-9, -] characters, found '{0}'")]
    HasInvalidCharacter(char),
    /// Indicates that name starts with invalid character.
    #[error("should start with [a-z] character, found '{0}'")]
    StartsWithInvalidCharacter(char),
    /// Indicates that name ends with invalida character.
    #[error("should end with [a-z, 0-9] character, found '{0}'")]
    EndsWithInvalidCharacter(char),
    /// Indicates that name contains consecutive hyphens.
    #[error("should not contain consecutive hyphens (--)")]
    ContainsConsecutiveHyphens,
}

/// validates if s is empty
pub fn is_not_empty(s: &str) -> Result<(), AlphanumHyphenValidationError> {
    if s.is_empty() {
        return Err(AlphanumHyphenValidationError::IsEmpty);
    }
    Ok(())
}

/// validates if length of `s` is within limit
pub fn is_within_max_length(
    s: &str,
    max_length: usize,
) -> Result<(), AlphanumHyphenValidationError> {
    if s.len() > max_length {
        return Err(AlphanumHyphenValidationError::ExceedsMaxLength(max_length, s.len()));
    }
    Ok(())
}

/// validates if s contains only valid character
pub fn has_valid_char_only(s: &str) -> Result<(), AlphanumHyphenValidationError> {
    for ch in s.chars() {
        if !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && ch != '-' {
            return Err(AlphanumHyphenValidationError::HasInvalidCharacter(ch));
        }
    }
    Ok(())
}

/// validates if s starts with valid character
pub fn starts_with_valid_character(s: &str) -> Result<(), AlphanumHyphenValidationError> {
    if let Some(first_char) = s.chars().next() {
        if !first_char.is_ascii_lowercase() {
            return Err(AlphanumHyphenValidationError::StartsWithInvalidCharacter(first_char));
        }
    }
    Ok(())
}

/// validates if s ends with valid character
pub fn ends_with_valid_character(s: &str) -> Result<(), AlphanumHyphenValidationError> {
    if let Some(last_char) = s.chars().next_back() {
        if !last_char.is_ascii_lowercase() && !last_char.is_ascii_digit() {
            return Err(AlphanumHyphenValidationError::EndsWithInvalidCharacter(last_char));
        }
    }
    Ok(())
}

/// validates that s does not contain consecutive hyphens
pub fn has_no_consecutive_hyphens(s: &str) -> Result<(), AlphanumHyphenValidationError> {
    let mut is_prev_hyphen = false;
    for ch in s.chars() {
        const HYPHEN: char = '-';
        if ch == HYPHEN && is_prev_hyphen {
            return Err(AlphanumHyphenValidationError::ContainsConsecutiveHyphens);
        }
        is_prev_hyphen = ch == HYPHEN;
    }
    Ok(())
}

/// Validates whether `s` is a valid name.
pub fn validate_alphanumhyphen(
    s: &str,
    max_length: usize,
) -> Result<(), AlphanumHyphenValidationError> {
    is_not_empty(s)?;
    starts_with_valid_character(s)?;
    ends_with_valid_character(s)?;
    is_within_max_length(s, max_length)?;
    has_valid_char_only(s)?;
    has_no_consecutive_hyphens(s)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_LENGTH: usize = 40;

    #[test]
    fn test_is_valid() {
        assert_eq!(validate_alphanumhyphen("alpha-num-hyphen", MAX_LENGTH), Ok(()));
        assert_eq!(validate_alphanumhyphen("abcdefghijklmnopqrstuvwxyz", MAX_LENGTH), Ok(()));
        assert_eq!(
            validate_alphanumhyphen("alpha-num-hyphen-0-1-2-3-4-5-6-7-8-9", MAX_LENGTH),
            Ok(())
        );
        assert_eq!(
            validate_alphanumhyphen("alpha-num-hyphen-has-40-symbols-90123456", MAX_LENGTH),
            Ok(())
        );
    }

    #[test]
    fn test_is_empty() {
        assert_eq!(
            validate_alphanumhyphen("", MAX_LENGTH),
            Err(AlphanumHyphenValidationError::IsEmpty)
        );
        assert_eq!(starts_with_valid_character(""), Ok(()));
        assert_eq!(ends_with_valid_character(""), Ok(()));
    }

    #[test]
    fn test_exceeds_max_length() {
        let result =
            validate_alphanumhyphen("alpha-num-hyphen-has-41-symbols-90123456x", MAX_LENGTH);
        assert_eq!(result, Err(AlphanumHyphenValidationError::ExceedsMaxLength(40, 41)));
        // Verify that error formatting is correct.
        assert_eq!(
            result.unwrap_err().to_string(),
            "should not be longer than 40 bytes, found 41 bytes"
        );
    }

    #[test]
    fn test_has_invalid_character() {
        for ch in ['A', 'L', 'X', '!', '@', '*', '/', ':', '\\', '.', ';', '[', ']', '(', ')'] {
            let result = validate_alphanumhyphen(
                &format!("alpha-num-hyphen-has-{ch}-invalid-char"),
                MAX_LENGTH,
            );
            assert_eq!(result, Err(AlphanumHyphenValidationError::HasInvalidCharacter(ch)));
            // Verify that error formatting is correct.
            assert_eq!(
                result.unwrap_err().to_string(),
                format!("should consist of [a-z, 0-9, -] characters, found '{ch}'")
            );
        }
    }

    #[test]
    fn test_has_non_ascii_character() {
        let result = validate_alphanumhyphen("alpha-num-hyphen-ї-has-non-ascii", MAX_LENGTH);
        assert_eq!(result, Err(AlphanumHyphenValidationError::HasInvalidCharacter('ї')));
        // Verify that error formatting is correct.
        assert_eq!(
            result.unwrap_err().to_string(),
            "should consist of [a-z, 0-9, -] characters, found 'ї'"
        );
    }

    #[test]
    fn test_starts_with_hyphen() {
        let result = validate_alphanumhyphen("-alpha-num-hyphen", MAX_LENGTH);
        assert_eq!(result, Err(AlphanumHyphenValidationError::StartsWithInvalidCharacter('-')));
        // Verify that error formatting is correct.
        assert_eq!(result.unwrap_err().to_string(), "should start with [a-z] character, found '-'");
    }

    #[test]
    fn test_starts_with_digit() {
        let result = validate_alphanumhyphen("0-alpha-num-hyphen", MAX_LENGTH);
        assert_eq!(result, Err(AlphanumHyphenValidationError::StartsWithInvalidCharacter('0')));
        // Verify that error formatting is correct.
        assert_eq!(result.unwrap_err().to_string(), "should start with [a-z] character, found '0'");
    }

    #[test]
    fn test_ends_with_hyphen() {
        let result = validate_alphanumhyphen("alpha-num-hyphen-", MAX_LENGTH);
        assert_eq!(result, Err(AlphanumHyphenValidationError::EndsWithInvalidCharacter('-')));
        // Verify that error formatting is correct.
        assert_eq!(
            result.unwrap_err().to_string(),
            "should end with [a-z, 0-9] character, found '-'"
        );
    }

    #[test]
    fn test_has_2_consecutive_hyphens() {
        assert_eq!(
            validate_alphanumhyphen("alpha-num-hyphen--123", MAX_LENGTH),
            Err(AlphanumHyphenValidationError::ContainsConsecutiveHyphens)
        );
        assert_eq!(
            validate_alphanumhyphen("alpha-num-hyphen-xxx---xxx", MAX_LENGTH),
            Err(AlphanumHyphenValidationError::ContainsConsecutiveHyphens)
        );
    }
}
