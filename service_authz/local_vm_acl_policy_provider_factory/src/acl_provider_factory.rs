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

//! Creates local VM ACL policy provider.

use log::{error, info};
use std::path::PathBuf;
use std::sync::Arc;

use tracing::instrument;

use sdv_authz::SingleVmAclPolicyProvider;
use sdv_authz_acl_file_provider::build_acl_provider_from_directories;
use sdv_authz_acl_file_provider::AclFileProvider;

/// Build SDV version of ACL provider that gets mappings from SBR.
#[cfg(sdv_acl_provider)]
#[instrument(name = "authz_perf::provider_factory::create_for_core", skip_all)]
pub fn create_local_vm_acl_policy_provider() -> anyhow::Result<Arc<dyn SingleVmAclPolicyProvider>> {
    use anyhow::Context;
    use sdv_authz_acl_file_provider::CachingAclFileProvider;
    use sdv_authz_sdv_local_vm_acl_policy_provider::create_acl_provider_from_service_bundle_registry;

    let provider: AclFileProvider = create_acl_provider_from_service_bundle_registry()
        .with_context(|| "Failed to create SdvLocalVmAclPolicyProvider")?;

    let caching_provider = CachingAclFileProvider::new(Arc::new(provider));

    Ok(Arc::new(caching_provider))
}

/// Build IVI version of ACL provider that gets mappings from a specific set of directories.
#[cfg(ivi_acl_provider)]
#[instrument(name = "authz_perf::provider_factory::create_for_ivi", skip_all)]
pub fn create_local_vm_acl_policy_provider() -> anyhow::Result<Arc<dyn SingleVmAclPolicyProvider>> {
    use sdv_authz_acl_file_provider::CachingAclFileProvider;

    const IVI_ACL_POLICY_DIRECTORIES: [&str; 4] = [
        "/product/etc/sdv_authz_acls",
        "/system/etc/sdv_authz_acls",
        "/system_ext/etc/sdv_authz_acls",
        "/vendor/etc/sdv_authz_acls",
    ];

    let provider: AclFileProvider =
        create_provider_from_directories_lenient(IVI_ACL_POLICY_DIRECTORIES.into_iter())?;

    let caching_provider = CachingAclFileProvider::new(Arc::new(provider));

    Ok(Arc::new(caching_provider))
}

/// ACL provider build function that is lenient about directory existence.
///
/// Filters missing directories rather than passing them to build_acl_provider_from_directories,
/// which would return error if given a nonexistent directory.
///
/// This may be removed / simplified once ACL directories are fully set up on IVI.
#[allow(dead_code)]
fn create_provider_from_directories_lenient<'a>(
    directories: impl Iterator<Item = &'a str>,
) -> Result<AclFileProvider, anyhow::Error> {
    // Convert to PathBufs and filter out those that don't exist.
    let paths = directories.map(PathBuf::from).filter(|path| {
        match path.try_exists() {
            Ok(true) => {
                info!("Loading ACL policies from {}", path.display());
                true
            }
            Ok(false) => {
                info!("Not loading ACL policies from {}; path does not exist", path.display());
                false
            }
            Err(e) => {
                // May happen in some test environments.  Don't fail, but log.
                error!(
                    "Not loading ACL policies from {}; error {e} checking for existence",
                    path.display()
                );
                false
            }
        }
    });

    build_acl_provider_from_directories(paths)
}

/// Allow compiling on non SDV and non IVI targets, but crash at runtime because
/// provider has to be defined explicitly.
#[cfg(not(sdv_acl_provider))]
#[cfg(not(ivi_acl_provider))]
#[instrument(name = "authz_perf::provider_factory::create_for_non_sdv", skip_all)]
pub fn create_local_vm_acl_policy_provider() -> anyhow::Result<Arc<dyn SingleVmAclPolicyProvider>> {
    // Check service_authz/local_vm_acl_policy_provider_factory/README.md
    // for more details.
    panic!("ACL policy provider is not specified for this target")
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    #[test]
    fn test_create_local_vm_policy_provider_for_ivi_succeeds_with_missing_directory() {
        let test_dir = TempDir::new();
        let mut nonexistent_path = test_dir.unwrap().path().to_path_buf();
        nonexistent_path.push("nonexistent");

        create_provider_from_directories_lenient(std::iter::once(
            nonexistent_path.to_str().unwrap(),
        ))
        .unwrap();
    }

    #[test]
    fn test_create_local_vm_policy_provider_for_ivi_fails_with_regular_file() {
        let test_dir = TempDir::new().unwrap();
        let mut regular_file_path = test_dir.path().to_path_buf();
        regular_file_path.push("regular_file");
        std::fs::write(regular_file_path.as_path(), "").unwrap();

        assert!(create_provider_from_directories_lenient(std::iter::once(
            regular_file_path.to_str().unwrap(),
        ))
        .is_err());
    }
}
