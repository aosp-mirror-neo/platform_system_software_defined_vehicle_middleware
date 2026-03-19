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

//! Functions to read ACL file mappings from a directory and build a provider from them

use log::error;
use std::fs::DirEntry;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{anyhow, Context};
use tracing::{instrument, trace};

use crate::{AclFileMapping, VmAclFileMapping};

use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName, VmName};

/// Read mappings suitable for adding to a AclFileProviderBuilder a directory.
///
/// The ACL directory is expected to contain subdirectories whose names are package names;
/// these in turn are expected to contain AclPolicy textproto files with names of the form
/// "<Service Bundle Name>.textproto".
#[instrument(name = "authz_perf::provider::read_acl_file_mappings_from_directory")]
pub(crate) fn read_acl_file_mappings_from_directory(
    acl_dir_path: &Path,
) -> anyhow::Result<Vec<AclFileMapping>> {
    if !acl_dir_path.try_exists().with_context(|| {
        format!("Failure while checking existence of ACL directory {}.", acl_dir_path.display())
    })? {
        return Err(anyhow!(
            "Cannot read ACLs from {}: path does not exist.",
            acl_dir_path.display()
        ));
    }

    let acl_dir_metadata = acl_dir_path.metadata().with_context(|| {
        format!(
            "Unable to read metadata for root ACL path {} to confirm it is a directory.",
            acl_dir_path.display()
        )
    })?;
    if !acl_dir_metadata.is_dir() {
        return Err(anyhow!("Cannot read ACLs from {}: not a directory.", acl_dir_path.display()));
    }

    let mut mappings = Vec::new();
    for root_dir_entry_result in acl_dir_path
        .read_dir()
        .with_context(|| format!("Cannot read ACL dir {}", acl_dir_path.display()))?
    {
        let pkg_dir = root_dir_entry_result
            .with_context(|| format!("Cannot get entry from {}.", acl_dir_path.display()))?;
        let pkg_dir_metadata = pkg_dir.path().metadata().with_context(|| {
            format!(
                "Unable to read metadata for package ACL path {} to confirm it is a directory.",
                pkg_dir.path().display()
            )
        })?;
        if !pkg_dir_metadata.is_dir() {
            continue;
        }

        let Some(pkg_name) = package_name_from_dir_entry(&pkg_dir) else {
            continue;
        };

        for pkg_dir_entry_result in pkg_dir
            .path()
            .read_dir()
            .with_context(|| format!("Cannot read pkg ACL subdir {}.", pkg_dir.path().display()))?
        {
            let acl_file = pkg_dir_entry_result
                .with_context(|| format!("Cannot get entry from {}.", pkg_dir.path().display()))?;
            if !acl_file.path().metadata()?.is_file() {
                continue;
            }

            if let Some(bundle_name) = service_bundle_name_from_dir_entry(&acl_file) {
                mappings.push(((pkg_name.clone(), Some(bundle_name)), acl_file.path()));
            }
        }
    }

    trace!(name: "authz_perf::provider::mappings_count", count = mappings.len());

    Ok(mappings)
}

fn package_name_from_dir_entry(de: &DirEntry) -> Option<PackageName> {
    let Ok(file_name) = de.file_name().into_string() else {
        error!(
            "Package ACL dir name {} cannot be converted to string; skipping.",
            de.path().display()
        );
        return None;
    };
    match PackageName::from_str(file_name.as_str()) {
        Ok(package_name) => Some(package_name),
        Err(e) => {
            error!(
                "Package ACL dir name {} is not a valid package name ({e}); skipping.",
                de.path().display(),
            );
            None
        }
    }
}

fn service_bundle_name_from_dir_entry(de: &DirEntry) -> Option<ServiceBundleName> {
    let Ok(file_name) = de.file_name().into_string() else {
        error!(
            "Package ACL dir entry {} cannot be converted to string; skipping.",
            de.path().display()
        );
        return None;
    };
    let Some(stripped_file_name) = file_name.strip_suffix(".textproto") else {
        error!("Package ACL dir entry name {file_name} missing .textproto suffix; skipping.",);
        return None;
    };
    match ServiceBundleName::from_str(stripped_file_name) {
        Ok(bundle_name) => Some(bundle_name),
        Err(e) => {
            error!(
                "ACL file name {} (from {}) is not a valid Service Bundle Name ({}); skipping.",
                stripped_file_name,
                de.path().display(),
                e
            );
            None
        }
    }
}

