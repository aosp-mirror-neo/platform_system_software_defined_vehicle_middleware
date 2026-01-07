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

use crate::{authz_result::AuthzResult, AuthzGuard};

use std::collections::BTreeMap;

use sdv_comms_id::{names::VmName, ServiceFqin};

use tracing::instrument;

/// MultiVmAuthzGuard controls the access to the Service Units and Unit Types.
///
/// Instance of this struct can be created only by Authz struct.
#[derive(Debug)]
pub struct MultiVmAuthzGuard {
    guards_map: BTreeMap<VmName, AuthzGuard>,
}

impl MultiVmAuthzGuard {
    pub(crate) fn new(guards_map: BTreeMap<VmName, AuthzGuard>) -> Self {
        Self { guards_map }
    }

    /// Checks if the provided FQIN can access the guarded Object.
    #[instrument(
        name = "authz_perf::service::multi_vm_authz_guard::is_accessible_to_fqin",
        skip(self)
    )]
    pub fn is_accessible_to_fqin(&self, fqin: &ServiceFqin) -> AuthzResult {
        let mut permitted_audit_msgs: Vec<(VmName, String)> = vec![];
        for (vm_name, authz_guard) in &self.guards_map {
            match authz_guard.is_accessible_to_fqin(fqin) {
                AuthzResult::Permitted(audit_msg) => {
                    // Gather audit messages from all VMs which permit access.
                    permitted_audit_msgs.push((vm_name.clone(), audit_msg));
                }
                AuthzResult::ExplicitlyDenied(audit_msg) => {
                    // Authz is explicitly denied if at least one VM explicitly denies access.
                    return AuthzResult::ExplicitlyDenied(format!(
                        "Explicitly denied by {vm_name:?}: {audit_msg}"
                    ));
                }
                AuthzResult::ImplicitlyDenied(_) => {
                    // Neither explicitly permitted nor denied, thus continue.
                    continue;
                }
            }
        }

        if permitted_audit_msgs.is_empty() {
            let vm_names: Vec<VmName> = self.guards_map.keys().cloned().collect();
            return AuthzResult::ImplicitlyDenied(format!(
                "No ACL found that permit access among VMs: {vm_names:?}"
            ));
        }

        let mut result_audit_msg: String = "Explicitly permitted by:\n".into();
        for (vm_name, audit_msg) in permitted_audit_msgs {
            result_audit_msg.push_str(&format!(
                "- {:?} VM with audit message: {}\n",
                vm_name.name(),
                audit_msg
            ));
        }
        AuthzResult::Permitted(result_audit_msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use crate::AuthzResult;
    use sdv_authz_policy::Acl;
    use sdv_comms_id::names::VmName;

    fn create_authz_guard(includes: &[&str], never_includes: &[&str]) -> AuthzGuard {
        let acl = {
            let includes: Vec<String> = includes.iter().map(|s| s.to_string()).collect();
            let never_includes: Vec<String> =
                never_includes.iter().map(|s| s.to_string()).collect();
            Acl::new(&"TestAcl".into(), &includes, &never_includes).unwrap()
        };
        AuthzGuard::new(&[acl])
    }

    #[test]
    fn test_is_accessible_to_fqin_permitted() {
        let subject_fqin = ServiceFqin::from_str("vm-test:com.google.Car/main").unwrap();

        let guards_map = BTreeMap::from([
            (VmName::from_str("vm1").unwrap(), create_authz_guard(&["vm-test:*"], &[])),
            (VmName::from_str("vm2").unwrap(), create_authz_guard(&["*"], &[])),
        ]);

        let authz_result = MultiVmAuthzGuard::new(guards_map).is_accessible_to_fqin(&subject_fqin);

        assert_eq!(
            authz_result,
            AuthzResult::Permitted(r#"Explicitly permitted by:
- "vm1" VM with audit message: Authorization is permitted by ACL AclName("TestAcl") include statement AclStatementStr("vm-test:*")
- "vm2" VM with audit message: Authorization is permitted by ACL AclName("TestAcl") include statement AclStatementStr("*")
"#.to_string())
        );
    }

    #[test]
    fn test_is_accessible_to_fqin_explicitly_denied() {
        let subject_fqin = ServiceFqin::from_str("vm-test:com.google.Car/main").unwrap();

        let guards_map = BTreeMap::from([
            (VmName::from_str("vm1").unwrap(), create_authz_guard(&["vm-non-test:*"], &[])),
            (VmName::from_str("vm2").unwrap(), create_authz_guard(&[], &["vm-test:*"])),
        ]);

        let authz_result = MultiVmAuthzGuard::new(guards_map).is_accessible_to_fqin(&subject_fqin);

        assert_eq!(
            authz_result,
            AuthzResult::ExplicitlyDenied(r#"Explicitly denied by VmName("vm2"): Authorization is denied by ACL AclName("TestAcl") never_include statement AclStatementStr("vm-test:*")"#.to_string())
        );
    }

    #[test]
    fn test_is_accessible_to_fqin_permitted_and_denied() {
        let subject_fqin = ServiceFqin::from_str("vm-test:com.google.Car/main").unwrap();

        let guards_map = BTreeMap::from([
            (VmName::from_str("vm1").unwrap(), create_authz_guard(&["vm-test:*"], &[])),
            (VmName::from_str("vm2").unwrap(), create_authz_guard(&[], &["vm-test:*"])),
        ]);

        let authz_result = MultiVmAuthzGuard::new(guards_map).is_accessible_to_fqin(&subject_fqin);

        assert_eq!(
            authz_result,
            AuthzResult::ExplicitlyDenied(r#"Explicitly denied by VmName("vm2"): Authorization is denied by ACL AclName("TestAcl") never_include statement AclStatementStr("vm-test:*")"#.to_string())
        );
    }

    #[test]
    fn test_is_accessible_to_fqin_implicitly_denied() {
        let subject_fqin = ServiceFqin::from_str("vm-test:com.google.Car/main").unwrap();

        let guards_map = BTreeMap::from([
            (VmName::from_str("vm1").unwrap(), create_authz_guard(&["vm-non-test:*"], &[])),
            (VmName::from_str("vm2").unwrap(), create_authz_guard(&[], &["vm-non-test:*"])),
        ]);

        let authz_result = MultiVmAuthzGuard::new(guards_map).is_accessible_to_fqin(&subject_fqin);

        assert_eq!(
            authz_result,
            AuthzResult::ImplicitlyDenied(
                r#"No ACL found that permit access among VMs: [VmName("vm1"), VmName("vm2")]"#
                    .to_string()
            )
        );
    }

    #[test]
    fn test_is_accessible_to_fqin_implicitly_denied_empty_guards_list() {
        let subject_fqin = ServiceFqin::from_str("vm-test:com.google.Car/main").unwrap();

        let authz_result =
            MultiVmAuthzGuard::new(BTreeMap::new()).is_accessible_to_fqin(&subject_fqin);

        assert_eq!(
            authz_result,
            AuthzResult::ImplicitlyDenied(
                r#"No ACL found that permit access among VMs: []"#.to_string()
            )
        );
    }
}
