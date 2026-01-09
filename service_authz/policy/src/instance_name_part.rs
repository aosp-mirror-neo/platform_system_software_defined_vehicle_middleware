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

use sdv_comms_id_validator::{validate_instance_name, InstanceNameValidationError};
use thiserror::Error;

use std::fmt;
use std::str::FromStr;

use crate::selector_or_constant_value::SelectorOrConstantValue;

/// InstanceNamePart is used to describe a Set of FQINs which are matching
/// underlying Instance name rules.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) struct InstanceNamePart(pub SelectorOrConstantValue);

/// Error that can arise during Instance Name validation.
#[derive(Error, Debug, PartialEq)]
#[error("{0}")]
pub struct InstanceNamePartParsingError(#[from] pub InstanceNameValidationError);

impl FromStr for InstanceNamePart {
    type Err = InstanceNamePartParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let part = SelectorOrConstantValue::from_str(s, &validate_instance_name)?;
        Ok(InstanceNamePart(part))
    }
}

impl fmt::Display for InstanceNamePart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::selector::StarSelector;

    #[test]
    fn test_from_str_success_selector() {
        let result = InstanceNamePart::from_str("*");
        assert_eq!(
            result,
            Ok(InstanceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)))
        );
    }

    #[test]
    fn test_from_str_success_constant_value() {
        let result = InstanceNamePart::from_str("instance-front-left");
        assert_eq!(
            result,
            Ok(InstanceNamePart(SelectorOrConstantValue::ConstantValue(
                "instance-front-left".into()
            )))
        );
    }

    #[test]
    fn test_from_str_failed_invalid_selector() {
        let err = InstanceNamePart::from_str("!*").unwrap_err();
        assert!(matches!(err, InstanceNamePartParsingError(_)), "Unexpected error: {err:?}");
    }

    #[test]
    fn test_from_str_failed_invalid_constant_value() {
        let err = InstanceNamePart::from_str("*-front-left").unwrap_err();
        assert!(matches!(err, InstanceNamePartParsingError(_)), "Unexpected error: {err:?}");
    }
}
