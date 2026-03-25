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

//! Crate for ACL Policy, ACL Provider, AuthzGuard.

mod acl_policy_model;
mod acl_policy_provider;
mod authz_enforcement_flag;
mod authz_guard;
mod authz_result;
mod authz_service;
mod authz_via_acls;
mod authz_via_permissions;
mod local_vm_authz_policy_provider;
mod multi_vm_authz_guard;
mod parse_acl_policy_proto;
mod single_vm_acl_policy_provider;
#[cfg(test)]
mod test_acl_policy_provider;
mod vm_authz_policy_model;

pub use acl_policy_model::AclPolicyModel;
pub use acl_policy_provider::AclPolicyProvider;
pub use authz_enforcement_flag::{AuthzEnforcementFlag, AuthzStrategy};
pub use authz_service::AuthzService;
pub use authz_service::ServiceUnitRegistry;
pub use local_vm_authz_policy_provider::LocallyStoredAuthzPoliciesProvider;
pub use local_vm_authz_policy_provider::VmLevelPermissionsProvider;
pub use single_vm_acl_policy_provider::SingleVmAclPolicyProvider;
pub use vm_authz_policy_model::VmAuthzPolicyModel;

use authz_guard::AuthzGuard;
use authz_result::AuthzResult;
#[cfg(test)]
use test_acl_policy_provider::TestAclPolicyProvider;
