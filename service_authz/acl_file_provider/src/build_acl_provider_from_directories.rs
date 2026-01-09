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
use crate::AclFileProvider;

/// Build an AclFileProvider by scanning mappings from a list of directories.
#[instrument(name = "authz_perf::provider::build_acl_provider_from_directories", skip_all)]
pub fn build_acl_provider_from_directories<P: AsRef<Path>>(
    directories: impl Iterator<Item = P>,
) -> anyhow::Result<AclFileProvider> {
    let dir_iters = directories
        .map(|d| read_acl_file_mappings_from_directory(d.as_ref()))
        .collect::<anyhow::Result<Vec<_>>>()?;

    AclFileProvider::new(dir_iters.into_iter().flatten())
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

        let provider = build_acl_provider_from_directories(
            [root_path_1.as_path(), root_path_2.as_path()].into_iter(),
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
    }
}
