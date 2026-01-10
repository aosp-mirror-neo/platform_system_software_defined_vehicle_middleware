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

//! Types that are related to the Authz integration into the SD.

pub(crate) mod authz_binder_service;
pub(crate) mod authz_filter;
pub(crate) mod mesh_acl_policy_provider;
pub(crate) mod proto;
pub(crate) mod remote_vm_acl_policy_provider;

pub(crate) use authz_binder_service::AuthzBinderService;
pub(crate) use authz_filter::{create_authz_filter_for_sud, AuthzFilterForSud};
pub(crate) use mesh_acl_policy_provider::MeshAclPolicyProvider;
pub(crate) use remote_vm_acl_policy_provider::RemoteVmAclPolicyProvider;
