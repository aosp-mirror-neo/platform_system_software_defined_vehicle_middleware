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

//! Represents ACL constant value type used to define ACL statements.

use sdv_comms_id_validator::{
    validate_package_name, validate_package_namespace, PackageNameValidationError,
    PackageNamespaceValidationError,
};
use thiserror::Error;

use std::fmt;
use std::str::FromStr;

use crate::constant_value::ConstantValue;
use crate::package_namespace::PackageNamespace;
use crate::selector::StarSelector;

/// PackageNamePart is used to describe a Set of FQINs which are matching
/// underlying Package name rules.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) enum PackageNamePart {
    StarSelector(StarSelector),
    ConstantValue(ConstantValue),
    PackageNamespace(PackageNamespace),
}

/// Error that can arise during Package Name part validation.
#[derive(Error, Debug, PartialEq)]
pub enum PackageNamePartParsingError {
    /// Indicates that Package Namespace is not valid.
    #[error("{0}")]
    InvalidNamespace(#[from] PackageNamespaceValidationError),
    /// Indicates that Package Name is not valid.
    #[error("{0}")]
    InvalidName(#[from] PackageNameValidationError),
}

impl FromStr for PackageNamePart {
    type Err = PackageNamePartParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(star_selector) = StarSelector::try_from(s) {
            return Ok(PackageNamePart::StarSelector(star_selector));
        }

        const NAMESPACE_END_SELECTOR: &str = ".*";
        if s.ends_with(NAMESPACE_END_SELECTOR) {
            let package_ns: &str = &s[0..s.len() - NAMESPACE_END_SELECTOR.len()];
            validate_package_namespace(package_ns)?;
            return Ok(PackageNamePart::PackageNamespace(PackageNamespace(package_ns.to_string())));
        }

        validate_package_name(s)?;
        Ok(PackageNamePart::ConstantValue(ConstantValue(s.to_string())))
    }
}

impl PackageNamePart {
    /// Returns whether there is a non-empty intersection between two FQIN Sets
    /// represented by the following `PackageNamePart`s.
    pub fn has_intersection(&self, other: &PackageNamePart) -> bool {
        match self {
            PackageNamePart::StarSelector(_) => true,
            PackageNamePart::ConstantValue(self_constant_value) => match other {
                PackageNamePart::StarSelector(_) => true,
                PackageNamePart::ConstantValue(other_constant_value) => {
                    self_constant_value.has_intersection(other_constant_value)
                }
                PackageNamePart::PackageNamespace(other_package_ns) => {
                    other_package_ns.has_intersection_with_constant_value(self_constant_value)
                }
            },
            PackageNamePart::PackageNamespace(self_package_ns) => match other {
                PackageNamePart::StarSelector(_) => true,
                PackageNamePart::ConstantValue(other_constant_value) => {
                    self_package_ns.has_intersection_with_constant_value(other_constant_value)
                }
                PackageNamePart::PackageNamespace(other_package_ns) => {
                    other_package_ns.has_intersection(self_package_ns)
                }
            },
        }
    }
}

impl fmt::Display for PackageNamePart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let out = match self {
            PackageNamePart::StarSelector(star_selector) => star_selector.to_string(),
            PackageNamePart::ConstantValue(constant_value) => constant_value.to_string(),
            PackageNamePart::PackageNamespace(package_ns) => package_ns.to_string(),
        };
        write!(f, "{out}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_success_selector() {
        let result = PackageNamePart::from_str("*");
        assert_eq!(result, Ok(PackageNamePart::StarSelector(StarSelector)));
    }

    #[test]
    fn test_from_str_success_package_namespace() {
        let result = PackageNamePart::from_str("com.google.sdv.*");
        assert_eq!(result, Ok(PackageNamePart::PackageNamespace("com.google.sdv".into())));
    }

