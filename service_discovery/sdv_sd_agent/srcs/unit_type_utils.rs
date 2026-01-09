// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::UnitType::UnitType;

/// Clears the serviceBundleName field from a UnitType and converts qualified names
/// into a fully-qualified package name + unqualified unit type name.
///
/// Service bundle name as part of unit types is deprecated
pub fn deprecate_unit_type_owner(unit_type: UnitType) -> UnitType {
    let mut modified_unit_type = unit_type.clone();

    // Handle qualified UnitType names, relative to the deprecated owner service bundle.
    // If unitTypeName is "com.example.MyType", the part before the last dot ("com.example")
    // is prepended to the sdvPackageName, and the part after the last dot ("MyType") becomes the typeName.
    if let Some((package_prefix, unit_name)) = modified_unit_type.typeName.rsplit_once('.') {
        let full_package = if modified_unit_type.sdvPackageName.is_empty() {
            package_prefix.to_string()
        } else {
            format!("{}.{}", modified_unit_type.sdvPackageName, package_prefix)
        };
        modified_unit_type.sdvPackageName = full_package;
        modified_unit_type.typeName = unit_name.to_string();
    }

    // Clear the serviceBundleName, which is deprecated
    modified_unit_type.serviceBundleName = "".to_string();

    modified_unit_type
}

#[cfg(test)]
mod tests {
    use super::deprecate_unit_type_owner;
    use super::UnitType;

    #[test]
    fn test_deprecate_unit_type_owner_service_bundle_name_cleared() {
        let input_unit_type = UnitType {
            sdvPackageName: "com.example".to_string(),
            serviceBundleName: "SomeServiceBundle".to_string(),
            typeName: "MyType".to_string(),
        };
        let expected_unit_type = UnitType {
            sdvPackageName: "com.example".to_string(),
            serviceBundleName: "".to_string(),
            typeName: "MyType".to_string(),
        };

        let result = deprecate_unit_type_owner(input_unit_type);
        assert_eq!(result, expected_unit_type);
    }

    #[test]
    fn test_deprecate_unit_type_owner_qualified_name() {
        let input_unit_type = UnitType {
            sdvPackageName: "com.example".to_string(),
            serviceBundleName: "MyBundle".to_string(),
            typeName: "foo.bar.MyType".to_string(),
        };
        let expected_unit_type = UnitType {
            sdvPackageName: "com.example.foo.bar".to_string(),
            serviceBundleName: "".to_string(),
            typeName: "MyType".to_string(),
        };

        let result = deprecate_unit_type_owner(input_unit_type);
        assert_eq!(result, expected_unit_type);
    }
}
