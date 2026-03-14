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

use crate::{AclPolicyModel, VmAuthzPolicyModel};

use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName, VmName};

use std::sync::Arc;

/// Trait to retrieve the ACL Policy for a given Service Bundle.
pub trait AclPolicyProvider: Send + Sync {
    /// Gets the ACL policy for a package or a service bundle.
    fn get_acl_policy(
        &self,
        vm_name: &VmName,
        package_name: &PackageName,
        service_bundle_name: Option<&ServiceBundleName>,
    ) -> Result<Arc<AclPolicyModel>, anyhow::Error>;

    /// Gets the Authz policy for a given remote VM.
    ///
    /// This method retrieves the Authz policy for a remote VM.
    /// The policy is stored on the local VM and can be used to check
    /// authorization for services running on the remote VM.
    fn get_vm_authz_policy(
        &self,
        vm_name: &VmName,
    ) -> Result<Arc<VmAuthzPolicyModel>, anyhow::Error>;

    /// Gets a list of VM Names on which ACL policy could be queried.
    fn get_available_vm_names(&self) -> Vec<VmName>;
}
