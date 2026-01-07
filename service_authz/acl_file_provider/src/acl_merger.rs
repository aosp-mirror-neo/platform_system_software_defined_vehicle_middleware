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
use std::str::FromStr;

use anyhow::Result;
use protobuf::Message;

use sdv_authz_policy_proto_rust::acl_policy::AclPolicy as AclPolicyProto;
use sdv_comms_id::names::PackageName;

use crate::caching_acl_reader::CachingAclReader;

pub(crate) fn merge_package_acl_policies(
    package_name: &PackageName,
    unit_type_acls: &HashMap<PackageName, HashSet<PathBuf>>,
    reader: &CachingAclReader,
) -> Result<AclPolicyProto> {
    let mut merged_policy = AclPolicyProto::new();
    let mut current_package_name = Some(package_name.clone());
    while let Some(p_name) = current_package_name {
        if let Some(acl_policy_paths) = unit_type_acls.get(&p_name) {
            for acl_policy_path in acl_policy_paths {
                let mut policy = reader.read_or_get_cached(&p_name, None, acl_policy_path)?;

                // To avoid name collisions when merging ACLs from multiple files, prefix
                // each ACL name with a unique identifier derived from the file path.
                let prefix = format!("{}_", acl_policy_path.to_string_lossy());

                for acl in &mut policy.acls {
                    acl.name = format!("{}{}", prefix, acl.name);
                }

                // A package without a service bundle cannot be used to guard service units
                policy.service_units = Vec::new();

                for ut in &mut policy.unit_types {
                    ut.acl_names = ut
                        .acl_names
                        .iter()
                        .map(|old_name| format!("{}{}", prefix, old_name))
                        .collect();
                }

                merged_policy.merge_from_bytes(&policy.write_to_bytes()?)?;
            }
        }
        current_package_name =
            p_name.name().rfind('.').and_then(|i| PackageName::from_str(&p_name.name()[..i]).ok());
    }

    Ok(merged_policy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdv_authz_policy_proto_rust::acl_policy::{Acl, UnitAclMapping};
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
