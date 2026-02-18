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

//! SDV Local VM ACL Policy Provider.

use log::{debug, error};
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Context;
use tracing::{instrument, trace_span};

use google_sdv_service_bundles_registry::aidl::google::sdv::service_bundles_registry::IRegistry::{
    BpRegistry as BpServiceBundlesRegistry, IRegistry as IServiceBundlesRegistry
};
use google_sdv_service_bundles_registry::aidl::google::sdv::service_bundles_registry::ServiceBundleMetadata::ServiceBundleMetadata;
use sdv_authz_acl_file_provider::{AclFileMapping, AclFileProvider};
use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName};

/// SDV Local VM ACL policy provider.

#[instrument(name = "authz_perf::provider::create_from_sbr", skip_all)]
pub fn create_acl_provider_from_service_bundle_registry() -> anyhow::Result<AclFileProvider> {
    let mappings = read_acl_file_mappings_from_service_bundle_registry()
        .with_context(|| "Failure while reading ACL file mappings from Service Bundle Registry")?;

    AclFileProvider::new(mappings.into_iter())
        .with_context(|| "Error adding ACL mappings read from Service Bundle Registry.")
}

/// Read mappings from (package name, service bundle name) to ACL policy file from SBR.
fn read_acl_file_mappings_from_service_bundle_registry() -> anyhow::Result<Vec<AclFileMapping>> {
    let registry =
        get_service_bundles_registry().with_context(|| "Failed to get Service Bundles Registry")?;

    let span =
        trace_span!("authz_perf::provider::get_available_service_bundles_metadata").entered();
    let available_service_bundles = registry
        .getAvailableServiceBundlesMetadata()
        .with_context(|| "Failed to get available Service Bundles Metadata")?;
    drop(span);

    let mappings =
        available_service_bundles.iter().filter_map(parse_service_bundle_metadata).collect();

    Ok(mappings)
}

/// Parses Service Bundle Metadata and returns tuple of Package Name,
/// Service Bundle Name and ACL policy path.
fn parse_service_bundle_metadata(service_bundle: &ServiceBundleMetadata) -> Option<AclFileMapping> {
    let path = match &service_bundle.authorizationPolicyPath {
        Some(path) => PathBuf::from(path),
        None => {
            debug!(
                "{}.{}: ACL policy path is not specified",
                service_bundle.packageName, service_bundle.name
            );
            return None;
        }
    };

    let package_name = match PackageName::from_str(&service_bundle.packageName) {
        Ok(package_name) => package_name,
        Err(err) => {
            error!(
                "{}.{}: ACL policy ignored due to invalid Package Name: {}",
                service_bundle.packageName, service_bundle.name, err
            );
            return None;
        }
    };

    let service_bundle_name = match ServiceBundleName::from_str(&service_bundle.name) {
        Ok(service_bundle_name) => service_bundle_name,
        Err(err) => {
            error!(
                "{}.{}: ACL policy ignored due to invalid Service Bundle Name: {}",
                service_bundle.packageName, service_bundle.name, err
            );
            return None;
        }
    };

    debug!(
        "{}.{}: ACL policy path is specified as {:?}",
        service_bundle.packageName, service_bundle.name, path
    );

    Some(((package_name, Some(service_bundle_name)), path))
}

/// Gets service bundles registry handler.
#[instrument(name = "authz_perf::provider::get_service_bundles_registry", skip_all)]
fn get_service_bundles_registry(
) -> Result<binder::Strong<dyn IServiceBundlesRegistry>, anyhow::Error> {
    let descriptor = <BpServiceBundlesRegistry as IServiceBundlesRegistry>::get_descriptor()
        .to_owned()
        + "/default";
    binder::wait_for_interface::<dyn IServiceBundlesRegistry>(&descriptor)
        .with_context(|| "Failed to find interface ServiceBundlesRegistry")
}

#[cfg(test)]
mod tests {
    use super::*;
    use google_sdv_service_bundles_registry::aidl::google::sdv::service_bundles_registry::ServiceBundleMetadata::ServiceBundleMetadata;

    const VALID_PACKAGE_NAME: &str = "com.google.test";
    const VALID_SERVICE_BUNDLE_NAME: &str = "TestBundle";
    const VALID_ACL_PATH: &str = "/path/to/acl";

    #[test]
    fn parse_metadata_no_acl_path_returns_none() {
        let metadata = ServiceBundleMetadata {
            packageName: VALID_PACKAGE_NAME.to_string(),
            name: VALID_SERVICE_BUNDLE_NAME.to_string(),
            authorizationPolicyPath: None,
            ..Default::default()
        };

        assert!(parse_service_bundle_metadata(&metadata).is_none());
    }

    #[test]
    fn parse_metadata_invalid_package_name_returns_none() {
        let metadata = ServiceBundleMetadata {
            packageName: "Invalid-Package-Name".to_string(),
            name: VALID_SERVICE_BUNDLE_NAME.to_string(),
            authorizationPolicyPath: Some(VALID_ACL_PATH.to_string()),
            ..Default::default()
        };

        assert!(parse_service_bundle_metadata(&metadata).is_none());
    }

    #[test]
    fn parse_metadata_invalid_service_bundle_name_returns_none() {
        let metadata = ServiceBundleMetadata {
            packageName: VALID_PACKAGE_NAME.to_string(),
            name: "Invalid Bundle Name".to_string(),
            authorizationPolicyPath: Some(VALID_ACL_PATH.to_string()),
            ..Default::default()
        };

        assert!(parse_service_bundle_metadata(&metadata).is_none());
    }
}
