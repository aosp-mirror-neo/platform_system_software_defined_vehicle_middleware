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

//! Wrappers for feature flags used in Authz.

// Feature flag for enabling permissions-based checks in Authz.
pub fn enable_permissions() -> bool {
    sdv_middleware_flags::enable_authz_permissions()
}

// Compile-time flag for enforcing permissions and ACLs consistency.
//
// Adds sanity checks on top of enforce_permissions_and_acls_consistency():
// - additionally checks that permissions are enabled
// - additionally checks that ACLs are not disabled
pub fn enforce_consistency() -> bool {
    enable_permissions() && !disable_acls() && enforce_permissions_and_acls_consistency()
}

// Internal flag to control permissions and ACLs consistency.
//
// If enabled, Authz will require that permissions- and ACLs-based checks are
// consistent.
//
// To be used for local migration from ACLs to Permissions.
fn enforce_permissions_and_acls_consistency() -> bool {
    false
}

// Compile-time flag for disabling ACLs.
//
// If enabled, Authz will not use ACLs for permission checks.
pub fn disable_acls() -> bool {
    false
}
