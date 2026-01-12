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

//! This file creates and represents the Service Name.

use std::fmt;
use std::str::FromStr;

use sdv_comms_id_validator::service_name_validation::{
    validate_service_name, ServiceNameValidationError,
};

/// Service Name, e.g. `SomeService`, `WindowService`.
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Default, Hash, Eq)]
pub struct ServiceName(String);

impl ServiceName {
    /// Returns wrapped name as &str.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl FromStr for ServiceName {
    type Err = ServiceNameValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_service_name(s)?;
        Ok(ServiceName(s.to_string()))
    }
}

impl fmt::Display for ServiceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_name_from_valid_str() {
        assert_eq!(
            ServiceName::from_str("SomeService"),
            Ok(ServiceName("SomeService".to_string()))
        );
        assert_eq!(
            ServiceName::from_str("WindowService888"),
            Ok(ServiceName("WindowService888".to_string()))
        );
    }

    #[test]
    fn test_service_name_from_invalid_str() {
        assert_eq!(ServiceName::from_str(""), Err(ServiceNameValidationError::IsEmpty));
        assert_eq!(
            ServiceName::from_str("WindowService888*"),
            Err(ServiceNameValidationError::EndsWithInvalidCharacter('*'))
        );
    }
}
