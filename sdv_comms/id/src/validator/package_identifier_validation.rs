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

//! This file parses and validates the Package Identifier.

use thiserror::Error;

/// Maximum length of the Package Identifier in bytes.
///
/// Decrease by 1 since Rust strings do not have a NUL terminator at the end compare to C strings.
pub const PACKAGE_IDENTIFIER_MAX_LENGTH_BYTES: usize = 128 - 1;

/// Error that can arise during Package Name validation.
#[derive(Error, Debug, PartialEq)]
#[error("{0}")]
pub struct PackageNameValidationError(#[from] pub PackageIdentifierValidationError);

/// Validates whether `s` is a valid Package Name.
pub fn validate_package_name(s: &str) -> Result<(), PackageNameValidationError> {
    const DISALLOW_SINGLE_SEGMENT: bool = false;
    validate_package_identifier(s, DISALLOW_SINGLE_SEGMENT)
        .map_err(PackageNameValidationError::from)
}

/// Error that can arise during Package Namespace validation.
#[derive(Error, Debug, PartialEq)]
#[error("{0}")]
pub struct PackageNamespaceValidationError(#[from] pub PackageIdentifierValidationError);

/// Validates whether `s` is a valid Package Namespace.
pub fn validate_package_namespace(s: &str) -> Result<(), PackageNamespaceValidationError> {
    const ALLOW_SINGLE_SEGMENT: bool = true;
    validate_package_identifier(s, ALLOW_SINGLE_SEGMENT)
        .map_err(PackageNamespaceValidationError::from)
}

/// Error that can arise during Package Identifier validation.
#[derive(Error, Debug, PartialEq)]
pub enum PackageIdentifierValidationError {
    /// Indicates that Package Identifier is empty.
    #[error("should not be empty")]
    IsEmpty,
    /// Indicates that Package Identifier exceeds max length in bytes.
    #[error("should not be longer than {0} bytes, found {1} bytes")]
    ExceedsMaxLength(usize, usize),
    /// Indicates that Package Identifier contains invalid character.
    #[error("should consist of [a-z, 0-9, _, .] characters, found '{0}'")]
    HasInvalidCharacter(char),
    /// Indicates that Package Identifier ends with invalid character.
    #[error("should end with [a-z, 0-9] character, found '{0}'")]
    EndsWithInvalidCharacter(char),
    /// Indicates that Package Identifier has a segment which starts with invalid character.
    #[error("segment should start with [a-z] character, found '{0}'")]
    SegmentStartsWithInvalidCharacter(char),
    /// Indicates that Package Identifier ends with invalid character.
    #[error("segment should end with [a-z, 0-9] character, found '{0}'")]
    SegmentEndsWithInvalidCharacter(char),
    /// Indicates that Package Identifier has one segment.
    #[error("should contain at least 2 segments, found 1")]
    HasOneSegment,
    /// Indicates that Package Identifier contains consecutive underscores.
    #[error("should not contain consecutive underscores (__)")]
    ContainsConsecutiveUnderscores,
}

/// Validates whether `s` is a valid Package Identifier which could be
/// a Package Name or Package Namespace depending whether we allow single
/// segment in `s`.
fn validate_package_identifier(
    s: &str,
    allow_single_segment: bool,
) -> Result<(), PackageIdentifierValidationError> {
    if s.is_empty() {
        return Err(PackageIdentifierValidationError::IsEmpty);
    }

    if s.len() > PACKAGE_IDENTIFIER_MAX_LENGTH_BYTES {
        return Err(PackageIdentifierValidationError::ExceedsMaxLength(
            PACKAGE_IDENTIFIER_MAX_LENGTH_BYTES,
            s.len(),
        ));
    }

    const UNDERSCORE: char = '_';
    const DOT: char = '.';

    let mut segment_start = true;
    let mut is_prev_underscore = false;
    let mut has_dot = false;
    for ch in s.chars() {
        if !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && ch != UNDERSCORE && ch != DOT {
            return Err(PackageIdentifierValidationError::HasInvalidCharacter(ch));
        }
        if segment_start && !ch.is_ascii_lowercase() {
            return Err(PackageIdentifierValidationError::SegmentStartsWithInvalidCharacter(ch));
        }
        if is_prev_underscore && ch == UNDERSCORE {
            return Err(PackageIdentifierValidationError::ContainsConsecutiveUnderscores);
        }
        if is_prev_underscore && ch == DOT {
            return Err(PackageIdentifierValidationError::SegmentEndsWithInvalidCharacter(
                UNDERSCORE,
            ));
        }

        segment_start = ch == DOT;
        is_prev_underscore = ch == UNDERSCORE;
        has_dot |= ch == DOT;
    }

    if !has_dot && !allow_single_segment {
        return Err(PackageIdentifierValidationError::HasOneSegment);
    }

    if segment_start {
        return Err(PackageIdentifierValidationError::EndsWithInvalidCharacter(DOT));
    }
    if is_prev_underscore {
        return Err(PackageIdentifierValidationError::SegmentEndsWithInvalidCharacter(UNDERSCORE));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_name_is_valid() {
        assert_eq!(validate_package_name("com.google"), Ok(()));
        assert_eq!(validate_package_name("com.google.sdv"), Ok(()));

        assert_eq!(validate_package_name("c_o_m.g_o_o_g_l_e.s_d_v"), Ok(()));
        assert_eq!(validate_package_name("c7777777o999m8888888.s111d22v33333333"), Ok(()));

        assert_eq!(validate_package_name("c_0_m.g_0_0_g_1_e.s_d_v"), Ok(()));

        assert_eq!(validate_package_name("com.google.sdv.with_127_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234"), Ok(()));
    }

    #[test]
    fn test_package_name_is_empty() {
        assert_eq!(
            validate_package_name(""),
            Err(PackageNameValidationError(PackageIdentifierValidationError::IsEmpty))
        );
    }

    #[test]
    fn test_package_name_exceeds_max_length() {
        let result = validate_package_name("com.google.sdv.with_128_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234x");
        assert_eq!(
            result,
            Err(PackageNameValidationError(PackageIdentifierValidationError::ExceedsMaxLength(
                127, 128
            )))
        );
        // Verify that error formatting is correct.
        assert_eq!(
            result.unwrap_err().to_string(),
            "should not be longer than 127 bytes, found 128 bytes"
        );
    }

    #[test]
    fn test_package_name_has_invalid_character() {
        for ch in ['A', 'L', 'X', '!', '@', '*', '/', ':', '\\', ';', '[', ']', '(', ')'] {
            assert_eq!(
                validate_package_name(&format!("package.identifier.has.invalid{ch}char")),
                Err(PackageNameValidationError(
                    PackageIdentifierValidationError::HasInvalidCharacter(ch)
                ))
            );
        }
    }

    #[test]
    fn test_package_name_has_non_ascii_character() {
        let result = validate_package_name("package.identifier.has.invalidїchar");
        assert_eq!(
            result,
            Err(PackageNameValidationError(PackageIdentifierValidationError::HasInvalidCharacter(
                'ї'
            )))
        );
        // Verify that error formatting is correct.
        assert_eq!(
            result.unwrap_err().to_string(),
            "should consist of [a-z, 0-9, _, .] characters, found 'ї'"
        );
    }

    #[test]
    fn test_package_name_ends_with_dot() {
        assert_eq!(
            validate_package_name("com.google.sdv."),
            Err(PackageNameValidationError(
                PackageIdentifierValidationError::EndsWithInvalidCharacter('.')
            ))
        );
    }

    #[test]
    fn test_package_name_segment_starts_with_digit() {
        for s in ["0", "0com.google.sdv", "com.0google.sdv", "com.google.0sdv"] {
            assert_eq!(
                validate_package_name(s),
                Err(PackageNameValidationError(
                    PackageIdentifierValidationError::SegmentStartsWithInvalidCharacter('0')
                )),
                "{s}"
            );
        }
    }

    #[test]
    fn test_package_name_segment_starts_with_underscore() {
        for s in ["_", "_com.google.sdv", "com._google.sdv", "com.google._sdv"] {
            assert_eq!(
                validate_package_name(s),
                Err(PackageNameValidationError(
                    PackageIdentifierValidationError::SegmentStartsWithInvalidCharacter('_')
                )),
                "{s}"
            );
        }
    }

    #[test]
    fn test_package_name_segment_starts_with_dot() {
        for s in [".", ".com.google.sdv", "com..google.sdv", "com.google..sdv"] {
            assert_eq!(
                validate_package_name(s),
                Err(PackageNameValidationError(
                    PackageIdentifierValidationError::SegmentStartsWithInvalidCharacter('.')
                )),
                "{s}"
            );
        }
    }

    #[test]
    fn test_package_name_segment_ends_with_underscore() {
        for s in ["com_.google.sdv", "com.google_.sdv", "com.google.sdv_"] {
            assert_eq!(
                validate_package_name(s),
                Err(PackageNameValidationError(
                    PackageIdentifierValidationError::SegmentEndsWithInvalidCharacter('_')
                )),
                "{s}"
            );
        }
    }

    #[test]
    fn test_package_name_has_one_segment() {
        assert_eq!(
            validate_package_name("comgooglesdv"),
            Err(PackageNameValidationError(PackageIdentifierValidationError::HasOneSegment))
        );
    }

    #[test]
    fn test_package_name_constains_consecutive_underscores() {
        for s in [
            "co___m.google.sdv",
            "com.goog__le.sdv",
            "com.google.s_____dv",
            "com__.google.sdv",
            "com.google__.sdv",
            "com.google.sdv__",
        ] {
            assert_eq!(
                validate_package_name(s),
                Err(PackageNameValidationError(
                    PackageIdentifierValidationError::ContainsConsecutiveUnderscores
                )),
                "{s}"
            );
        }
    }

    #[test]
    fn test_package_namespace_is_valid() {
        assert_eq!(validate_package_namespace("com"), Ok(()));
        assert_eq!(validate_package_namespace("com.google"), Ok(()));
        assert_eq!(validate_package_namespace("com.google.sdv"), Ok(()));
    }

    #[test]
    fn test_package_namespace_is_invalid() {
        assert_eq!(
            validate_package_namespace(""),
            Err(PackageNamespaceValidationError(PackageIdentifierValidationError::IsEmpty))
        );
    }
}
