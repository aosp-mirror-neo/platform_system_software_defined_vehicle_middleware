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

//! Create remote ACl policy provider from GetPeerLocalAclsResponse proto which
//! contains a remote peer ACL policies.

use std::collections::HashMap;
use std::str::FromStr;

use crate::authz::RemoteVmAclPolicyProvider;

use sdv_authz_policy_proto_rust::acl_policy::AclPolicy as AclPolicyProto;
use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName};
use sdv_sd_mesh_api::sdv_sd_mesh::GetPeerLocalAclsResponse;

use anyhow::{anyhow, Context};
use protobuf::MessageDyn;
use tracing::instrument;

/// Parses GetPeerLocalAclsResponse proto message into a RemoteVmAclPolicyProvider.
#[instrument(name = "authz_perf::mesh::from_peer_local_acls_proto", skip_all, fields(size_bytes=proto.compute_size_dyn()))]
pub(crate) fn from_peer_local_acls_proto(
    proto: GetPeerLocalAclsResponse,
) -> Result<RemoteVmAclPolicyProvider, anyhow::Error> {
    let mut mapping: HashMap<(PackageName, Option<ServiceBundleName>), AclPolicyProto> =
        HashMap::new();

    for mut item in proto.service_bundle_acl_policies {
        let package_name = PackageName::from_str(&item.package_name)
            .with_context(|| "Failed to parse package name")?;
        let service_bundle_name =
            if item.service_bundle_name.is_empty() {
                None
            } else {
                Some(ServiceBundleName::from_str(&item.service_bundle_name).with_context(|| {
                    format!("{package_name}: Failed to parse service bundle name")
                })?)
            };
        let acl_policy = item.acl_policy.take().ok_or_else(|| {
            anyhow!(
                "{}{}: ACL policy field is not specified",
                package_name,
                match service_bundle_name.clone() {
                    Some(name) => format!(".{}", name),
                    None => String::new(),
                }
            )
        })?;

        let key = (package_name.clone(), service_bundle_name.clone());
        if mapping.contains_key(&key) {
            return Err(anyhow!(
                "{}{}: ACL policy declared more than once in the proto message",
                package_name,
                match service_bundle_name.clone() {
                    Some(name) => format!(".{}", name),
                    None => String::new(),
                }
            ));
        }

        mapping.insert(key, acl_policy);
    }

    Ok(RemoteVmAclPolicyProvider::new(mapping))
}
