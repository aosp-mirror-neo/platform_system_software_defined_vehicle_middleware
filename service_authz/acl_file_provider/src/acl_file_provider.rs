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

//! LocallyStoredAuthzPoliciesProvider implementation that reads ACLs from textproto files.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::anyhow;
use tracing::{instrument, trace_span};

use sdv_authz::{
    LocallyStoredAuthzPoliciesProvider, SingleVmAclPolicyProvider, VmLevelPermissionsProvider,
};
use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
use sdv_authz_policy_proto_rust::vm_authz_policy::VmAuthzPolicy as VmAuthzPolicyProto;
use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName, VmName};

use crate::acl_merger::merge_package_acl_policies;
use crate::caching_acl_reader::CachingAclReader;

/// Tuple struct mapping a single (package, service bundle) to an ACL policy file path.
pub type AclFileMapping = ((PackageName, Option<ServiceBundleName>), PathBuf);

/// LocallyStoredAuthzPoliciesProvider that reads each ACL from a textproto file.
#[derive(Debug, Default)]
pub struct AclFileProvider {
    service_bundle_acls: HashMap<(PackageName, ServiceBundleName), PathBuf>,
    package_acls: HashMap<PackageName, HashSet<PathBuf>>,
    reader: CachingAclReader,
}

impl AclFileProvider {
    /// Create an AclFileProvider with the given mappings.
    ///
    /// Can return error (e.g. if multiple mappings are provided for the same bundle).
    pub fn new(mapping_iter: impl Iterator<Item = AclFileMapping>) -> anyhow::Result<Self> {
        let mut provider = AclFileProvider::default();
        for (bundle_key, acl_policy_path) in mapping_iter {
            let (package_name, service_bundle_name) = bundle_key;
            if let Some(service_bundle_name) = service_bundle_name {
                let bundle_key = (package_name.clone(), service_bundle_name);
                if let Some(previous_acl_policy_path) =
                    provider.service_bundle_acls.get(&bundle_key)
                {
                    return Err(anyhow!(
                        "Multiple ACL policy files found for {}.{}: {} and {}",
                        bundle_key.0,
                        bundle_key.1,
                        previous_acl_policy_path.display(),
                        acl_policy_path.display()
                    ));
                }
                provider.service_bundle_acls.insert(bundle_key, acl_policy_path.clone());
                // Since service bundles might contain unit_types in legacy ACLs, also consider their
                // ACLs for unit types of the service bundle's package
                provider.package_acls.entry(package_name).or_default().insert(acl_policy_path);
            } else {
                provider.package_acls.entry(package_name).or_default().insert(acl_policy_path);
            }
        }

        let _span = trace_span!(
            "authz_perf::provider::new_acl_file_provider",
            mapping_count = provider.service_bundle_acls.len() + provider.package_acls.len()
        )
        .entered();

        Ok(provider)
    }
}

impl LocallyStoredAuthzPoliciesProvider for AclFileProvider {}

impl SingleVmAclPolicyProvider for AclFileProvider {
    /// Gets the Service Bundle / package ACL policy proto.
    #[instrument(name = "authz_perf::provider::get_acl_policy_proto", skip_all)]
    fn get_acl_policy_proto(
        &self,
        package_name: &PackageName,
        service_bundle_name: Option<&ServiceBundleName>,
    ) -> anyhow::Result<AclPolicyProto> {
        if let Some(service_bundle_name) = service_bundle_name {
            let acl_policy_path = self
                .service_bundle_acls
                .get(&(package_name.clone(), service_bundle_name.clone()))
                .ok_or_else(|| {
                    anyhow!("{}.{}: ACL policy is not found", package_name, service_bundle_name)
                })?;

            self.reader.read_or_get_cached(package_name, Some(service_bundle_name), acl_policy_path)
        } else {
            merge_package_acl_policies(package_name, &self.package_acls, &self.reader)
        }
    }

    /// Returns a vector of (Package Name, Service Bundle Name) pairs
    /// representing the service bundles / packages available on the local VM with
    /// available ACL policy.
    fn get_namespace_with_acl_policy(
        &self,
    ) -> Result<Vec<(PackageName, Option<ServiceBundleName>)>, anyhow::Error> {
        let mut result: Vec<(PackageName, Option<ServiceBundleName>)> =
            self.service_bundle_acls.keys().map(|k| (k.0.clone(), Some(k.1.clone()))).collect();
        result.extend(self.package_acls.keys().map(|k| (k.clone(), None)));
        Ok(result)
    }
}

