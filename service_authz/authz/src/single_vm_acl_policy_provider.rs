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

//! ACL Policy Provider trait used to retrieve ACLs related information.

use crate::AclPolicyModel;

use sdv_authz_policy_proto_rust::acl_policy::AclPolicy as AclPolicyProto;
use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName};

use anyhow::Context;

use std::sync::Arc;

/// Trait to retrieve the ACL Policy for a given Service Bundle within
/// a single VM.
pub trait SingleVmAclPolicyProvider: Send + Sync {
    /// Gets the Service Bundle / package ACL policy proto.
    fn get_acl_policy_proto(
        &self,
        package_name: &PackageName,
        service_bundle_name: Option<&ServiceBundleName>,
    ) -> Result<AclPolicyProto, anyhow::Error>;

    /// Returns a vector of (Package Name, Service Bundle Name) pairs
    /// representing the service bundles / packages available on the
    /// local VM with available ACL policy.
    fn get_namespace_with_acl_policy(
        &self,
    ) -> Result<Vec<(PackageName, Option<ServiceBundleName>)>, anyhow::Error>;

    /// Gets an ACL policy.
    fn get_acl_policy(
        &self,
        package_name: &PackageName,
        service_bundle_name: Option<&ServiceBundleName>,
    ) -> Result<Arc<AclPolicyModel>, anyhow::Error> {
        let proto = self.get_acl_policy_proto(package_name, service_bundle_name)?;

        AclPolicyModel::try_from(proto)
            .with_context(|| {
                format!(
                    "{package_name}{}: Failed to parse ACL policy from proto",
                    match service_bundle_name {
                        Some(name) => format!(".{}", name),
                        None => String::new(),
                    }
                )
            })
            .map(Arc::new)
    }
}
