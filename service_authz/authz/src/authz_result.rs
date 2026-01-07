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

//! Represents the result of an authorization operation.

use sdv_authz_policy::AccessState;

/// Represents the result of an authorization operation.
#[derive(Debug, PartialEq)]
pub enum AuthzResult {
    /// Explicitly allowed and not forbidden, includes a debug explanation.
    Permitted(String),
    /// Explicitly forbidden, includes a debug explanation.
    ExplicitlyDenied(String),
    /// Neither explicitly permitted nor denied (default deny), includes a debug explanation.
    ImplicitlyDenied(String),
}

impl From<AccessState> for AuthzResult {
    fn from(state: AccessState) -> Self {
        match state {
            AccessState::Permitted(acl_name, statement) => AuthzResult::Permitted(format!(
                "Authorization is permitted by ACL {acl_name:?} include statement {statement:?}"
            )),
            AccessState::ExplicitlyDenied(acl_name, statement) => {
                AuthzResult::ExplicitlyDenied(format!(
                    "Authorization is denied by ACL {acl_name:?} never_include statement {statement:?}"
                ))
            },
            AccessState::ImplicitlyDenied => AuthzResult::ImplicitlyDenied(
                "Authorization is denied implicitly because provided ACLs neither permit nor deny authorization explicitly".to_string()
            ),
        }
    }
}