impl VmLevelPermissionsProvider for AclFileProvider {
    fn get_vm_authz_policy_proto(
        &self,
        _vm_name: &VmName,
    ) -> Result<VmAuthzPolicyProto, anyhow::Error> {
        unimplemented!("VM Authz policy not supported by AclFileProvider yet");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use sdv_comms_id::names::ServiceUnitName;
    use std::collections::HashSet;
    use std::path::Path;
    use std::path::PathBuf;
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

    // Only add empty files because we only care about file presence here.
    fn add_acl_file(
        root_dir: &Path,
        package_name: &PackageName,
        bundle_name: &ServiceBundleName,
        service_unit_name: &ServiceUnitName,
    ) -> PathBuf {
        let mut path = PathBuf::from(root_dir);
        path.push(package_name.name());
        path.set_extension(bundle_name.name());
        std::fs::write(&path, make_acl_textproto_string(service_unit_name).as_str()).unwrap();

        path
    }

    #[test]
    fn test_new_success() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();

        let pkg_name = PackageName::from_str("com.package").unwrap();

        let bundle_1_name = ServiceBundleName::from_str("Bundle1").unwrap();
        let bundle_1_service_unit_name =
            ServiceUnitName::from_str("bundle-1-service-unit-name").unwrap();
        let bundle_1_path =
            add_acl_file(root_path, &pkg_name, &bundle_1_name, &bundle_1_service_unit_name);

        let bundle_2_name = ServiceBundleName::from_str("Bundle2").unwrap();
        let bundle_2_service_unit_name =
            ServiceUnitName::from_str("bundle-2-service-unit-name").unwrap();
        let bundle_2_path =
            add_acl_file(root_path, &pkg_name, &bundle_2_name, &bundle_2_service_unit_name);

        let mappings: [AclFileMapping; 2] = [
            ((pkg_name.clone(), Some(bundle_1_name.clone())), bundle_1_path.clone()),
            ((pkg_name.clone(), Some(bundle_2_name.clone())), bundle_2_path.clone()),
        ];

        let provider = AclFileProvider::new(mappings.into_iter()).unwrap();
        let known_namespaces_vec = provider.get_namespace_with_acl_policy().unwrap();
        assert_eq!(known_namespaces_vec.len(), 3); // 2 bundles + 1 package

        // Check that the right set of bundles is present in the provider.
        let known_namespaces_set =
            HashSet::<(PackageName, Option<ServiceBundleName>)>::from_iter(known_namespaces_vec);
        assert_eq!(known_namespaces_set.len(), 3);
        assert!(known_namespaces_set.contains(&(pkg_name.clone(), Some(bundle_1_name.clone()))));
        assert!(known_namespaces_set.contains(&(pkg_name.clone(), Some(bundle_2_name.clone()))));
        assert!(known_namespaces_set.contains(&(pkg_name.clone(), None)));

        // Check that we got the right policy for each bundle
        let bundle_1_policy = provider.get_acl_policy(&pkg_name, Some(&bundle_1_name)).unwrap();
        assert!(bundle_1_policy.has_service_unit(&bundle_1_service_unit_name));

        let bundle_2_policy = provider.get_acl_policy(&pkg_name, Some(&bundle_2_name)).unwrap();
        assert!(bundle_2_policy.has_service_unit(&bundle_2_service_unit_name));
    }

    #[test]
    fn test_new_errors_on_collision() {
        let pkg_name = PackageName::from_str("com.package").unwrap();
        let bundle_name = ServiceBundleName::from_str("Bundle").unwrap();

        let mappings: [AclFileMapping; 2] = [
            (
                (pkg_name.clone(), Some(bundle_name.clone())),
                PathBuf::from("/acls/com.package.Bundle.textproto"),
            ),
            (
                (pkg_name.clone(), Some(bundle_name.clone())),
                PathBuf::from("/more_acls/com.package.Bundle.textproto"),
            ),
        ];

        assert_eq!(AclFileProvider::new(mappings.into_iter())
            .unwrap_err()
            .to_string(),
            "Multiple ACL policy files found for com.package.Bundle: /acls/com.package.Bundle.textproto and /more_acls/com.package.Bundle.textproto");
    }

    #[test]
    fn test_new_succeeds_with_empty_mappings() {
        assert_eq!(
            AclFileProvider::new(std::iter::empty())
                .unwrap()
                .get_namespace_with_acl_policy()
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn test_read_unparseable_acl_textproto() {
        let pkg_name = PackageName::from_str("com.package").unwrap();
        let bundle_name = ServiceBundleName::from_str("Bundle").unwrap();

        let test_tempdir = TempDir::new().unwrap();
        let mut bad_acl_file_path = test_tempdir.path().to_path_buf();
        bad_acl_file_path.push("Unparseable.textproto");
        std::fs::write(bad_acl_file_path.as_path(), "this won't parse as an AclPolicy textproto")
            .unwrap();

        let provider = AclFileProvider::new(std::iter::once((
            (pkg_name.clone(), Some(bundle_name.clone())),
            bad_acl_file_path.clone(),
        )))
        .unwrap();

        assert!(provider
            .get_acl_policy_proto(&pkg_name, Some(&bundle_name))
            .unwrap_err()
            .to_string()
            .contains("Failed to parse AclPolicy textproto file"));
    }

    #[test]
    fn test_read_invalid_acl_textproto() {
        let pkg_name = PackageName::from_str("com.package").unwrap();
        let bundle_name = ServiceBundleName::from_str("Bundle").unwrap();

        let test_tempdir = TempDir::new().unwrap();
        let mut bad_acl_file_path = test_tempdir.path().to_path_buf();
        bad_acl_file_path.push("Invalid.textproto");
        std::fs::write(
            bad_acl_file_path.as_path(),
            // Valid parseable AclPolicy textproto, but the service unit references
            // a nonexistent ACL name.
            r#"
                service_units {
                    name: "serviceunitwithnonexistentacl"
                    acl_names: "NonExistentAcl"
                }
            "#,
        )
        .unwrap();

        let provider = AclFileProvider::new(std::iter::once((
            (pkg_name.clone(), Some(bundle_name.clone())),
            bad_acl_file_path,
        )))
        .unwrap();

        // We can get the proto...
        provider.get_acl_policy_proto(&pkg_name, Some(&bundle_name)).unwrap();

        // ...but not the AclPolicyModel
        assert_eq!(
            provider.get_acl_policy(&pkg_name, Some(&bundle_name)).unwrap_err().to_string(),
            "com.package.Bundle: Failed to parse ACL policy from proto"
        );
    }
}
