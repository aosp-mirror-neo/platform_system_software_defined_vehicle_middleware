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

//! Entry point to the Authz crate.

use crate::multi_vm_authz_guard::MultiVmAuthzGuard;
use crate::{AclPolicyProvider, AuthzGuard, AuthzResult};

use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::anyhow;
use sdv_comms_id::{names::PackageName, names::ServiceUnitName, names::UnitTypeName, ServiceFqin};
use tracing::instrument;

use std::sync::Arc;

/// AuthzViaAcls is an entry point for ACL-based authorization checks.
///
/// The AuthzViaAcls component relies on ACL policies to function, which are stored
/// within APEXes in the SDV platform. To access these policies, users of AuthzViaAcls
/// must provide an ACL policy provider.
pub struct AuthzViaAcls {
    provider: Arc<dyn AclPolicyProvider>,
}

impl AuthzViaAcls {
    /// Create new AuthzViaAcls instance.
    pub fn new(provider: Arc<dyn AclPolicyProvider>) -> Self {
        Self { provider }
    }

    /// Returns Subject FQIN authorization result for the Object Service Unit.
    pub fn check_authorization_for_service_unit(
        &self,
        subject_fqin: &ServiceFqin,
        object_fqin: &ServiceFqin,
        object_service_unit_name: &ServiceUnitName,
    ) -> AuthzResult {
        let authz_guard_result =
            self.create_service_unit_authz_guard(object_service_unit_name, object_fqin);

        let authz_guard = match authz_guard_result {
            Ok(authz_guard) => authz_guard,
            Err(err) => {
                return AuthzResult::ExplicitlyDenied(format!(
                    "failed to create AuthzGuard: {err:?}"
                ))
            }
        };

        authz_guard.is_accessible_to_fqin(subject_fqin)
    }

    /// Returns Subject FQIN authorization result for the Object Unit Type.
    pub fn check_authorization_for_unit_type(
        &self,
        subject_fqin: &ServiceFqin,
        object_package_name: &PackageName,
        object_unit_type_name: &UnitTypeName,
    ) -> AuthzResult {
        let authz_guard_result =
            self.create_unit_type_authz_guard(object_package_name, object_unit_type_name);

        let authz_guard = match authz_guard_result {
            Ok(authz_guard) => authz_guard,
            Err(err) => {
                return AuthzResult::ExplicitlyDenied(format!(
                    "failed to create AuthzGuard: {err:?}"
                ))
            }
        };

        authz_guard.is_accessible_to_fqin(subject_fqin)
    }

    /// Creates a Service Unit AuthzGuard which controls the access to
    /// the Service Unit with a given FQIN.
    #[instrument(name = "authz_perf::service::create_service_unit_authz_guard", skip_all)]
    fn create_service_unit_authz_guard(
        &self,
        service_unit_name: &ServiceUnitName,
        object_fqin: &ServiceFqin,
    ) -> Result<AuthzGuard, anyhow::Error> {
        let acl_policy = self.provider.get_acl_policy(
            object_fqin.vm_name(),
            object_fqin.package_name(),
            Some(object_fqin.service_name()),
        )?;

        let acls = acl_policy
            .get_acls_by_service_unit(service_unit_name)
            .ok_or(anyhow!("ACLs for {:?} is not specified", service_unit_name))?;

        Ok(AuthzGuard::new(acls))
    }

    /// Creates a Unit Type AuthzGuard which controls the access the Unit Type
    /// with a given package name, service bundle name and unit type name.
    #[instrument(name = "authz_perf::service::create_unit_type_authz_guard", skip_all)]
    fn create_unit_type_authz_guard(
        &self,
        package_name: &PackageName,
        unit_type_name: &UnitTypeName,
    ) -> Result<MultiVmAuthzGuard, anyhow::Error> {
        let vm_names = self.provider.get_available_vm_names();

        let mut authz_guards_map = BTreeMap::new();

        // Construct the fully qualified name (FQN) for the unit type.
        // This FQN will be split into potential package/relative-name combinations
        // to search for applicable ACL policies across different scopes.
        let full_unit_type_name = package_name.name().to_owned() + "." + unit_type_name.name();
        let pkg_unit_type_candidates = split_full_unit_type_name(&full_unit_type_name)?;

        for (pkg_name, rel_unit_type_name) in pkg_unit_type_candidates {
            for vm_name in &vm_names {
                let acl_policy = match self.provider.get_acl_policy(
                    vm_name, &pkg_name, None, // No service bundle
                ) {
                    Ok(policy) => policy,
                    Err(_) => continue, // Not found, try next package level or VM.
                };

                if let Some(acls) = acl_policy.get_acls_by_unit_type(&rel_unit_type_name) {
                    authz_guards_map.insert(vm_name.clone(), AuthzGuard::new(acls));
                }
            }
        }

        if authz_guards_map.is_empty() {
            return Err(anyhow!(
                "ACLs for {} are not found on available VMs: {:?}",
                full_unit_type_name,
                vm_names
            ));
        }

        Ok(MultiVmAuthzGuard::new(authz_guards_map))
    }
}

