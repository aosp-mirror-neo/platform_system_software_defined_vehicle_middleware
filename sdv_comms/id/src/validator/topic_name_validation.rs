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

use thiserror::Error;

/// Maximum length of the Topic Name in bytes.
///
/// Decrease by 1 since Rust strings do not have a NUL terminator at the end compare to C strings.
pub const TOPIC_NAME_MAX_LENGTH_BYTES: usize = 128 - 1;

/// Error that can arise during Topic Name validation.
#[derive(Error, Debug, PartialEq)]
pub enum TopicNameValidationError {
    /// Indicates that Topic Name is empty.
    #[error("should not be empty")]
    IsEmpty,
    /// Indicates that Topic Name exceeds max length in bytes.
    #[error("should not be longer than {0} bytes, found {1} bytes")]
    ExceedsMaxLength(usize, usize),
    /// Indicates that Topic Name contains invalid character.
    #[error("should consist of printable ASCII characters, found '{0}'")]
    HasInvalidCharacter(char),
}

/// Validates whether `s` is a valid Topic Name.
pub fn validate_topic_name(s: &str) -> Result<(), TopicNameValidationError> {
    // TODO (b/475489800)
    // Enabling back empty topic name for compatibility reason.
    // Once all tests will be updated with valid name the following code
    // can be uncommented.
    // if s.is_empty() {
    //     return Err(TopicNameValidationError::IsEmpty);
    // }

    if s.len() > TOPIC_NAME_MAX_LENGTH_BYTES {
        return Err(TopicNameValidationError::ExceedsMaxLength(
            TOPIC_NAME_MAX_LENGTH_BYTES,
            s.len(),
        ));
    }

    for ch in s.chars() {
        if !ch.is_ascii_graphic() && !ch.is_ascii_whitespace() {
            return Err(TopicNameValidationError::HasInvalidCharacter(ch));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_name_is_valid() {
        assert_eq!(validate_topic_name("Topic-Name_123"), Ok(()));
        assert_eq!(validate_topic_name("CarSpeedTopic"), Ok(()));
        assert_eq!(validate_topic_name("Tire_Pressure-FrontLeft"), Ok(()));
        assert_eq!(validate_topic_name("cooling"), Ok(()));
        assert_eq!(validate_topic_name("With Spaces"), Ok(()));
        assert_eq!(validate_topic_name("!@#$%^&*()_+=-`~"), Ok(()));
        assert_eq!(validate_topic_name("Topic	Name"), Ok(()));
        assert_eq!(
            validate_topic_name(
                "Topic
Name"
            ),
            Ok(())
        );
        assert_eq!(validate_topic_name(""), Ok(()));
    }

    // TODO (b/475489800)
    // uncomment the following test
    // #[test]
    // fn test_topic_name_is_empty() {
    //     assert_eq!(validate_topic_name(""), Err(TopicNameValidationError::IsEmpty));
    // }

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
        assert_eq!(
            result.unwrap_err().to_string(),
            format!(
                "should not be longer than {} bytes, found {} bytes",
                TOPIC_NAME_MAX_LENGTH_BYTES,
                TOPIC_NAME_MAX_LENGTH_BYTES + 1
            )
        );
    }

    #[test]
    fn test_topic_name_has_invalid_character() {
        assert_eq!(
            validate_topic_name("topic\x07name"),
            Err(TopicNameValidationError::HasInvalidCharacter('\u{7}'))
        );
        assert_eq!(
            validate_topic_name("\u{0000}"),
            Err(TopicNameValidationError::HasInvalidCharacter('\0'))
        );
    }
}
