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

//! This file creates and represents the Topic Name.

use std::convert::TryFrom;
use std::fmt;
use std::str::FromStr;

use sdv_comms_id_validator::topic_name_validation::{
    validate_topic_name, TopicNameValidationError,
};

/// Name of a pub/sub topic.
//
/// Together with unit type, uniquely identifies a pub/sub topic in the system
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TopicName(String);

impl AsRef<str> for TopicName {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<TopicName> for String {
    fn from(val: TopicName) -> Self {
        val.0
    }
}

impl TryFrom<String> for TopicName {
    type Error = TopicNameValidationError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        validate_topic_name(&name)?;
        Ok(TopicName(name))
    }
}

impl FromStr for TopicName {
    type Err = TopicNameValidationError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        validate_topic_name(name)?;
        Ok(TopicName(name.to_string()))
    }
}

impl fmt::Display for TopicName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_name_from_str() {
        assert_eq!(TopicName::from_str("car-speed-1"), Ok(TopicName("car-speed-1".to_string())));
        assert_eq!(TopicName::from_str("cooling"), Ok(TopicName("cooling".to_string())));
        assert_eq!(
            TopicName::from_str("Tire_Pressure"),
            Ok(TopicName("Tire_Pressure".to_string()))
        );
        assert_eq!(TopicName::from_str("Topic-Name"), Ok(TopicName("Topic-Name".to_string())));
        assert_eq!(
            TopicName::from_str("Topic Name 1!"),
            Ok(TopicName("Topic Name 1!".to_string()))
        );
        assert_eq!(TopicName::from_str(""), Ok(TopicName("".to_string())));
    }

    #[test]
    fn test_invalid_topic_name_from_str() {
        assert_eq!(
            TopicName::from_str("topic\x07name"),
            Err(TopicNameValidationError::HasInvalidCharacter('\u{7}'))
        );
    }

    #[test]
    fn test_invalid_topic_name_try_from_str() {
        assert_eq!(
            TopicName::try_from("topic\x07name".to_string()),
            Err(TopicNameValidationError::HasInvalidCharacter('\u{7}'))
        );
    }

    #[test]
    fn test_too_long_topic_name_from_str() {
        assert_eq!(
            TopicName::from_str("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            Err(TopicNameValidationError::ExceedsMaxLength(127, 128))
        );
    }

    #[test]
    fn test_too_long_topic_name_try_from_str() {
        assert_eq!(
            TopicName::try_from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()),
            Err(TopicNameValidationError::ExceedsMaxLength(127, 128))
        );
    }

    #[test]
    fn test_topic_name_display() {
        let topic_name = TopicName::from_str("car-speed-1").unwrap();
        assert_eq!(format!("{topic_name}"), "car-speed-1");
    }

    #[test]
    fn test_topic_name_conversions() {
        let name = "test-topic";
        // Test FromStr for TopicName
        let topic_name = TopicName::from_str(name).unwrap();

        // Test AsRef<str>
        assert_eq!(topic_name.as_ref(), name);

        // Test From<TopicName> for String
        let s: String = topic_name.clone().into();
        assert_eq!(s, name);

        // Test TryFrom<String> for TopicName
        let topic_name_from_string = TopicName::try_from(name.to_string()).unwrap();
        assert_eq!(topic_name_from_string, topic_name);
    }
}
