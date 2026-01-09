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

use sdv_authz::AuthzEnforcementFlag;
use sdv_authz_enforcement_flag::SystemPropertyAuthzEnforcementFlag;

/// Verifies that Authz enforcement flag is turned on when a designated test
/// template config is applied which turns on authz enforcement flag
/// before test run and turns it off after the test run.
#[test]
fn authz_enforcement_flag_enabled_explicitly_by_config_template() {
    assert!(SystemPropertyAuthzEnforcementFlag::new().is_enabled());
}
