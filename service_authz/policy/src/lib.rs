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

//! Combines SDV Service Authorization ACL policy related files into a single module.

mod acl_name;
mod acl_policy;
mod acl_policy_error;
mod acl_statement_str;
mod constant_value;
mod fqin_set;
mod fqin_statement;
mod instance_name_part;
mod package_name_part;
mod package_namespace;
mod selector;
mod selector_or_constant_value;
mod service_name_part;
mod vm_name_part;

pub use acl_name::AclName;
pub use acl_policy::{AccessState, Acl, ObjectAuthzGuard};
pub use acl_policy_error::ParseAclsError;
pub use acl_statement_str::AclStatementStr;

// TODO(b/341881974): consider not exposing these errors.
pub use instance_name_part::InstanceNamePartParsingError;
pub use package_name_part::PackageNamePartParsingError;
pub use service_name_part::ServiceNamePartParsingError;
pub use vm_name_part::VmNamePartParsingError;
