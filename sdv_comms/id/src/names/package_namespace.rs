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

//! This file creates and represents the Package Namespace.

use std::fmt;
use std::str::FromStr;

use sdv_comms_id_validator::package_identifier_validation::{
    validate_package_namespace, PackageNamespaceValidationError,
};

/// Package Namespace, e.g. `com`, `com.google`, `com.google.sdv`.
#[repr(transparent)]
#[derive(Debug, PartialEq)]
pub struct PackageNamespace(String);

impl PackageNamespace {
    /// Returns wrapped name as &str.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl FromStr for PackageNamespace {
    type Err = PackageNamespaceValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_package_namespace(s)?;
        Ok(PackageNamespace(s.to_string()))
    }
}

impl fmt::Display for PackageNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use sdv_comms_id_validator::package_identifier_validation::PackageIdentifierValidationError;

    #[test]
    fn test_package_namespace_from_valid_str() {
        assert_eq!(PackageNamespace::from_str("com"), Ok(PackageNamespace("com".to_string())));
        assert_eq!(
            PackageNamespace::from_str("com.google"),
            Ok(PackageNamespace("com.google".to_string()))
        );
        assert_eq!(
            PackageNamespace::from_str("com.google.sdv"),
            Ok(PackageNamespace("com.google.sdv".to_string()))
        );
    }

    #[test]
    fn test_package_namespace_from_invalid_str() {
        let identifier_err = PackageIdentifierValidationError::IsEmpty;
        assert_eq!(
            PackageNamespace::from_str(""),
            Err(PackageNamespaceValidationError(identifier_err))
        );

        let identifier_err =
            PackageIdentifierValidationError::SegmentStartsWithInvalidCharacter('.');
        assert_eq!(
            PackageNamespace::from_str(".com.android"),
            Err(PackageNamespaceValidationError(identifier_err))
        );
    }
}
