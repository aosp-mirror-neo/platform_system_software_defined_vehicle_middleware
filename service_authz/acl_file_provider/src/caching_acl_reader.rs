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

//! Utility function to read an ACL policy proto from a file.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use protobuf::text_format;
use tracing::trace_span;

use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
use sdv_authz_policy_proto_rust::vm_authz_policy::VmAuthzPolicy as VmAuthzPolicyProto;
use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName, VmName};

/// Caches ACL policy protos read from files.
#[derive(Debug, Default)]
pub(crate) struct CachingAclReader {
    cache: Mutex<HashMap<PathBuf, AclPolicyProto>>,
    vm_cache: Mutex<HashMap<PathBuf, VmAuthzPolicyProto>>,
}

impl CachingAclReader {
    /// Reads the ACL policy proto from the given path, using the cache if possible.
    pub(crate) fn read_or_get_cached(
        &self,
        package_name: &PackageName,
        service_bundle_name: Option<&ServiceBundleName>,
        acl_policy_path: &PathBuf,
    ) -> Result<AclPolicyProto> {
        {
            let cache = self.cache.lock().unwrap();
            if let Some(acl_policy) = cache.get(acl_policy_path) {
                return Ok(acl_policy.clone());
            }
        }

        let acl_policy_proto =
            read_acl_policy_proto_from_file(package_name, service_bundle_name, acl_policy_path)?;

        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(acl_policy_path.clone(), acl_policy_proto.clone());
        }

        Ok(acl_policy_proto)
    }

    /// Reads the VM Authz policy proto from the given path, using the cache if possible.
    #[allow(dead_code)]
    pub(crate) fn read_or_get_vm_cached(
        &self,
        vm_name: &VmName,
        acl_policy_path: &PathBuf,
    ) -> Result<VmAuthzPolicyProto> {
        {
            let vm_cache = self.vm_cache.lock().unwrap();
            if let Some(vm_authz_policy) = vm_cache.get(acl_policy_path) {
                return Ok(vm_authz_policy.clone());
            }
        }

        let vm_authz_policy_proto = read_vm_authz_policy_proto_from_file(vm_name, acl_policy_path)?;

        {
            let mut vm_cache = self.vm_cache.lock().unwrap();
            vm_cache.insert(acl_policy_path.clone(), vm_authz_policy_proto.clone());
        }

        Ok(vm_authz_policy_proto)
    }
}

fn read_acl_policy_proto_from_file(
    package_name: &PackageName,
    service_bundle_name: Option<&ServiceBundleName>,
    acl_policy_path: &PathBuf,
) -> anyhow::Result<AclPolicyProto> {
    let span = trace_span!("authz_perf::provider::read_textproto").entered();
    let acl_policy_content = std::fs::read_to_string(acl_policy_path).with_context(|| {
        format!(
            "{}{}: Failed to read ACL policy from path {}",
            package_name,
            match service_bundle_name {
                Some(name) => format!(".{}", name),
                None => String::new(),
            },
            acl_policy_path.display()
        )
    })?;
    drop(span);

    let span =
        trace_span!("authz_perf::provider::parse_textproto", size_bytes = acl_policy_content.len())
            .entered();
    let acl_policy_proto: AclPolicyProto = text_format::parse_from_str(&acl_policy_content)
        .with_context(|| {
            format!(
                "{}{}: Failed to parse AclPolicy textproto file {}",
                package_name,
                match service_bundle_name {
                    Some(name) => format!(".{}", name),
                    None => String::new(),
                },
                acl_policy_path.display()
            )
        })?;
    drop(span);

    Ok(acl_policy_proto)
}

