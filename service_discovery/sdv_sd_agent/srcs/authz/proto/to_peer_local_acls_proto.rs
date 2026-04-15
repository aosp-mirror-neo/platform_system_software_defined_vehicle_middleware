// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Uses local VM ACL policy provider to create a Vec<ServiceBundleAclPolicy>.

use crate::MeshAclPolicyProvider;

use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
use sdv_sd_mesh_api::sdv_sd_mesh::ServiceBundleAclPolicy;

use anyhow::{anyhow, Context};
use protobuf::MessageDyn;
use protobuf::MessageField;
use tracing::{instrument, trace};

/// Uses SingleVmAclPolicyProvider to create Vec<ServiceBundleAclPolicy>.
#[instrument(name = "authz_perf::mesh::to_peer_local_acls_proto", skip_all)]
pub(crate) fn to_peer_local_acls_proto(
    provider: &MeshAclPolicyProvider,
) -> Result<Vec<ServiceBundleAclPolicy>, anyhow::Error> {
    let mut result: Vec<ServiceBundleAclPolicy> = Vec::new();
    let provider = provider.get_local_vm_acl_policy_provider();

    let subjects = provider
        .get_namespace_with_acl_policy()
        .with_context(|| "Failed to get local subjects with specified ACL policy")?;

    for (package_name, service_bundle_name) in &subjects {
        let acl_policy = provider
            .get_acl_policy_proto(package_name, service_bundle_name.as_ref())
            .with_context(|| {
                anyhow!(
                    "{}{}: Failed to get ACL policy",
                    package_name,
                    match service_bundle_name {
                        Some(name) => format!(".{}", name),
                        None => String::new(),
                    }
                )
            })?;

        let item = ServiceBundleAclPolicy {
            package_name: package_name.name().to_string(),
            service_bundle_name: service_bundle_name
                .as_ref()
                .map_or("".to_string(), |s| s.name().to_string()),
            acl_policy: MessageField::<AclPolicyProto>::some(acl_policy),
            ..Default::default()
        };

        result.push(item);
    }

    trace!(
        name: "authz_perf::provider::get_peer_local_acls_response_proto",
        size_bytes = result.iter().fold(0, |acc, item| acc + item.compute_size_dyn()),
        subjects_with_acls_count = subjects.len()
    );

    Ok(result)
}
