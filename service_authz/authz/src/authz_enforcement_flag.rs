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

/// Trait to retrieve the Authz enforcement flag value.
///
/// # For testing purposes, simply create your own testing implementation:
/// ```
/// struct TestAuthzEnforcementFlagEnabled;
///
/// impl AuthzEnforcementFlag for TestAuthzEnforcementFlagEnabled {
///   fn is_enabled(&self) -> bool { true }
/// }
/// ```
pub trait AuthzEnforcementFlag: Send + Sync {
    /// Returns whether Authz enforcement flag is enabled.
    fn is_enabled(&self) -> bool;
}
