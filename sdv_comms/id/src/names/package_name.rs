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

//! This file creates and represents the Package Name.

use std::fmt;
use std::str::FromStr;

use sdv_comms_id_validator::package_identifier_validation::{
    validate_package_name, PackageNameValidationError,
};

/// Package Name, e.g. `com.google.sdv`, `com.android.sdv.cluster`.
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Default, Hash, Eq)]
pub struct PackageName(String);

impl PackageName {
    /// Returns wrapped name as &str.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl FromStr for PackageName {
    type Err = PackageNameValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_package_name(s)?;
        Ok(PackageName(s.to_string()))
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use sdv_comms_id_validator::package_identifier_validation::PackageIdentifierValidationError;

    #[test]
    fn test_package_name_from_valid_str() {
        assert_eq!(
            PackageName::from_str("com.google.sdv"),
            Ok(PackageName("com.google.sdv".to_string()))
        );
        assert_eq!(
            PackageName::from_str("com.android.sdv.cluster"),
            Ok(PackageName("com.android.sdv.cluster".to_string()))
        );
    }

    #[test]
    fn test_package_name_from_invalid_str() {
        let identifier_err = PackageIdentifierValidationError::IsEmpty;
        assert_eq!(PackageName::from_str(""), Err(PackageNameValidationError(identifier_err)));

        let identifier_err =
            PackageIdentifierValidationError::SegmentStartsWithInvalidCharacter('.');
        assert_eq!(
            PackageName::from_str(".com.android"),
            Err(PackageNameValidationError(identifier_err))
        );
    }
}
