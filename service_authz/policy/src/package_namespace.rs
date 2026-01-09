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

//! Represents PackageNamespace type used to define ACL statements.

use std::fmt;

use crate::constant_value::ConstantValue;

/// PackageNamespace represents FQIN Set where each FQIN has specified package
/// namespace.
///
/// For example, vm1:com.google.sdv.Window/instance has the following package
/// namespaces: "com", "com.google". Moreover, "com.google.sdv" package name
/// considered as a package namespace.
///
/// Validation is already complete when PackageNamespace object is created.
/// TODO(b/335388119): Use type from sdv_identity instead of String.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) struct PackageNamespace(pub String);

impl PackageNamespace {
    /// Returns whether there is a non-empty intersection between two FQIN Sets
    /// represented by the following `PackageNamespace`s.
    pub fn has_intersection(&self, other: &PackageNamespace) -> bool {
        self.contains_narrower(&other.0) || other.contains_narrower(&self.0)
    }

    /// Returns whether there is a non-empty intersection between two FQIN Sets
    /// represented by `PackageNamespace` and `ConstantValue`.
    pub fn has_intersection_with_constant_value(&self, other: &ConstantValue) -> bool {
        self.contains_narrower(&other.0)
    }

    /// Returns whether `self` is broader namespace and contains `narrower` namespace.
    pub fn contains_narrower(&self, narrower: &String) -> bool {
        let broader: &String = &self.0;

        broader == narrower ||
        // append `.` to `broader` to check whether the `narrower` namespace lies in
        // the `broader` namespace.
        //
        // Simply check that `narrower` starts with `broader` is not enough, for example
        // "a.b.c" does not contain "a.b.com.x".
        narrower.starts_with(&format!("{broader}."))
    }
}

impl<S: Into<String>> From<S> for PackageNamespace {
    fn from(value: S) -> Self {
        Self(value.into())
    }
}

impl fmt::Display for PackageNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.*", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_intersection_true() {
        let ns = PackageNamespace::from("com.google.sdv");
        assert!(ns.has_intersection(&"com".into()));
        assert!(ns.has_intersection(&"com.google".into()));
        assert!(ns.has_intersection(&"com.google.sdv".into()));
        assert!(ns.has_intersection(&"com.google.sdv.wm".into()));
        assert!(ns.has_intersection(&"com.google.sdv.wm.main".into()));
    }

    #[test]
    fn test_has_intersection_false() {
        let ns = PackageNamespace::from("com.google.sdv");
        assert!(!ns.has_intersection(&"".into()));
        assert!(!ns.has_intersection(&"co".into()));
        assert!(!ns.has_intersection(&"com.goog".into()));
        assert!(!ns.has_intersection(&"com.google.s".into()));
        assert!(!ns.has_intersection(&"com.google.sdvsdv".into()));
    }

    #[test]
    fn test_has_intersection_with_constant_value_true() {
        let ns = PackageNamespace::from("com.google");
        assert!(ns.has_intersection_with_constant_value(&"com.google".into()));
        assert!(ns.has_intersection_with_constant_value(&"com.google.sdv".into()));
    }

    #[test]
    fn test_has_intersection_with_constant_value_false() {
        let ns = PackageNamespace::from("com.google");
        assert!(!ns.has_intersection_with_constant_value(&"com".into()));
        assert!(!ns.has_intersection_with_constant_value(&"com.googlesdv".into()));
        assert!(!ns.has_intersection_with_constant_value(&"com.goog".into()));
        assert!(!ns.has_intersection_with_constant_value(&"com.goog.sdv".into()));
    }
}
