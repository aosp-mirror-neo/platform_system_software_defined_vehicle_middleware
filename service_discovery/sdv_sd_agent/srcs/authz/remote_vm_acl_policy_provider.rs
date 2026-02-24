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

//! Remote ACL policy provider is responsible for providing ACL policy on
//! a remote VM.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use sdv_authz::{AclPolicyModel, SingleVmAclPolicyProvider};
use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName};

use anyhow::{anyhow, Context};
use tracing::instrument;

pub(crate) struct RemoteVmAclPolicyProvider {
    mapping: HashMap<(PackageName, Option<ServiceBundleName>), AclPolicyProto>,
    model_cache: Mutex<HashMap<(PackageName, Option<ServiceBundleName>), Arc<AclPolicyModel>>>,
}

impl RemoteVmAclPolicyProvider {
    pub fn new(mapping: HashMap<(PackageName, Option<ServiceBundleName>), AclPolicyProto>) -> Self {
        RemoteVmAclPolicyProvider { mapping, model_cache: Mutex::new(HashMap::new()) }
    }
}

impl SingleVmAclPolicyProvider for RemoteVmAclPolicyProvider {
    /// Gets the package or Service Bundle ACL policy proto.
    #[instrument(name = "authz_perf::remote_provider::get_acl_policy_proto", skip_all)]
    fn get_acl_policy_proto(
        &self,
        package_name: &PackageName,
        service_bundle_name: Option<&ServiceBundleName>,
    ) -> Result<AclPolicyProto, anyhow::Error> {
        self.mapping.get(&(package_name.clone(), service_bundle_name.cloned())).cloned().ok_or_else(
            || {
                anyhow!(
                    "{}{}: ACL policy is not found",
                    package_name,
                    match service_bundle_name {
                        Some(name) => format!(".{}", name),
                        None => String::new(),
                    }
                )
            },
        )
    }

    /// Gets the Service Bundle ACL policy.
    ///
    /// If the ACL policy is already cached, returns it from the cache.
    /// Otherwise, gets the ACL policy proto, converts it to a model, caches it,
    /// and returns it.
    fn get_acl_policy(
        &self,
        package_name: &PackageName,
        service_bundle_name: Option<&ServiceBundleName>,
    ) -> Result<Arc<AclPolicyModel>, anyhow::Error> {
        {
            let model_cache = self.model_cache.lock().unwrap();
            if let Some(acl_policy) =
                model_cache.get(&(package_name.clone(), service_bundle_name.cloned()))
            {
                return Ok(acl_policy.clone());
            }
        }

        let acl_policy_proto = self.get_acl_policy_proto(package_name, service_bundle_name)?;

        let acl_policy_model =
            AclPolicyModel::try_from(acl_policy_proto).with_context(
                || match service_bundle_name {
                    Some(service_bundle_name) => format!(
                    "{package_name}.{service_bundle_name}: Failed to parse ACL policy from proto"
                ),
                    None => format!("{package_name}: Failed to parse ACL policy from proto"),
                },
            )?;

        let acl_policy_model = Arc::new(acl_policy_model);

        {
            let mut model_cache = self.model_cache.lock().unwrap();
            model_cache.insert(
                (package_name.clone(), service_bundle_name.cloned()),
                acl_policy_model.clone(),
            );
        }

        Ok(acl_policy_model)
    }

    /// Returns a vector of (Package Name, Service Bundle Name) pairs
    /// representing the service bundles available on the local VM with
    /// available ACL policy.
    fn get_namespace_with_acl_policy(
        &self,
    ) -> Result<Vec<(PackageName, Option<ServiceBundleName>)>, anyhow::Error> {
        Ok(self.mapping.keys().cloned().collect())
    }
}