fn vm_name_from_dir_entry(de: &DirEntry) -> Option<VmName> {
    let Ok(file_name) = de.file_name().into_string() else {
        error!(
            "VM authz policy file name {} cannot be converted to string; skipping.",
            de.path().display()
        );
        return None;
    };
    let Some(stripped_file_name) = file_name.strip_suffix(".textproto") else {
        error!("VM authz policy file name {file_name} missing .textproto suffix; skipping.",);
        return None;
    };
    match VmName::from_str(stripped_file_name) {
        Ok(vm_name) => Some(vm_name),
        Err(e) => {
            error!(
                "VM authz policy file name {} (from {}) is not a valid VM Name ({}); skipping.",
                stripped_file_name,
                de.path().display(),
                e
            );
            None
        }
    }
}

/// Read mappings suitable for adding to a AclFileProviderBuilder.
///
/// The VM authz policy directory is expected to contain VmAuthzPolicy textproto files.
/// The file names are expected to be of the form "<VM Name>.textproto".
pub fn read_vm_file_mappings_from_directory(
    vm_directory: impl AsRef<Path>,
) -> anyhow::Result<(Vec<VmAclFileMapping>, Option<PathBuf>)> {
    let vm_directory = vm_directory.as_ref();
    if !vm_directory.try_exists().with_context(|| {
        format!(
            "Failure while checking existence of VM authz policy directory {}.",
            vm_directory.display()
        )
    })? {
        return Err(anyhow!(
            "Cannot read VM authz policies from {}: path does not exist.",
            vm_directory.display()
        ));
    }

    let vm_directory_metadata = vm_directory.metadata().with_context(|| {
        format!(
            "Unable to read metadata for VM authz policy directory {} to confirm it is a directory.",
            vm_directory.display()
        )
    })?;
    if !vm_directory_metadata.is_dir() {
        return Err(anyhow!(
            "Cannot read VM authz policies from {}: not a directory.",
            vm_directory.display()
        ));
    }

    let mut mappings = Vec::new();
    let mut default_vm_file = None;
    for vm_dir_entry_result in vm_directory.read_dir().with_context(|| {
        format!("Cannot read VM authz policy directory {}", vm_directory.display())
    })? {
        let vm_dir_entry = vm_dir_entry_result
            .with_context(|| format!("Cannot get entry from {}.", vm_directory.display()))?;
        let vm_authz_file_metadata = vm_dir_entry.path().metadata().with_context(|| {
            format!(
                "Unable to read metadata for VM authz policy file {} to confirm it is a file.",
                vm_dir_entry.path().display()
            )
        })?;
        if !vm_authz_file_metadata.is_file() {
            continue;
        }

        if vm_dir_entry.file_name() == ".default.textproto" {
            default_vm_file = Some(vm_dir_entry.path());
        } else if let Some(vm_name) = vm_name_from_dir_entry(&vm_dir_entry) {
            mappings.push((vm_name, vm_dir_entry.path()));
        }
    }

    trace!(name: "authz_perf::provider::vm_mappings_count", count = mappings.len());

    Ok((mappings, default_vm_file))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn add_dir(parent: &Path, child: &str) -> PathBuf {
        let new_path: PathBuf = [parent, Path::new(child)].iter().collect();
        std::fs::create_dir(&new_path).unwrap();
        new_path
    }

    fn add_file(parent: &Path, child: &str) -> PathBuf {
        let new_path: PathBuf = [parent, Path::new(child)].iter().collect();
        std::fs::write(&new_path, "").unwrap(); // Unwrap to panic on error
        new_path
    }

    fn bundle_key(
        package_name: &str,
        bundle_name: &str,
    ) -> (PackageName, Option<ServiceBundleName>) {
        (
            PackageName::from_str(package_name).unwrap(),
            Some(ServiceBundleName::from_str(bundle_name).unwrap()),
        )
    }

    #[test]
    fn test_read_acl_file_mappings_from_directory_success() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();

        let pkg_a_path = add_dir(root_path, "com.apackage");
        let bundle_a_1_path = add_file(pkg_a_path.as_path(), "Bundle1.textproto");
        let bundle_a_2_path = add_file(pkg_a_path.as_path(), "Bundle2.textproto");

        let pkg_b_path = add_dir(root_path, "com.bpackage");
        let bundle_b_3_path = add_file(pkg_b_path.as_path(), "Bundle3.textproto");

        let read_mappings = read_acl_file_mappings_from_directory(root_path).unwrap();

        let actual_mappings =
            HashMap::<(PackageName, Option<ServiceBundleName>), PathBuf>::from_iter(read_mappings);
        assert_eq!(actual_mappings.len(), 3);

        assert_eq!(
            actual_mappings.get(&bundle_key("com.apackage", "Bundle1")),
            Some(&bundle_a_1_path)
        );
        assert_eq!(
            actual_mappings.get(&bundle_key("com.apackage", "Bundle2")),
            Some(&bundle_a_2_path)
        );
        assert_eq!(
            actual_mappings.get(&bundle_key("com.bpackage", "Bundle3")),
            Some(&bundle_b_3_path)
        );
    }

    #[test]
    fn test_read_acl_file_mappings_from_directory_failure_if_nonexistent() {
        assert_eq!(
            read_acl_file_mappings_from_directory(Path::new("/non/existent/directory"))
                .unwrap_err()
                .to_string(),
            "Cannot read ACLs from /non/existent/directory: path does not exist."
        );
    }

    #[test]
    fn test_read_acl_file_mappings_from_directory_failure_if_not_a_directory() {
        let test_tempdir = TempDir::new().unwrap();
        let mut path = test_tempdir.path().to_path_buf();
        path.push("a_plain_file");
        std::fs::write(&path, "").unwrap();

        assert_eq!(
            read_acl_file_mappings_from_directory(&path).unwrap_err().to_string(),
            format!("Cannot read ACLs from {}: not a directory.", path.display())
        );
    }

    #[test]
    fn test_read_acl_file_mappings_from_directory_ignore_bad_package_name() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();

        let pkg_a_path = add_dir(root_path, "_com.apackage");
        add_file(pkg_a_path.as_path(), "Bundle1.textproto");

        let read_mappings = read_acl_file_mappings_from_directory(root_path).unwrap();

        assert_eq!(read_mappings.len(), 0);
    }

    #[test]
    fn test_read_acl_file_mappings_from_directory_ignore_bad_bundle_name() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();

        let pkg_a_path = add_dir(root_path, "com.apackage");
        add_file(pkg_a_path.as_path(), "_Bundle1.textproto");

        let read_mappings = read_acl_file_mappings_from_directory(root_path).unwrap();

        assert_eq!(read_mappings.len(), 0);
    }

    #[test]
    fn test_read_acl_file_mappings_from_directory_ignore_regular_file_in_root() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();

        add_file(root_path, "Bundle1.textproto");

        let read_mappings = read_acl_file_mappings_from_directory(root_path).unwrap();

        assert_eq!(read_mappings.len(), 0);
    }

    #[test]
    fn test_read_acl_file_mappings_from_directory_ignore_dir_under_pkg_dir() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();

        let pkg_a_path = add_dir(root_path, "com.apackage");
        add_dir(pkg_a_path.as_path(), "Bundle1.textproto");

        let read_mappings = read_acl_file_mappings_from_directory(root_path).unwrap();

        assert_eq!(read_mappings.len(), 0);
    }

    #[test]
    fn test_read_acl_file_mappings_from_directory_ignore_non_textproto() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();

        let pkg_a_path = add_dir(root_path, "com.apackage");
        add_dir(pkg_a_path.as_path(), "Bundle1");

        let read_mappings = read_acl_file_mappings_from_directory(root_path).unwrap();
        assert_eq!(read_mappings.len(), 0);
    }

    #[test]
    fn test_read_acl_file_mappings_from_directory_ignore_non_utf8_package_name() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();

        // Create a directory with a non-UTF8 name.
        let invalid_utf8_bytes = b"com.apackage\x80";
        let os_str = OsStr::from_bytes(invalid_utf8_bytes);
        std::fs::create_dir(root_path.join(os_str)).unwrap();

        let read_mappings = read_acl_file_mappings_from_directory(root_path).unwrap();
        assert!(read_mappings.is_empty());
    }

    #[test]
    fn test_read_acl_file_mappings_from_directory_ignore_non_utf8_bundle_name() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();

        let pkg_a_path = add_dir(root_path, "com.apackage");

        // Create a file with a non-UTF8 name.
        let invalid_utf8_bytes = b"Bundle1\x80.textproto";
        let os_str = OsStr::from_bytes(invalid_utf8_bytes);
        std::fs::write(pkg_a_path.join(os_str), "").unwrap();

        let read_mappings = read_acl_file_mappings_from_directory(root_path).unwrap();
        assert!(read_mappings.is_empty());
    }

    #[test]
    fn test_read_vm_file_mappings_from_directory_success() {
        let test_tempdir = TempDir::new().unwrap();
        let root_path = test_tempdir.path();

        let vm_a_path = add_file(root_path, "local-vm.textproto");
        let vm_b_path = add_file(root_path, "another-vm.textproto");
        let default_vm_path = add_file(root_path, ".default.textproto");

        let (read_mappings, default_mapping) =
            read_vm_file_mappings_from_directory(root_path).unwrap();
        let actual_mappings = HashMap::<VmName, PathBuf>::from_iter(read_mappings);

        assert_eq!(actual_mappings.len(), 2);
        assert_eq!(actual_mappings.get(&VmName::from_str("local-vm").unwrap()), Some(&vm_a_path));
        assert_eq!(actual_mappings.get(&VmName::from_str("another-vm").unwrap()), Some(&vm_b_path));
        assert_eq!(default_mapping, Some(default_vm_path));
    }
}
