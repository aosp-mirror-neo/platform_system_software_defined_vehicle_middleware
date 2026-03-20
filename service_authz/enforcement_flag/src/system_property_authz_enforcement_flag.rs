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

//! Authz enforcement flag implementation which uses Android system property.

use anyhow::anyhow;
use log::error;
use rustutils::android::system_properties::read;
use tracing::{instrument, trace_span};

use sdv_authz::{AuthzEnforcementFlag, AuthzStrategy};

const AUTHZ_ENFORCEMENT_FLAG: &str = "sdv.authz.enable";

// The SDV Comms Stack is running on the vendor partition on Android 15,
// thus the property is different there.
const AUTHZ_ENFORCEMENT_FLAG_A15: &str = "vendor.sdv.authz.enable";

/// System property based implementation of AuthzEnforcementFlag trait.
pub struct SystemPropertyAuthzEnforcementFlag {
    property_path: &'static str,
}

impl SystemPropertyAuthzEnforcementFlag {
    /// Creates new instance of SystemPropertyAuthzEnforcementFlag.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { property_path: AUTHZ_ENFORCEMENT_FLAG }
    }

    /// Creates new instance of SystemPropertyAuthzEnforcementFlag for Android 15.
    #[allow(clippy::new_without_default)]
    pub fn new_for_a15() -> Self {
        Self { property_path: AUTHZ_ENFORCEMENT_FLAG_A15 }
    }

    fn property_path(&self) -> &'static str {
        self.property_path
    }
}

impl AuthzEnforcementFlag for SystemPropertyAuthzEnforcementFlag {
    #[instrument(name = "authz_perf::flag::get_enforcement_strategy", skip_all)]
    fn get_enforcement_strategy(&self) -> AuthzStrategy {
        get_enforcement_strategy_from_property(self.property_path())
    }
}

/// Returns authz enforcement strategy read from `sdv.authz.enable` system property.
///
/// Returns `AuthzStrategy::Lenient` if flag is not set correctly, or error during reading system
/// property, or property does not exist, or failed to determine whether the build type is "user".
fn get_enforcement_strategy_from_property(property_path: &str) -> AuthzStrategy {
    const DEFAULT_ENFORCEMENT_STRATEGY: AuthzStrategy = AuthzStrategy::Lenient;

    match is_user_build() {
        Ok(is_user_build) => {
            if is_user_build {
                // For `user` build we should always return DEFAULT_ENFORCEMENT_STRATEGY regardless of `sdv.authz.enable` value.
                return DEFAULT_ENFORCEMENT_STRATEGY;
            }
        }
        Err(err) => {
            error!(
                r#"Failed to determine whether build type is "user": {err:?}. Defaulting to {DEFAULT_ENFORCEMENT_STRATEGY:?}"#
            );
            return DEFAULT_ENFORCEMENT_STRATEGY;
        }
    }

    let _span = trace_span!("authz_perf::flag::is_enabled_by_system_property").entered();

    match read(property_path) {
        Ok(Some(value)) => match value.as_str() {
            // Property values mapped to AuthzStrategy enum
            "acls_only" => AuthzStrategy::AclsOnly,
            "permissions_only" => AuthzStrategy::PermissionsOnly,
            "lenient" => AuthzStrategy::Lenient,
            "strict" => AuthzStrategy::Strict,
            "disabled" => AuthzStrategy::Disabled,
            // Legacy property values
            "true" => AuthzStrategy::Lenient,
            "false" => AuthzStrategy::Disabled,
            _ => {
                error!(
                    r#"{property_path:?} system property has unexpected value: {value:?}. Defaulting to {DEFAULT_ENFORCEMENT_STRATEGY:?}"#
                );
                DEFAULT_ENFORCEMENT_STRATEGY
            }
        },
        Ok(None) => {
            error!(
                r#"{property_path:?} system property does not exist. Defaulting to {DEFAULT_ENFORCEMENT_STRATEGY:?}"#
            );
            DEFAULT_ENFORCEMENT_STRATEGY
        }
        Err(err) => {
            error!(
                r#"Error during reading {property_path:?} system property: {err:?}. Defaulting to {DEFAULT_ENFORCEMENT_STRATEGY:?}"#
            );
            DEFAULT_ENFORCEMENT_STRATEGY
        }
    }
}

const RO_BUILD_TYPE_SYSTEM_PROPERTY: &str = "ro.build.type";

/// Returns whether build type is `user`.
#[instrument(name = "authz_perf::flag::is_user_build", skip_all)]
fn is_user_build() -> Result<bool, anyhow::Error> {
    match read(RO_BUILD_TYPE_SYSTEM_PROPERTY) {
        Ok(Some(value)) => match value.as_str() {
            "user" => Ok(true),
            _ => Ok(false),
        },
        Ok(None) => {
            Err(anyhow!("{:?} system property does not exist", RO_BUILD_TYPE_SYSTEM_PROPERTY))
        }
        Err(err) => Err(anyhow::Error::new(err).context(format!(
            "Error during reading {RO_BUILD_TYPE_SYSTEM_PROPERTY:?} system property"
        ))),
    }
}
