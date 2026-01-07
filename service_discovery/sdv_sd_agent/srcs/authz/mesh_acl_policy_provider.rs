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

//! Mesh ACL policy provider.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use sdv_authz::{AclPolicyModel, AclPolicyProvider, SingleVmAclPolicyProvider};
use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName, VmName};

use anyhow::anyhow;
use tracing::instrument;

pub(crate) struct MeshAclPolicyProvider {
    local_vm_name: VmName,
    local_vm_acl_policy_provider: Arc<dyn SingleVmAclPolicyProvider>,
    // This hashmap should not have a provider for a local VM.
    remote_vm_acl_policy_providers: Mutex<HashMap<VmName, Box<dyn SingleVmAclPolicyProvider>>>,
}

impl MeshAclPolicyProvider {
    /// Creates new MeshAclPolicyProvider.
    pub fn new(
        local_vm_name: VmName,
        local_vm_acl_policy_provider: Arc<dyn SingleVmAclPolicyProvider>,
    ) -> Self {
        let remote_vm_acl_policy_providers = Mutex::new(HashMap::new());

        Self { local_vm_name, local_vm_acl_policy_provider, remote_vm_acl_policy_providers }
    }

    /// Sets a remote ACL policy provider for a `vm_name`.
    #[instrument(
        name = "authz_perf::mesh_provider::set_remote_acl_policy_provider",
        skip(self, provider)
    )]
    pub fn set_remote_acl_policy_provider(
        &self,
        vm_name: VmName,
        provider: Box<dyn SingleVmAclPolicyProvider>,
    ) {
        let mut remote_providers = self.remote_vm_acl_policy_providers.lock().unwrap();

        remote_providers.insert(vm_name, provider);
    }

    /// Removes a remote ACL policy provider for a `vm_name`.
    #[instrument(name = "authz_perf::mesh_provider::remove_remote_acl_policy_provider", skip(self))]
    pub fn remove_remote_acl_policy_provider(&self, vm_name: &VmName) {
        let mut remote_providers = self.remote_vm_acl_policy_providers.lock().unwrap();

        remote_providers.remove(vm_name);
    }
}

impl AclPolicyProvider for MeshAclPolicyProvider {
    #[instrument(name = "authz_perf::mesh_provider::get_acl_policy", skip_all)]
    fn get_acl_policy(
        &self,
        vm_name: &VmName,
        package_name: &PackageName,
        service_bundle_name: Option<&ServiceBundleName>,
    ) -> Result<Arc<AclPolicyModel>, anyhow::Error> {
        if *vm_name == self.local_vm_name {
            return self
                .local_vm_acl_policy_provider
                .get_acl_policy(package_name, service_bundle_name);
        }

        let remote_providers = self.remote_vm_acl_policy_providers.lock().unwrap();

        let remote_provider = remote_providers
            .get(vm_name)
            .ok_or_else(|| anyhow!("No remote ACL policy provider for {:?}", vm_name))?;

        remote_provider.get_acl_policy(package_name, service_bundle_name)
    }

    fn get_available_vm_names(&self) -> Vec<VmName> {
        let mut vm_names = vec![self.local_vm_name.clone()];

        for (vm_name, _) in self.remote_vm_acl_policy_providers.lock().unwrap().iter() {
            vm_names.push(vm_name.clone());
        }

        vm_names
    }
}
