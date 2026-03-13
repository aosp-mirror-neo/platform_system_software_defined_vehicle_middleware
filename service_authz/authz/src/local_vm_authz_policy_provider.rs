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

use crate::SingleVmAclPolicyProvider;
use crate::VmAuthzPolicyModel;

use sdv_authz_policy_proto_rust::vm_authz_policy::VmAuthzPolicy as VmAuthzPolicyProto;
use sdv_comms_id::names::VmName;

use anyhow::Context;
use std::sync::Arc;

/// Trait to retrieve the VM-level permissions.
pub trait VmLevelPermissionsProvider: Send + Sync {
    /// Gets the VM Authz policy proto.
    fn get_vm_authz_policy_proto(
        &self,
        vm_name: &VmName,
    ) -> Result<VmAuthzPolicyProto, anyhow::Error>;

    /// Gets the VM-level permissions for a given remote VM.
    ///
    /// This method retrieves the VM-level permissions for a remote VM.
    /// The permissions are stored on the local VM and can be used to check
    /// authorization for services running on the remote VM.
    fn get_vm_authz_policy(
        &self,
        vm_name: &VmName,
    ) -> Result<Arc<VmAuthzPolicyModel>, anyhow::Error> {
        let proto = self.get_vm_authz_policy_proto(vm_name)?;

        VmAuthzPolicyModel::try_from(proto)
            .with_context(|| format!("{vm_name}: Failed to parse VM Authz policy from proto"))
            .map(Arc::new)
    }
}

/// Trait to retrieve all locally stored Authz-data,
/// in particular application-level ACLs/permissions and VM-level permissions.
pub trait LocallyStoredAuthzPoliciesProvider:
    VmLevelPermissionsProvider + SingleVmAclPolicyProvider + Send + Sync
{
}
