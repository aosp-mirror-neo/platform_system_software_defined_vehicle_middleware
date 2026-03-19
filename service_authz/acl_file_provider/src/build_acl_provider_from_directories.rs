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

/// Builder function to construct an ACL provider from mappings in a set of directories.
use std::path::Path;

use tracing::instrument;

use crate::read_acl_file_mappings_from_directory;
use crate::read_vm_file_mappings_from_directory;
use crate::AclFileProvider;

/// Build an AclFileProvider by scanning mappings from a list of directories.
#[instrument(name = "authz_perf::provider::build_acl_provider_from_directories", skip_all)]
pub fn build_acl_provider_from_directories<P: AsRef<Path>>(
    directories: impl Iterator<Item = P>,
    vm_directory: impl AsRef<Path>,
) -> anyhow::Result<AclFileProvider> {
    let acl_file_mappings = directories
        .map(|d| read_acl_file_mappings_from_directory(d.as_ref()))
        .collect::<anyhow::Result<Vec<_>>>()?;

    let (vm_file_mappings, default_vm_file) =
        read_vm_file_mappings_from_directory(vm_directory.as_ref())?;

    AclFileProvider::new(
        acl_file_mappings.into_iter().flatten(),
        vm_file_mappings.into_iter(),
        default_vm_file,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::str::FromStr;
    use tempfile::TempDir;

    use sdv_authz::SingleVmAclPolicyProvider;
    use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName};

    // Only add empty files because we only care about file presence here.
    fn add_acl_file(
        root_dir: &Path,
        package_name: &PackageName,
        bundle_name: &ServiceBundleName,
    ) -> PathBuf {
        let mut path = PathBuf::from(root_dir);
        path.push(package_name.name());
        std::fs::create_dir(&path).unwrap();

        path.push(bundle_name.name());
        path.set_extension("textproto");
        std::fs::write(&path, "").unwrap();

        path
    }

    #[test]
    fn test_build_acl_provider_from_directories_success() {
        let test_tempdir = TempDir::new().unwrap();

        let mut root_path_1 = test_tempdir.path().to_path_buf();
        root_path_1.push("root1");
        std::fs::create_dir(&root_path_1).unwrap();

        let mut root_path_2 = test_tempdir.path().to_path_buf();
        root_path_2.push("root2");
        std::fs::create_dir(&root_path_2).unwrap();

        let pkg_name = PackageName::from_str("com.package").unwrap();

        let bundle_1_name = ServiceBundleName::from_str("Bundle1").unwrap();
        add_acl_file(root_path_1.as_path(), &pkg_name, &bundle_1_name);

        let bundle_2_name = ServiceBundleName::from_str("Bundle2").unwrap();
        add_acl_file(root_path_2.as_path(), &pkg_name, &bundle_2_name);

        let mut vm_root_path = test_tempdir.path().to_path_buf();
        vm_root_path.push("vm_root");
        std::fs::create_dir(&vm_root_path).unwrap();

        // Add a vm-1.textproto to vm_root
        let mut vm_1_file = vm_root_path.clone();
        vm_1_file.push("vm-1.textproto");
        std::fs::write(
            &vm_1_file,
            r#"
            allow_publisher {
                message: "com.pkg.Msg"
                topic: "*"
            }
        "#,
        )
        .unwrap();

        // Add a .default.textproto to vm_root
        let mut default_vm_file = vm_root_path.clone();
        default_vm_file.push(".default.textproto");
        std::fs::write(
            &default_vm_file,
            r#"
            deny_publisher {
                message: "com.pkg.Msg"
                topic: "*"
            }
        "#,
        )
        .unwrap();

        let provider = build_acl_provider_from_directories(
            [root_path_1.as_path(), root_path_2.as_path()].into_iter(),
            vm_root_path.as_path(),
        )
        .unwrap();

        let known_namespaces_vec = provider.get_namespace_with_acl_policy().unwrap();
        assert_eq!(known_namespaces_vec.len(), 3); // 2 bundles + 1 package

        // Check that the right set of bundles is present in the provider.
        let known_namespaces_set =
            HashSet::<(PackageName, Option<ServiceBundleName>)>::from_iter(known_namespaces_vec);
        assert_eq!(known_namespaces_set.len(), 3);
        assert!(known_namespaces_set.contains(&(pkg_name.clone(), Some(bundle_1_name.clone()))));
        assert!(known_namespaces_set.contains(&(pkg_name.clone(), Some(bundle_2_name.clone()))));
        assert!(known_namespaces_set.contains(&(pkg_name.clone(), None)));

        // Check the vm-1 policy
        use sdv_authz::VmLevelPermissionsProvider;
        use sdv_comms_id::names::VmName;
        let vm_name = VmName::from_str("vm-1").unwrap();
        let vm_policy = provider.get_vm_authz_policy_proto(&vm_name).unwrap();
        assert!(!vm_policy.allow_publisher.is_empty());

        // Check that the default VM policy works
        // (vm-2 is not in the directory, so it should use the default policy)
        let vm_name = VmName::from_str("vm-2").unwrap();
        let vm_policy = provider.get_vm_authz_policy_proto(&vm_name).unwrap();
        assert!(!vm_policy.deny_publisher.is_empty());
    }
}
