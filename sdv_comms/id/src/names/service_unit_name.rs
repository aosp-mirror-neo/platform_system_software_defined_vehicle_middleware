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

//! This file creates and represents the Service Unit Name.

use std::fmt;
use std::str::FromStr;

use sdv_comms_id_validator::service_unit_name_validation::{
    validate_service_unit_name, ServiceUnitNameValidationError,
};

/// Unit Name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ServiceUnitName(String);

impl ServiceUnitName {
    /// Returns the name as a string slice.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl FromStr for ServiceUnitName {
    type Err = ServiceUnitNameValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_service_unit_name(s)?;
        Ok(ServiceUnitName(s.to_string()))
    }
}

impl fmt::Display for ServiceUnitName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_unit_name_from_str() {
        assert_eq!(
            ServiceUnitName::from_str("car-speed-1"),
            Ok(ServiceUnitName("car-speed-1".to_string()))
        );
        assert_eq!(
            ServiceUnitName::from_str("cooling"),
            Ok(ServiceUnitName("cooling".to_string()))
        );
        assert_eq!(
            ServiceUnitName::from_str("tire-pressure"),
            Ok(ServiceUnitName("tire-pressure".to_string()))
        );
    }

    #[test]
    fn test_service_unit_name_from_str_invalid() {
        assert!(ServiceUnitName::from_str("").is_err());
        assert!(ServiceUnitName::from_str("Unit-Name").is_err());
        assert!(ServiceUnitName::from_str("unit_name").is_err());
        assert!(ServiceUnitName::from_str("unit--name").is_err());
    }

    #[test]
    fn test_service_unit_name_display() {
        let unit_name = ServiceUnitName::from_str("car-speed-1").unwrap();
        assert_eq!(format!("{unit_name}"), "car-speed-1");
    }
}
