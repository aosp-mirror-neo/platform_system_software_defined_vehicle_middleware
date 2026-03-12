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

//! This file parses and validates the Topic Name.

use super::alphanumhyphen_validation::{
    validate_alphanumhyphen_allow_empty, AlphanumHyphenValidationError,
};
use thiserror::Error;

/// Maximum length of the Topic Name in bytes.
///
/// Decrease by 1 since Rust strings do not have a NUL terminator at the end compare to C strings.
pub const TOPIC_NAME_MAX_LENGTH_BYTES: usize = 128 - 1;

/// Error that can arise during Topic Name validation.
#[derive(Error, Debug, PartialEq)]
pub enum TopicNameValidationError {
    #[error("Exceeds max length {0}: {1}")]
    /// Indicates that name exceeds max length in bytes.
    ExceedsMaxLength(usize, usize),
    /// Indicates that name contains invalid character.
    #[error("should consist of [a-z, 0-9, -] characters, found '{0}'")]
    HasInvalidCharacter(char),
    /// Indicates that name starts with invalid character.
    #[error("should start with [a-z] character, found '{0}'")]
    StartsWithInvalidCharacter(char),
    /// Indicates that name ends with invalid character.
    #[error("should end with [a-z, 0-9] character, found '{0}'")]
    EndsWithInvalidCharacter(char),
    /// Indicates that name contains consecutive hyphens.
    #[error("should not contain consecutive hyphens (--)")]
    ContainsConsecutiveHyphens,
}

// Manually map the internal error to your public error
impl From<AlphanumHyphenValidationError> for TopicNameValidationError {
    fn from(err: AlphanumHyphenValidationError) -> Self {
        match err {
            AlphanumHyphenValidationError::HasInvalidCharacter(c) => Self::HasInvalidCharacter(c),
            AlphanumHyphenValidationError::StartsWithInvalidCharacter(c) => {
                Self::StartsWithInvalidCharacter(c)
            }
            AlphanumHyphenValidationError::EndsWithInvalidCharacter(c) => {
                Self::EndsWithInvalidCharacter(c)
            }
            AlphanumHyphenValidationError::ContainsConsecutiveHyphens => {
                Self::ContainsConsecutiveHyphens
            }
            AlphanumHyphenValidationError::ExceedsMaxLength(max, actual) => {
                Self::ExceedsMaxLength(max, actual)
            }
            _ => {
                // other errors are do not apply to topics
                unreachable!()
            }
        }
    }
}

/// Validates whether `s` is a valid Topic Name.
pub fn validate_topic_name(s: &str) -> Result<(), TopicNameValidationError> {
    validate_alphanumhyphen_allow_empty(s, TOPIC_NAME_MAX_LENGTH_BYTES)
        .map_err(TopicNameValidationError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_name_is_valid() {
        assert_eq!(validate_topic_name("topic-name-123"), Ok(()));
        assert_eq!(validate_topic_name("car-speed-topic"), Ok(()));
        assert_eq!(validate_topic_name("cooling"), Ok(()));
        assert_eq!(validate_topic_name(""), Ok(())); // Empty string is allowed
    }

    #[test]
    fn test_topic_name_exceeds_max_length() {
        let long_name = "a".repeat(TOPIC_NAME_MAX_LENGTH_BYTES + 1);
        let result = validate_topic_name(&long_name);
        assert_eq!(
            result,
            Err(TopicNameValidationError::ExceedsMaxLength(
                TOPIC_NAME_MAX_LENGTH_BYTES,
                TOPIC_NAME_MAX_LENGTH_BYTES + 1
            ))
        );
    }

    #[test]
    fn test_topic_name_has_invalid_character() {
        assert_eq!(
            validate_topic_name("topic_name"),
            Err(TopicNameValidationError::HasInvalidCharacter('_'))
        );
        assert_eq!(
            validate_topic_name("topic-Name"),
            Err(TopicNameValidationError::HasInvalidCharacter('N'))
        );
        assert_eq!(
            validate_topic_name("topic name"),
            Err(TopicNameValidationError::HasInvalidCharacter(' '))
        );
    }

    #[test]
    fn test_topic_name_starts_with_invalid_character() {
        assert_eq!(
            validate_topic_name("-topic-name"),
            Err(TopicNameValidationError::StartsWithInvalidCharacter('-'))
        );
        assert_eq!(
            validate_topic_name("Topic-name"),
            Err(TopicNameValidationError::StartsWithInvalidCharacter('T'))
        );
    }

    #[test]
    fn test_topic_name_ends_with_invalid_character() {
        assert_eq!(
            validate_topic_name("topic-name-"),
            Err(TopicNameValidationError::EndsWithInvalidCharacter('-'))
        );
    }

    #[test]
    fn test_topic_name_has_consecutive_hyphens() {
        assert_eq!(
            validate_topic_name("topic--name"),
            Err(TopicNameValidationError::ContainsConsecutiveHyphens)
        );
    }
}
