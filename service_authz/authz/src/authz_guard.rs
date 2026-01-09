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

//! AuthzGuard controls the access to the Service Units and Unit Types.

use sdv_authz_policy::{Acl, ObjectAuthzGuard};
use sdv_comms_id::ServiceFqin;

use crate::authz_result::AuthzResult;

use tracing::instrument;

/// AuthzGuard controls the access to the Service Units and Unit Types.
///
/// Instance of this struct can be created only by Authz struct.
#[derive(Debug)]
pub struct AuthzGuard {
    guard: ObjectAuthzGuard,
}

impl AuthzGuard {
    pub(crate) fn new(acls: &[Acl]) -> Self {
        Self { guard: ObjectAuthzGuard::new(acls) }
    }

    /// Checks if the provided FQIN can access the guarded Object.
    #[instrument(name = "authz_perf::service::authz_guard::is_accessible_to_fqin", skip(self))]
    pub fn is_accessible_to_fqin(&self, fqin: &ServiceFqin) -> AuthzResult {
        let status = self.guard.check_fqin_access(fqin);
        AuthzResult::from(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr;

    fn create_authz_guard() -> AuthzGuard {
        let acls1 = Acl::new(
            &"TirePressureSubs".into(),
            &["vm-cdc:*".into(), "vm-telemetry:*".into()],
            &["vm-ivi:*".into()],
        )
        .unwrap();

        let acls2 = Acl::new(
            &"OemApiClients".into(),
            &["com.google.android.ivi.datatunnel.*".into()],
            &["vm-cdc:*".into()],
        )
        .unwrap();

        AuthzGuard::new(&[acls1, acls2])
    }

    #[test]
    fn test_is_accessible_to_fqin_permitted() {
        let subject_fqin = ServiceFqin::from_str(
            "vm-telemetry:com.google.android.ivi.datatunnel.TunnelService/main",
        )
        .unwrap();

        let authz_result = create_authz_guard().is_accessible_to_fqin(&subject_fqin);
        assert!(
            matches!(authz_result, AuthzResult::Permitted(_)),
            "Unexpected authz result: {authz_result:?}"
        );
    }

    #[test]
    fn test_is_accessible_to_fqin_explicitly_denied() {
        // Explicitly denied by `never_include: vm-cdc:*` even though allowed
        // by `include: vm-cdc:*`.
        let subject_fqin = ServiceFqin::from_str("vm-cdc:com.oem.Ui/center").unwrap();

        let authz_result = create_authz_guard().is_accessible_to_fqin(&subject_fqin);
        assert!(
            matches!(authz_result, AuthzResult::ExplicitlyDenied(_)),
            "Unexpected authz result: {authz_result:?}"
        );
    }

    #[test]
    fn test_is_accessible_to_fqin_implicitly_denied() {
        let subject_fqin = ServiceFqin::from_str("vm:com.sdv.Telemetry/left-right").unwrap();

        let authz_result = create_authz_guard().is_accessible_to_fqin(&subject_fqin);
        assert!(
            matches!(authz_result, AuthzResult::ImplicitlyDenied(_)),
            "Unexpected authz result: {authz_result:?}"
        );
    }
}