    #[test]
    fn test_from_str_success_constant_value() {
        let result = PackageNamePart::from_str("com.google.sdv");
        assert_eq!(result, Ok(PackageNamePart::ConstantValue("com.google.sdv".into())));
    }

    #[test]
    fn test_from_str_failed_invalid_selector() {
        let err = PackageNamePart::from_str("!*").unwrap_err();
        assert!(
            matches!(err, PackageNamePartParsingError::InvalidName(_)),
            "Unexpected error: {err:?}"
        );
    }

    #[test]
    fn test_from_str_failed_invalid_package_namespace() {
        let err = PackageNamePart::from_str("com.*.google.sdv.*").unwrap_err();
        assert!(
            matches!(err, PackageNamePartParsingError::InvalidNamespace(_)),
            "Unexpected error: {err:?}"
        );
    }

    #[test]
    fn test_from_str_failed_invalid_constant_value() {
        let err = PackageNamePart::from_str("*.google.sdv").unwrap_err();
        assert!(
            matches!(err, PackageNamePartParsingError::InvalidName(_)),
            "Unexpected error: {err:?}"
        );
    }

    #[test]
    fn test_selector_has_intersection_with_selector() {
        let star1 = PackageNamePart::StarSelector(StarSelector);
        let star2 = PackageNamePart::StarSelector(StarSelector);

        assert!(star1.has_intersection(&star2));
        assert!(star2.has_intersection(&star1));
    }

    #[test]
    fn test_selector_has_intersection_with_constant_value() {
        let star = PackageNamePart::StarSelector(StarSelector);
        let constant_value = PackageNamePart::ConstantValue("com.google.sdv".into());

        assert!(star.has_intersection(&constant_value));
        assert!(constant_value.has_intersection(&star));
    }

    #[test]
    fn test_selector_has_intersection_with_package_namespace() {
        let star = PackageNamePart::StarSelector(StarSelector);
        let package_ns = PackageNamePart::PackageNamespace("com.google.sdv".into());

        assert!(star.has_intersection(&package_ns));
        assert!(package_ns.has_intersection(&star));
    }

    #[test]
    fn test_constant_value_has_intersection_with_constant_value() {
        let constant_value1 = PackageNamePart::ConstantValue("com.google.sdv1".into());
        let constant_value1_duplicate = PackageNamePart::ConstantValue("com.google.sdv1".into());
        let constant_value2 = PackageNamePart::ConstantValue("com.google.sdv2".into());

        assert!(constant_value1.has_intersection(&constant_value1_duplicate));
        assert!(constant_value1_duplicate.has_intersection(&constant_value1));

        assert!(!constant_value1.has_intersection(&constant_value2));
        assert!(!constant_value2.has_intersection(&constant_value1));
    }

    #[test]
    fn test_constant_value_has_intersection_with_package_namespace() {
        let constant_value = PackageNamePart::ConstantValue("com.google.sdv.wm".into());
        let package_ns1 = PackageNamePart::PackageNamespace("com.google.sdv".into());
        let package_ns2 = PackageNamePart::PackageNamespace("fr.oem.sdv".into());

        assert!(constant_value.has_intersection(&package_ns1));
        assert!(package_ns1.has_intersection(&constant_value));

        assert!(!constant_value.has_intersection(&package_ns2));
        assert!(!package_ns2.has_intersection(&constant_value));
    }

    #[test]
    fn test_package_namespace_has_intersection_with_package_namespace() {
        let package_ns1 = PackageNamePart::PackageNamespace("com.google.sdv".into());
        let package_ns1_narrower = PackageNamePart::PackageNamespace("com.google.sdv.init".into());
        let package_ns2 = PackageNamePart::PackageNamespace("com.google.ivi".into());

        assert!(package_ns1.has_intersection(&package_ns1_narrower));
        assert!(package_ns1_narrower.has_intersection(&package_ns1));

        assert!(!package_ns1.has_intersection(&package_ns2));
        assert!(!package_ns2.has_intersection(&package_ns1));
    }
}
