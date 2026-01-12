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

//! This file creates and represents the Instance Name.

use std::fmt;
use std::str::FromStr;

use sdv_comms_id_validator::instance_name_validation::{
    validate_instance_name, InstanceNameValidationError,
};

/// Instance Name, e.g. `main-instance`, `front-left-10`.
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq)]
pub struct InstanceName(String);

impl InstanceName {
    /// Returns wrapped name as &str.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl FromStr for InstanceName {
    type Err = InstanceNameValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_instance_name(s)?;
        Ok(InstanceName(s.to_string()))
    }
}

impl fmt::Display for InstanceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use sdv_comms_id_validator::alphanumhyphen_validation::AlphanumHyphenValidationError;

    #[test]
    fn test_instance_name_from_valid_str() {
        assert_eq!(
            InstanceName::from_str("main-instance"),
            Ok(InstanceName("main-instance".to_string()))
        );
        assert_eq!(
            InstanceName::from_str("instance-7777"),
            Ok(InstanceName("instance-7777".to_string()))
        );
    }

    #[test]
    fn test_instance_name_from_invalid_str() {
        let alphanum_err = AlphanumHyphenValidationError::IsEmpty;
        assert_eq!(InstanceName::from_str(""), Err(InstanceNameValidationError(alphanum_err)));

        let alphanum_err = AlphanumHyphenValidationError::EndsWithInvalidCharacter('*');
        assert_eq!(
            InstanceName::from_str("instance-*"),
            Err(InstanceNameValidationError(alphanum_err))
        );
    }
}
