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

use sdv_comms_id_validator::{validate_service_name, ServiceNameValidationError};
use thiserror::Error;

use std::fmt;
use std::str::FromStr;

use crate::selector_or_constant_value::SelectorOrConstantValue;

/// ServiceNamePart is used to describe a Set of FQINs which are matching
/// underlying Service name rules.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) struct ServiceNamePart(pub SelectorOrConstantValue);

/// Error that can arise during Service Name validation.
#[derive(Error, Debug, PartialEq)]
#[error("{0}")]
pub struct ServiceNamePartParsingError(#[from] pub ServiceNameValidationError);

impl FromStr for ServiceNamePart {
    type Err = ServiceNamePartParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let part = SelectorOrConstantValue::from_str(s, &validate_service_name)?;
        Ok(ServiceNamePart(part))
    }
}

impl fmt::Display for ServiceNamePart {
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
        let result = ServiceNamePart::from_str("*");
        assert_eq!(
            result,
            Ok(ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)))
        );
    }

    #[test]
    fn test_from_str_success_constant_value() {
        let result = ServiceNamePart::from_str("WindowManager");
        assert_eq!(
            result,
            Ok(ServiceNamePart(SelectorOrConstantValue::ConstantValue("WindowManager".into())))
        );
    }

    #[test]
    fn test_from_str_failed_invalid_selector() {
        let err = ServiceNamePart::from_str("!*").unwrap_err();
        assert!(matches!(err, ServiceNamePartParsingError(_)), "Unexpected error: {err:?}");
    }

    #[test]
    fn test_from_str_failed_invalid_constant_value() {
        let err = ServiceNamePart::from_str("*Manager").unwrap_err();
        assert!(matches!(err, ServiceNamePartParsingError(_)), "Unexpected error: {err:?}");
    }
}
