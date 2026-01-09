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

//! This file creates and represents the Unit Type Name.

use std::fmt;
use std::str::FromStr;

use sdv_comms_id_validator::unit_type_name_validation::{
    validate_unit_type_name, UnitTypeNameValidationError,
};

/// Unit Type Name, e.g. `Temperature`, `CalibrationService`.
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Default, Hash, Eq)]
pub struct UnitTypeName(String);

impl UnitTypeName {
    /// Returns wrapped name as &str.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl FromStr for UnitTypeName {
    type Err = UnitTypeNameValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_unit_type_name(s)?;
        Ok(UnitTypeName(s.to_string()))
    }
}

impl fmt::Display for UnitTypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdv_comms_id_validator::UnitTypeNameValidationError;

    #[test]
    fn test_service_name_from_valid_str() {
        assert_eq!(
            UnitTypeName::from_str("Temperature"),
            Ok(UnitTypeName("Temperature".to_string()))
        );
        assert_eq!(
            UnitTypeName::from_str("Temperature7"),
            Ok(UnitTypeName("Temperature7".to_string()))
        );
        assert_eq!(
            UnitTypeName::from_str("Measured_Temperature"),
            Ok(UnitTypeName("Measured_Temperature".to_string()))
        );
    }

    #[test]
    fn test_service_name_from_invalid_str() {
        assert_eq!(UnitTypeName::from_str(""), Err(UnitTypeNameValidationError::IsEmpty));
        assert_eq!(
            UnitTypeName::from_str("Temperature*"),
            Err(UnitTypeNameValidationError::EndsWithInvalidCharacter('*'))
        );
        assert_eq!(
            UnitTypeName::from_str("temperature"),
            Err(UnitTypeNameValidationError::TypeNameStartsWithInvalidCharacter('t'))
        );
        assert_eq!(
            UnitTypeName::from_str("0Ttemperature"),
            Err(UnitTypeNameValidationError::TypeNameStartsWithInvalidCharacter('0'))
        );
    }
}
