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

//! Utility function to merge ACL policies from multiple files.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::Result;
use protobuf::Message;

use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
use sdv_comms_id::names::PackageName;

use crate::caching_acl_reader::CachingAclReader;

/// Merges ACL policies from multiple files for a given package.
///
/// This function is used to aggregate all ACLs that apply to a package-level namespace.
/// It considers two types of files:
/// 1. Files explicitly associated with the package itself, by placing them in predefined
///    directories and not mentioning them in service bundle manifests.
/// 2. Files associated with service bundles within that package. Service bundle ACL files
///    are included because they may contain `unit_types` declarations which are
///    semantically scoped to the package.
///
/// ### Clash Avoidance
/// Since multiple files might define ACLs with the same name, this function:
/// *   Prefixes each ACL name with a unique identifier derived from its source file path.
/// *   Updates all `acl_names` references in `unit_types` to use these prefixed names.
///
/// ### Modifications
/// *   `service_units` are cleared in the merged policy because a package-level lookup
///     (where no specific service bundle is identified) should not be used to guard
///     bundle-specific service units.
pub(crate) fn merge_package_acl_policies(
    package_name: &PackageName,
    unit_type_acls: &HashMap<PackageName, HashSet<PathBuf>>,
    reader: &CachingAclReader,
) -> Result<AclPolicyProto> {
    let mut merged_policy = AclPolicyProto::new();
    if let Some(acl_policy_paths) = unit_type_acls.get(package_name) {
        for acl_policy_path in acl_policy_paths {
            let original_policy = reader.read_or_get_cached(package_name, None, acl_policy_path)?;
            let mut policy = AclPolicyProto::new();
            policy.acls = original_policy.acls;
            policy.unit_types = original_policy.unit_types;

            // To avoid name collisions when merging ACLs from multiple files, prefix
            // each ACL name with a unique identifier derived from the file path.
            let prefix = format!("{}_", acl_policy_path.to_string_lossy());

            for acl in &mut policy.acls {
                acl.name = format!("{}{}", prefix, acl.name);
            }

            for ut in &mut policy.unit_types {
                ut.acl_names =
                    ut.acl_names.iter().map(|old_name| format!("{}{}", prefix, old_name)).collect();
            }

            merged_policy.merge_from_bytes(&policy.write_to_bytes()?)?;
        }
    }

    Ok(merged_policy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdv_authz_policy_proto_rust::authz_policy::{Acl, UnitAclMapping};
    use sdv_comms_id::names::PackageName;
    use std::str::FromStr;
    use tempfile::TempDir;

    fn write_acl_policy(path: &PathBuf, policy: &AclPolicyProto) {
        let policy_str = protobuf::text_format::print_to_string(policy);
        std::fs::write(path, policy_str).unwrap();
    }

    #[test]
    fn test_merge_package_acl_policies_empty() {
        let pkg_name = PackageName::from_str("com.package").unwrap();
        let unit_type_acls = HashMap::new();
        let reader = CachingAclReader::default();

        let merged = merge_package_acl_policies(&pkg_name, &unit_type_acls, &reader).unwrap();
        assert_eq!(merged, AclPolicyProto::new());
    }

    #[test]
    fn test_merge_single_file() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();
        let pkg_name = PackageName::from_str("com.package").unwrap();

        let mut acl_path = PathBuf::from(root_path);
        acl_path.push("policy.textproto");

        let mut policy = AclPolicyProto::new();
        let mut acl = Acl::new();
        acl.name = "TestAcl".to_string();
        policy.acls.push(acl);
        let mut unit_type = UnitAclMapping::new();
        unit_type.name = "TestUnitType".to_string();
        unit_type.acl_names.push("TestAcl".to_string());
        policy.unit_types.push(unit_type);
        write_acl_policy(&acl_path, &policy);

        let mut unit_type_acls = HashMap::new();
        let mut paths = HashSet::new();
        paths.insert(acl_path.clone());
        unit_type_acls.insert(pkg_name.clone(), paths);

        let reader = CachingAclReader::default();
        let merged = merge_package_acl_policies(&pkg_name, &unit_type_acls, &reader).unwrap();

        assert_eq!(merged.acls.len(), 1);
        let prefix = format!("{}_", acl_path.to_string_lossy());
        let expected_acl_name = format!("{}TestAcl", prefix);
        assert_eq!(merged.acls[0].name, expected_acl_name);

        assert_eq!(merged.unit_types.len(), 1);
        assert_eq!(merged.unit_types[0].name, "TestUnitType");
        assert_eq!(merged.unit_types[0].acl_names.len(), 1);
        assert_eq!(merged.unit_types[0].acl_names[0], expected_acl_name);
        assert!(merged.service_units.is_empty());
    }

    #[test]
    fn test_merge_multiple_files_same_package() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();
        let pkg_name = PackageName::from_str("com.package").unwrap();

        let mut acl_path1 = PathBuf::from(root_path);
        acl_path1.push("policy1.textproto");
        let mut policy1 = AclPolicyProto::new();
        let mut acl1 = Acl::new();
        acl1.name = "Acl1".to_string();
        policy1.acls.push(acl1);
        write_acl_policy(&acl_path1, &policy1);

        let mut acl_path2 = PathBuf::from(root_path);
        acl_path2.push("policy2.textproto");
        let mut policy2 = AclPolicyProto::new();
        let mut acl2 = Acl::new();
        acl2.name = "Acl2".to_string();
        policy2.acls.push(acl2);
        write_acl_policy(&acl_path2, &policy2);

        let mut unit_type_acls = HashMap::new();
        let mut paths = HashSet::new();
        paths.insert(acl_path1.clone());
        paths.insert(acl_path2.clone());
        unit_type_acls.insert(pkg_name.clone(), paths);

        let reader = CachingAclReader::default();
        let merged = merge_package_acl_policies(&pkg_name, &unit_type_acls, &reader).unwrap();

        assert_eq!(merged.acls.len(), 2);
        let prefix1 = format!("{}_", acl_path1.to_string_lossy());
        let prefix2 = format!("{}_", acl_path2.to_string_lossy());
        let expected_acl_names = [format!("{}Acl1", prefix1), format!("{}Acl2", prefix2)];
        let actual_acl_names: Vec<String> = merged.acls.iter().map(|a| a.name.clone()).collect();
        assert!(actual_acl_names.contains(&expected_acl_names[0]));
        assert!(actual_acl_names.contains(&expected_acl_names[1]));
    }

    #[test]
    fn test_merge_service_units_cleared() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();
        let pkg_name = PackageName::from_str("com.package").unwrap();

        let mut acl_path = PathBuf::from(root_path);
        acl_path.push("policy.textproto");

        let policy_str = r#"
            service_units {
                name: "TestServiceUnit"
                acl_names: "TestAcl"
            }
            acls {
                name: "TestAcl"
            }
        "#;
        std::fs::write(&acl_path, policy_str).unwrap();

        let mut unit_type_acls = HashMap::new();
        let mut paths = HashSet::new();
        paths.insert(acl_path.clone());
        unit_type_acls.insert(pkg_name.clone(), paths);

        let reader = CachingAclReader::default();
        let merged = merge_package_acl_policies(&pkg_name, &unit_type_acls, &reader).unwrap();

        assert!(merged.service_units.is_empty());
        assert_eq!(merged.acls.len(), 1);
    }

    #[test]
    fn test_merge_package_hierarchy() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();
        let pkg_a = PackageName::from_str("com.package.a").unwrap();
        let pkg_b = PackageName::from_str("com.package").unwrap();

        // File 1 for package A: has ACL, UnitType, ServiceUnit (should be cleared), and allow_read_all
        let mut path_a1 = PathBuf::from(root_path);
        path_a1.push("a1.textproto");
        let mut policy_a1 = AclPolicyProto::new();
        policy_a1.acls.push({
            let mut acl = Acl::new();
            acl.name = "Acl1".to_string();
            acl
        });
        policy_a1.unit_types.push({
            let mut ut = UnitAclMapping::new();
            ut.name = "UT1".to_string();
            ut.acl_names.push("Acl1".to_string());
            ut
        });
        policy_a1.service_units.push({
            let mut su = UnitAclMapping::new();
            su.name = "SU1".to_string();
            su.acl_names.push("Acl1".to_string());
            su
        });
        policy_a1.allow_read_all = true;
        write_acl_policy(&path_a1, &policy_a1);

        // File 2 for package A: has another ACL and UnitType
        let mut path_a2 = PathBuf::from(root_path);
        path_a2.push("a2.textproto");
        let mut policy_a2 = AclPolicyProto::new();
        policy_a2.acls.push({
            let mut acl = Acl::new();
            acl.name = "Acl2".to_string();
            acl
        });
        policy_a2.unit_types.push({
            let mut ut = UnitAclMapping::new();
            ut.name = "UT2".to_string();
            ut.acl_names.push("Acl2".to_string());
            ut
        });
        write_acl_policy(&path_a2, &policy_a2);

        // File for package B: should be ignored by the merger for package A
        let mut path_b = PathBuf::from(root_path);
        path_b.push("b.textproto");
        let mut policy_b = AclPolicyProto::new();
        policy_b.acls.push({
            let mut acl = Acl::new();
            acl.name = "AclB".to_string();
            acl
        });
        write_acl_policy(&path_b, &policy_b);

        let mut unit_type_acls = HashMap::new();
        let mut paths_a = HashSet::new();
        paths_a.insert(path_a1.clone());
        paths_a.insert(path_a2.clone());
        unit_type_acls.insert(pkg_a.clone(), paths_a);

        let mut paths_b = HashSet::new();
        paths_b.insert(path_b.clone());
        unit_type_acls.insert(pkg_b.clone(), paths_b);

        let reader = CachingAclReader::default();
        let merged = merge_package_acl_policies(&pkg_a, &unit_type_acls, &reader).unwrap();

        // Verify the full contents of the AclPolicyProto message
        // 1. service_units MUST be cleared regardless of source file content
        assert!(merged.service_units.is_empty());

        // 2. acls MUST be merged from all relevant files and prefixed to avoid clashes
        assert_eq!(merged.acls.len(), 2);
        let prefix_a1 = format!("{}_", path_a1.to_string_lossy());
        let prefix_a2 = format!("{}_", path_a2.to_string_lossy());
        let acl_names: HashSet<String> = merged.acls.iter().map(|a| a.name.clone()).collect();
        assert!(acl_names.contains(&format!("{}Acl1", prefix_a1)));
        assert!(acl_names.contains(&format!("{}Acl2", prefix_a2)));
        // Data from package B MUST NOT be present
        assert!(!acl_names.contains("AclB"));

        // 3. unit_types MUST be merged and their acl_names references updated with prefixes
        assert_eq!(merged.unit_types.len(), 2);
        let ut_map: HashMap<String, Vec<String>> =
            merged.unit_types.iter().map(|ut| (ut.name.clone(), ut.acl_names.clone())).collect();
        assert_eq!(ut_map.get("UT1").unwrap(), &vec![format!("{}Acl1", prefix_a1)]);
        assert_eq!(ut_map.get("UT2").unwrap(), &vec![format!("{}Acl2", prefix_a2)]);

        // Verify that merge_package_acl_policies also works for pkg_b
        let merged_b = merge_package_acl_policies(&pkg_b, &unit_type_acls, &reader).unwrap();
        assert!(merged_b.service_units.is_empty());
        assert_eq!(merged_b.acls.len(), 1);
        let prefix_b = format!("{}_", path_b.to_string_lossy());
        assert_eq!(merged_b.acls[0].name, format!("{}AclB", prefix_b));
        assert!(merged_b.unit_types.is_empty());
        assert!(!merged_b.allow_read_all);
    }

    #[test]
    fn test_merge_file_not_found() {
        let pkg_name = PackageName::from_str("com.package").unwrap();
        let mut unit_type_acls = HashMap::new();
        let mut paths = HashSet::new();
        paths.insert(PathBuf::from("/non/existent.textproto"));
        unit_type_acls.insert(pkg_name.clone(), paths);

        let reader = CachingAclReader::default();
        let result = merge_package_acl_policies(&pkg_name, &unit_type_acls, &reader);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to read ACL policy from path"));
    }
}
