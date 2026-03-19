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

use crate::{AclPolicyModel, AclPolicyProvider, VmAuthzPolicyModel};

use anyhow::anyhow;
use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName, VmName};
use std::collections::HashMap;
use std::sync::Arc;

pub struct TestAclPolicyProvider {
    mapping: HashMap<(VmName, PackageName, Option<ServiceBundleName>), AclPolicyModel>,
    vm_mapping: HashMap<VmName, Arc<VmAuthzPolicyModel>>,
    default_vm_authz_policy: Option<Arc<VmAuthzPolicyModel>>,
}

impl TestAclPolicyProvider {
    pub fn new() -> Self {
        Self { mapping: HashMap::new(), vm_mapping: HashMap::new(), default_vm_authz_policy: None }
    }

    pub fn set_authz_policy(
        &mut self,
        vm_name: VmName,
        package_name: PackageName,
        service_name: Option<ServiceBundleName>,
        acl_policy: AclPolicyModel,
    ) {
        self.mapping.insert(
            (vm_name.clone(), package_name.clone(), service_name.clone()),
            acl_policy.clone(),
        );
        if service_name.is_some() {
            self.mapping.insert((vm_name, package_name, None), acl_policy);
        }
    }

    pub fn set_vm_authz_policy(&mut self, vm_name: VmName, vm_authz_policy: VmAuthzPolicyModel) {
        self.vm_mapping.insert(vm_name, Arc::new(vm_authz_policy));
    }
}

impl AclPolicyProvider for TestAclPolicyProvider {
    fn get_acl_policy(
        &self,
        vm_name: &VmName,
        package_name: &PackageName,
        service_bundle_name: Option<&ServiceBundleName>,
    ) -> Result<Arc<AclPolicyModel>, anyhow::Error> {
        self.mapping
            .get(&(vm_name.clone(), package_name.clone(), service_bundle_name.cloned()))
            .ok_or(anyhow!(
                "ACLs for {:?}:{}.{:?} are not specified",
                vm_name,
                package_name,
                service_bundle_name
            ))
            .cloned()
            .map(Arc::new)
    }

    fn get_available_vm_names(&self) -> Vec<VmName> {
        let mut vm_names: Vec<VmName> = self.mapping.keys().map(|k| k.0.clone()).collect();
        vm_names.sort_by_key(|vm_name| vm_name.name().to_string());
        vm_names
    }

    fn get_vm_authz_policy(
        &self,
        vm_name: &VmName,
    ) -> Result<Arc<crate::VmAuthzPolicyModel>, anyhow::Error> {
        self.vm_mapping
            .get(vm_name)
            .or(self.default_vm_authz_policy.as_ref())
            .ok_or(anyhow!("VM Authz policy for VM '{}' is not specified", vm_name))
            .cloned()
    }
}