/// Attempts to split a fully qualified name (FQN) into all possible valid (PackageName, UnitTypeName) pairs.
///
/// It iterates over all possible split points (dots, excluding the first one)
/// and attempts to parse the resulting slices, collecting all valid splits found.
///
/// The function returns Ok(Vec<...>) containing all valid splits, or Err()
/// if the input string is invalid.
fn split_full_unit_type_name(fqn: &str) -> Result<Vec<(PackageName, UnitTypeName)>, anyhow::Error> {
    if fqn.is_empty() {
        return Err(anyhow!("Empty Unit Type"));
    }

    let mut valid_splits = Vec::new();

    // We iterate over the index of every '.' starting with the *second* one (skip(1)).
    let mut dot_indices = fqn.match_indices('.').skip(1).peekable();

    // Check if there are any dots after the first one to attempt splitting
    if dot_indices.peek().is_none() {
        return Err(anyhow!("Invalid UnitTypeName (needs to contain multiple dots): '{}'", fqn));
    }

    for (dot_index, _) in dot_indices {
        let package_str = &fqn[..dot_index];
        let relative_type_str = &fqn[dot_index + 1..];

        // Attempt to parse the slices into the custom types.
        if let (Ok(package_name), Ok(relative_unit_type_name)) =
            (PackageName::from_str(package_str), UnitTypeName::from_str(relative_type_str))
        {
            // If both parse successfully, we have found a valid split. Collect it.
            valid_splits.push((package_name, relative_unit_type_name));
        }
    }

    Ok(valid_splits)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::AclPolicyModel;
    use crate::TestAclPolicyProvider;

    use std::{
        collections::{HashMap, HashSet},
        str::FromStr,
        sync::Arc,
    };

    use protobuf::text_format;
    use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
    use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName, VmName};

    #[test]
    fn test_create_service_unit_authz_guard_success() {
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("vm").unwrap(),
            PackageName::from_str("com.google.sdv").unwrap(),
            Some(ServiceBundleName::from_str("WindowManager").unwrap()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"
                        acls {
                            name: "TirePressureSubs"
                            include: "vm-cdc:*"
                            include: "vm-telemetry:*"
                            never_include: "vm-ivi:*"
                        }
                        service_units {
                            name: "tire-pressure-publisher"
                            acl_names: "TirePressureSubs"
                        }
                    "#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        let authz_via_acls = AuthzViaAcls::new(Arc::new(test_provider));

        let service_unit_name = ServiceUnitName::from_str("tire-pressure-publisher").unwrap();
        let object_fqin = ServiceFqin::from_str("vm:com.google.sdv.WindowManager/main").unwrap();

        let subject_fqin =
            ServiceFqin::from_str("vm-telemetry:com.oem.TunnelService/main").unwrap();
        let authz_result = authz_via_acls.check_authorization_for_service_unit(
            &subject_fqin,
            &object_fqin,
            &service_unit_name,
        );
        assert!(
            matches!(authz_result, AuthzResult::Permitted(_)),
            "Unexpected authz result: {authz_result:?}"
        );

        let subject_fqin = ServiceFqin::from_str("vm-ivi:com.oem.TunnelService/main").unwrap();
        let authz_result = authz_via_acls.check_authorization_for_service_unit(
            &subject_fqin,
            &object_fqin,
            &service_unit_name,
        );
        assert!(
            matches!(authz_result, AuthzResult::ExplicitlyDenied(_)),
            "Unexpected authz result: {authz_result:?}"
        );
    }

    #[test]
    fn test_create_service_unit_authz_guard_not_found() {
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("vm").unwrap(),
            PackageName::from_str("com.google.sdv").unwrap(),
            Some(ServiceBundleName::from_str("WindowManager").unwrap()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"
                        acls {
                            name: "TirePressureSubs"
                            include: "vm-cdc:*"
                            include: "vm-telemetry:*"
                            never_include: "vm-ivi:*"
                        }
                        service_units {
                            name: "tire-pressure-publisher"
                            acl_names: "TirePressureSubs"
                        }
                    "#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        let authz_via_acls = AuthzViaAcls::new(Arc::new(test_provider));

        let non_existent_service_unit_name =
            ServiceUnitName::from_str("temperature-publisher").unwrap();
        let object_fqin = ServiceFqin::from_str("vm:com.google.sdv.WindowManager/main").unwrap();

        let result = authz_via_acls.check_authorization_for_service_unit(
            &ServiceFqin::from_str("vm:com.a.B/c").unwrap(),
            &object_fqin,
            &non_existent_service_unit_name,
        );
        assert!(
            matches!(result, AuthzResult::ExplicitlyDenied(_)),
            "Unexpected result: {result:?}"
        );
    }

    #[test]
    fn test_create_unit_type_authz_guard_no_available_vms() {
        let test_provider = TestAclPolicyProvider::new();
        let authz_via_acls = AuthzViaAcls::new(Arc::new(test_provider));

        let err = authz_via_acls
            .create_unit_type_authz_guard(
                &PackageName::from_str("com.google.sdv").unwrap(),
                &UnitTypeName::from_str("TirePressureTopic").unwrap(),
            )
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "ACLs for com.google.sdv.TirePressureTopic are not found on available VMs: []"
        );
    }

    #[test]
    fn test_create_unit_type_authz_guard_no_acl_policy() {
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("vm").unwrap(),
            PackageName::from_str("com.google.sdv").unwrap(),
            None,
            AclPolicyModel::new(
                HashMap::new(),
                HashMap::new(),
                HashSet::new(),
                HashSet::new(),
                HashSet::new(),
                HashSet::new(),
                false,
            ),
        );

        let authz_via_acls = AuthzViaAcls::new(Arc::new(test_provider));

        let err = authz_via_acls
            .create_unit_type_authz_guard(
                &PackageName::from_str("com.google.other").unwrap(),
                &UnitTypeName::from_str("TirePressureTopic").unwrap(),
            )
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"ACLs for com.google.other.TirePressureTopic are not found on available VMs: [VmName("vm")]"#
        );
    }

    #[test]
    fn test_create_unit_type_authz_guard_no_unit_type_in_acl_policy() {
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("vm").unwrap(),
            PackageName::from_str("com.google.sdv").unwrap(),
            None,
            AclPolicyModel::new(
                HashMap::new(),
                HashMap::new(),
                HashSet::new(),
                HashSet::new(),
                HashSet::new(),
                HashSet::new(),
                false,
            ),
        );

        let authz_via_acls = AuthzViaAcls::new(Arc::new(test_provider));

        let err = authz_via_acls
            .create_unit_type_authz_guard(
                &PackageName::from_str("com.google.sdv").unwrap(),
                &UnitTypeName::from_str("TirePressureTopic").unwrap(),
            )
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"ACLs for com.google.sdv.TirePressureTopic are not found on available VMs: [VmName("vm")]"#
        );
    }

    #[test]
    fn test_create_unit_type_authz_guard_success() {
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("vm-allow").unwrap(),
            PackageName::from_str("com.google.sdv").unwrap(),
            None,
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"
                        acls {
                            name: "TirePressurePubs"
                            include: "vm-allow:*"
                        }
                        unit_types {
                            name: "TirePressureTopic"
                            acl_names: "TirePressurePubs"
                        }
                    "#,
                )
                .unwrap(),
            )
            .unwrap(),
        );
        test_provider.set_authz_policy(
            VmName::from_str("vm-deny").unwrap(),
            PackageName::from_str("com.google.sdv").unwrap(),
            None,
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"
                        acls {
                            name: "TirePressurePubs"
                            never_include: "vm-deny:*"
                        }
                        unit_types {
                            name: "TirePressureTopic"
                            acl_names: "TirePressurePubs"
                        }
                    "#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        let authz_via_acls = AuthzViaAcls::new(Arc::new(test_provider));

        let unit_type_authz_guard = authz_via_acls
            .create_unit_type_authz_guard(
                &PackageName::from_str("com.google.sdv").unwrap(),
                &UnitTypeName::from_str("TirePressureTopic").unwrap(),
            )
            .unwrap();

        let subject_fqin = ServiceFqin::from_str("vm-allow:com.oem.TunnelPublisher/main").unwrap();
        assert_eq!(
            unit_type_authz_guard.is_accessible_to_fqin(&subject_fqin),
            AuthzResult::Permitted(r#"Explicitly permitted by:
- "vm-allow" VM with audit message: Authorization is permitted by ACL AclName("TirePressurePubs") include statement AclStatementStr("vm-allow:*")
"#.to_string())
        );

        let subject_fqin = ServiceFqin::from_str("vm-deny:com.oem.TunnelPublisher/main").unwrap();
        assert_eq!(
            unit_type_authz_guard.is_accessible_to_fqin(&subject_fqin),
            AuthzResult::ExplicitlyDenied(
                r#"Explicitly denied by VmName("vm-deny"): Authorization is denied by ACL AclName("TirePressurePubs") never_include statement AclStatementStr("vm-deny:*")"#.to_string()
            )
        );

        let subject_fqin =
            ServiceFqin::from_str("vm-implicit-deny:com.oem.TunnelPublisher/main").unwrap();
        assert_eq!(
            unit_type_authz_guard.is_accessible_to_fqin(&subject_fqin),
            AuthzResult::ImplicitlyDenied(
                r#"No ACL found that permit access among VMs: [VmName("vm-allow"), VmName("vm-deny")]"#.to_string()
            )
        );
    }
}
