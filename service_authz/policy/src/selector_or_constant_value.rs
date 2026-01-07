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

use std::error::Error;
use std::fmt;

use crate::constant_value::ConstantValue;
use crate::selector::StarSelector;

/// SelectorOrConstantValue is used to describe a Set of objects with either
/// Selector or ConstantValue.
///
/// Later this enum will be used to describe some parts of the ACL statement.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) enum SelectorOrConstantValue {
    StarSelector(StarSelector),
    ConstantValue(ConstantValue),
}

impl SelectorOrConstantValue {
    /// Returns whether there is a non-empty intersection between two FQIN Sets
    /// represented by the following `SelectorOrConstantValue`s.
    pub fn has_intersection(&self, other: &SelectorOrConstantValue) -> bool {
        match self {
            SelectorOrConstantValue::StarSelector(_) => true,
            SelectorOrConstantValue::ConstantValue(self_constant_value) => match other {
                SelectorOrConstantValue::StarSelector(_) => true,
                SelectorOrConstantValue::ConstantValue(other_constant_value) => {
                    self_constant_value.has_intersection(other_constant_value)
                }
            },
        }
    }
}

impl SelectorOrConstantValue {
    // TODO(b/338029226): Consider use &str in Authz types and thus we could remove generics from this function.
    pub(crate) fn from_str<ErrT: Error>(
        s: &str,
        constant_validation_fn: &dyn Fn(&str) -> Result<(), ErrT>,
    ) -> Result<Self, ErrT> {
        if let Some(star_selector) = StarSelector::try_from(s) {
            return Ok(SelectorOrConstantValue::StarSelector(star_selector));
        }

        constant_validation_fn(s)?;
        Ok(SelectorOrConstantValue::ConstantValue(ConstantValue(s.to_string())))
    }
}

impl fmt::Display for SelectorOrConstantValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let out = match self {
            SelectorOrConstantValue::StarSelector(star_selector) => star_selector.to_string(),
            SelectorOrConstantValue::ConstantValue(constant_value) => constant_value.to_string(),
        };
        write!(f, "{out}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selector_has_intersection_with_constant_value() {
        let star = SelectorOrConstantValue::StarSelector(StarSelector);
        let constant_value = SelectorOrConstantValue::ConstantValue("value".into());

        assert!(star.has_intersection(&constant_value));
        assert!(constant_value.has_intersection(&star));
    }

    #[test]
    fn test_constant_value_has_intersection_with_constant_value() {
        let constant_value1 = SelectorOrConstantValue::ConstantValue("value1".into());
        let constant_value1_duplicate = SelectorOrConstantValue::ConstantValue("value1".into());
        let constant_value2 = SelectorOrConstantValue::ConstantValue("value2".into());

        assert!(constant_value1.has_intersection(&constant_value1_duplicate));
        assert!(constant_value1_duplicate.has_intersection(&constant_value1));

        assert!(!constant_value1.has_intersection(&constant_value2));
        assert!(!constant_value2.has_intersection(&constant_value1));
    }
}
