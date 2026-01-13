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

//! This file creates and represents the Channel Name.

use std::convert::TryFrom;
use std::fmt;
use std::str::FromStr;

use sdv_comms_id_validator::channel_name_validation::{
    validate_channel_name, ChannelNameValidationError,
};

/// Name of a pub/sub channel.
//
/// Together with unit type, uniquely identifies a pub/sub channel in the system
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChannelName(String);

impl AsRef<str> for ChannelName {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<ChannelName> for String {
    fn from(val: ChannelName) -> Self {
        val.0
    }
}

impl TryFrom<String> for ChannelName {
    type Error = ChannelNameValidationError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        validate_channel_name(&name)?;
        Ok(ChannelName(name))
    }
}

impl FromStr for ChannelName {
    type Err = ChannelNameValidationError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        validate_channel_name(name)?;
        Ok(ChannelName(name.to_string()))
    }
}

impl fmt::Display for ChannelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_name_from_str() {
        assert_eq!(
            ChannelName::from_str("car-speed-1"),
            Ok(ChannelName("car-speed-1".to_string()))
        );
        assert_eq!(ChannelName::from_str("cooling"), Ok(ChannelName("cooling".to_string())));
        assert_eq!(
            ChannelName::from_str("Tire_Pressure"),
            Ok(ChannelName("Tire_Pressure".to_string()))
        );
        assert_eq!(
            ChannelName::from_str("Channel-Name"),
            Ok(ChannelName("Channel-Name".to_string()))
        );
        assert_eq!(
            ChannelName::from_str("Channel Name 1!"),
            Ok(ChannelName("Channel Name 1!".to_string()))
        );
        assert_eq!(ChannelName::from_str(""), Ok(ChannelName("".to_string())));
    }

    #[test]
    fn test_invalid_channel_name_from_str() {
        assert_eq!(
            ChannelName::from_str("channel\x07name"),
            Err(ChannelNameValidationError::HasInvalidCharacter('\u{7}'))
        );
    }

    #[test]
    fn test_invalid_channel_name_try_from_str() {
        assert_eq!(
            ChannelName::try_from("channel\x07name".to_string()),
            Err(ChannelNameValidationError::HasInvalidCharacter('\u{7}'))
        );
    }

    #[test]
    fn test_too_long_channel_name_from_str() {
        assert_eq!(
            ChannelName::from_str("channelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelch"),
            Err(ChannelNameValidationError::ExceedsMaxLength(127, 128))
        );
    }

    #[test]
    fn test_too_long_channel_name_try_from_str() {
        assert_eq!(
            ChannelName::try_from("channelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelchannelch".to_string()),
            Err(ChannelNameValidationError::ExceedsMaxLength(127, 128))
        );
    }

    #[test]
    fn test_channel_name_display() {
        let channel_name = ChannelName::from_str("car-speed-1").unwrap();
        assert_eq!(format!("{channel_name}"), "car-speed-1");
    }

    #[test]
    fn test_channel_name_conversions() {
        let name = "test-channel";
        // Test FromStr for ChannelName
        let channel_name = ChannelName::from_str(name).unwrap();

        // Test AsRef<str>
        assert_eq!(channel_name.as_ref(), name);

        // Test From<ChannelName> for String
        let s: String = channel_name.clone().into();
        assert_eq!(s, name);

        // Test TryFrom<String> for ChannelName
        let channel_name_from_string = ChannelName::try_from(name.to_string()).unwrap();
        assert_eq!(channel_name_from_string, channel_name);
    }
}