#[allow(dead_code)]
fn read_vm_authz_policy_proto_from_file(
    vm_name: &VmName,
    acl_policy_path: &PathBuf,
) -> anyhow::Result<VmAuthzPolicyProto> {
    let span = trace_span!("authz_perf::provider::read_vm_textproto").entered();
    let acl_policy_content = std::fs::read_to_string(acl_policy_path).with_context(|| {
        format!(
            "{}: Failed to read VM Authz policy from path {}",
            vm_name,
            acl_policy_path.display()
        )
    })?;
    drop(span);

    let span = trace_span!(
        "authz_perf::provider::parse_vm_textproto",
        size_bytes = acl_policy_content.len()
    )
    .entered();
    let vm_authz_policy_proto: VmAuthzPolicyProto =
        text_format::parse_from_str(&acl_policy_content).with_context(|| {
            format!(
                "{}: Failed to parse VmAuthzPolicy textproto file {}",
                vm_name,
                acl_policy_path.display()
            )
        })?;
    drop(span);

    Ok(vm_authz_policy_proto)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdv_comms_id::names::ServiceUnitName;
    use std::str::FromStr;
    use tempfile::TempDir;

    fn make_acl_textproto_string(service_unit_name: &ServiceUnitName) -> String {
        // Creates an AclPolicy with a service unit whose name references the pkg/bundle
        // so that we can validate that we get the right one back.
        format!(
            r#"service_units {{
                   name: "{}"
                }}
            "#,
            service_unit_name.name()
        )
    }

    #[test]
    fn test_read_or_get_cached() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();

        let pkg_name = PackageName::from_str("com.package").unwrap();
        let bundle_name = ServiceBundleName::from_str("Bundle").unwrap();
        let service_unit_name = ServiceUnitName::from_str("service-unit-name").unwrap();

        let mut acl_file_path = PathBuf::from(root_path);
        acl_file_path.push(pkg_name.name());
        acl_file_path.set_extension(bundle_name.name());
        std::fs::write(&acl_file_path, make_acl_textproto_string(&service_unit_name).as_str())
            .unwrap();

        let reader = CachingAclReader::default();

        // First read: should read from file
        let policy1 =
            reader.read_or_get_cached(&pkg_name, Some(&bundle_name), &acl_file_path).unwrap();
        assert_eq!(policy1.service_units.len(), 1);
        assert_eq!(policy1.service_units[0].name, service_unit_name.name());

        // Modify the file content
        let new_service_unit_name = ServiceUnitName::from_str("new-service-unit-name").unwrap();
        std::fs::write(&acl_file_path, make_acl_textproto_string(&new_service_unit_name).as_str())
            .unwrap();

        // Second read: should return cached version
        let policy2 =
            reader.read_or_get_cached(&pkg_name, Some(&bundle_name), &acl_file_path).unwrap();
        assert_eq!(policy2.service_units.len(), 1);
        assert_eq!(policy2.service_units[0].name, service_unit_name.name());

        // Check that policy1 and policy2 are the same instance
        assert_eq!(format!("{:?}", &policy1), format!("{:?}", &policy2));
    }

    #[test]
    fn test_read_file_not_found() {
        let pkg_name = PackageName::from_str("com.package").unwrap();
        let bundle_name = ServiceBundleName::from_str("Bundle").unwrap();
        let non_existent_path = PathBuf::from("/path/to/non/existent/file.textproto");

        let reader = CachingAclReader::default();
        let result = reader.read_or_get_cached(&pkg_name, Some(&bundle_name), &non_existent_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to read ACL policy from path"));
    }

    #[test]
    fn test_read_unparseable_proto() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();

        let pkg_name = PackageName::from_str("com.package").unwrap();
        let bundle_name = ServiceBundleName::from_str("Bundle").unwrap();

        let mut acl_file_path = PathBuf::from(root_path);
        acl_file_path.push(pkg_name.name());
        acl_file_path.set_extension(bundle_name.name());
        std::fs::write(&acl_file_path, "this is not a valid textproto").unwrap();

        let reader = CachingAclReader::default();
        let result = reader.read_or_get_cached(&pkg_name, Some(&bundle_name), &acl_file_path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse AclPolicy textproto file"));
    }

    #[test]
    fn test_read_or_get_vm_cached() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();
        let vm_name = VmName::from_str("vm-name").unwrap();

        let mut vm_authz_policy_file_path = PathBuf::from(root_path);
        vm_authz_policy_file_path.push("vm_authz_policy.textproto");

        // Initial content
        std::fs::write(
            &vm_authz_policy_file_path,
            r#"
            allow_publisher {
                message: "com.pkg.Msg"
            }
        "#,
        )
        .unwrap();

        let reader = CachingAclReader::default();

        // First read: should read from file
        let policy1 = reader.read_or_get_vm_cached(&vm_name, &vm_authz_policy_file_path).unwrap();
        assert!(!policy1.allow_publisher.is_empty());

        // Modify the file content
        std::fs::write(&vm_authz_policy_file_path, "").unwrap();

        // Second read: should return cached version (true)
        let policy2 = reader.read_or_get_vm_cached(&vm_name, &vm_authz_policy_file_path).unwrap();
        assert!(!policy2.allow_publisher.is_empty());

        // Check that policy1 and policy2 are the same instance
        assert_eq!(format!("{:?}", &policy1), format!("{:?}", &policy2));
    }

    #[test]
    fn test_read_vm_file_not_found() {
        let vm_name = VmName::from_str("vm-name").unwrap();
        let non_existent_path = PathBuf::from("/path/to/non/existent/vm_file.textproto");

        let reader = CachingAclReader::default();
        let result = reader.read_or_get_vm_cached(&vm_name, &non_existent_path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to read VM Authz policy from path"));
    }

    #[test]
    fn test_read_unparseable_vm_proto() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();
        let vm_name = VmName::from_str("vm-name").unwrap();

        let mut acl_file_path = PathBuf::from(root_path);
        acl_file_path.push("invalid_vm_policy.textproto");
        std::fs::write(&acl_file_path, "this is not a valid textproto").unwrap();

        let reader = CachingAclReader::default();
        let result = reader.read_or_get_vm_cached(&vm_name, &acl_file_path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to parse VmAuthzPolicy textproto file"));
    }
}
