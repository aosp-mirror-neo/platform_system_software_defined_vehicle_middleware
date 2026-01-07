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

use std::fmt;

/// ConstantValue is used to describe a Set of objects which are equal to the
/// constant value.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) struct ConstantValue(pub String);

impl ConstantValue {
    /// Returns whether there is a non-empty intersection between two FQIN Sets
    /// represented by the following `ConstantValue`s.
    pub fn has_intersection(&self, other: &ConstantValue) -> bool {
        self.0 == other.0
    }
}

impl<S: Into<String>> From<S> for ConstantValue {
    fn from(value: S) -> Self {
        Self(value.into())
    }
}

impl fmt::Display for ConstantValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_value_intersection_false() {
        let value1 = ConstantValue::from("value1");
        let value2 = ConstantValue::from("value2");
        assert!(!value1.has_intersection(&value2));
    }

    #[test]
    fn test_constant_value_intersection_true() {
        let value1 = ConstantValue::from("value");
        let value2 = ConstantValue::from("value");
        assert!(value1.has_intersection(&value2));
    }
}
