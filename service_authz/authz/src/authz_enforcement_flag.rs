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

//! Authz enforcement flag trait that will be used to control authz service behavior.

/// Trait to retrieve the Authz enforcement strategy.
///
/// # For testing purposes, simply create your own testing implementation:
/// ```
/// use crate::{AuthzEnforcementFlag, AuthzStrategy};
///
/// struct TestAuthzEnforcementFlag;
///
/// impl AuthzEnforcementFlag for TestAuthzEnforcementFlag {
///   fn get_enforcement_strategy(&self) -> AuthzStrategy { AuthzStrategy::Strict }
/// }
/// ```
pub trait AuthzEnforcementFlag: Send + Sync {
    /// Returns exact Authz strategy to enforce.
    fn get_enforcement_strategy(&self) -> AuthzStrategy;
}

/// Authz enforcement strategy.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum AuthzStrategy {
    /// Only enforce permissions.
    PermissionsOnly,
    /// Enforce both permissions and ACLs, but allow access if either one permits.
    #[default]
    Lenient,
    /// Enforce both permissions and ACLs, but deny access if either one denies.
    Strict,
    /// Only enforce ACLs.
    AclsOnly,
    /// Disable Authz.
    Disabled,
}
