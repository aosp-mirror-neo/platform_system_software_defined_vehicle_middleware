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

//! Authz Service.

use crate::authz_via_acls::AuthzViaAcls;
#[cfg(feature = "permissions")]
use crate::authz_via_permissions::AuthzViaPermissions;
use crate::{AclPolicyProvider, AuthzEnforcementFlag, AuthzResult};
use sdv_comms_id::names::{PackageName, ServiceUnitName, UnitTypeName};
use sdv_comms_id::ServiceFqin;

use log::{debug, error, warn};
use tracing::instrument;

use std::sync::Arc;

/// Authz Service.
pub struct AuthzService {
    authz_via_acls: AuthzViaAcls,
    #[cfg(feature = "permissions")]
    authz_via_permissions: AuthzViaPermissions,
    authz_enforcement_flag: Arc<dyn AuthzEnforcementFlag>,
}

impl AuthzService {
    /// Creates new instance of Authz Service.
    pub fn new(
        acl_policy_provider: Arc<dyn AclPolicyProvider>,
        authz_enforcement_flag: Arc<dyn AuthzEnforcementFlag>,
    ) -> Self {
        let authz_via_acls = AuthzViaAcls::new(acl_policy_provider.clone());
        #[cfg(feature = "permissions")]
        let authz_via_permissions = AuthzViaPermissions::new(acl_policy_provider);

        AuthzService {
            authz_via_acls,
            #[cfg(feature = "permissions")]
            authz_via_permissions,
            authz_enforcement_flag,
        }
    }

    /// Returns whether Subject FQIN is authorized for the Object Service Unit.
    #[instrument(name = "authz_perf::service::is_authorized_for_service_unit", skip_all)]
    pub fn is_authorized(
        &self,
        subject_fqin: &ServiceFqin,
        object_fqin: &ServiceFqin,
        object_service_unit_name: &ServiceUnitName,
    ) -> bool {
        let object_log_identifier = format!("{object_fqin}#{object_service_unit_name}");

        let acl_result = self.authz_via_acls.check_authorization_for_service_unit(
            subject_fqin,
            object_fqin,
            object_service_unit_name,
        );
        if let AuthzResult::Permitted(_) = acl_result {
            debug!("Permitted by ACLs: {subject_fqin} to access {object_log_identifier}");
            return self.handle_authz_result(subject_fqin, object_log_identifier, acl_result);
        }

        #[cfg(feature = "permissions")]
        {
            let permissions_result =
                self.authz_via_permissions.check_authorization_for_service_unit(
                    subject_fqin,
                    object_fqin,
                    object_service_unit_name,
                );
            if let AuthzResult::Permitted(_) = permissions_result {
                debug!(
                    "Permitted by permissions: {subject_fqin} to access {object_log_identifier}"
                );
                return self.handle_authz_result(
                    subject_fqin,
                    object_log_identifier,
                    permissions_result,
                );
            }
        }

        // Neither permitted, so we'll use the ACL result for logging denial.
        self.handle_authz_result(subject_fqin, object_log_identifier, acl_result)
    }

    /// Returns whether Subject FQIN is authorized for the Object Unit Type.
    #[instrument(name = "authz_perf::service::is_authorized_for_unit_type", skip_all)]
    pub fn is_authorized_for_unit_type(
        &self,
        subject_fqin: &ServiceFqin,
        object_package_name: &PackageName,
        object_unit_type_name: &UnitTypeName,
    ) -> bool {
        let object_log_identifier = format!("{object_package_name}.{object_unit_type_name}");

        let acl_result = self.authz_via_acls.check_authorization_for_unit_type(
            subject_fqin,
            object_package_name,
            object_unit_type_name,
        );
        if let AuthzResult::Permitted(_) = acl_result {
            debug!("Permitted by ACLs: {subject_fqin} to access {object_log_identifier}");
            return self.handle_authz_result(subject_fqin, object_log_identifier, acl_result);
        }

        #[cfg(feature = "permissions")]
        {
            let permissions_result = self.authz_via_permissions.check_authorization_for_unit_type(
                subject_fqin,
                object_package_name,
                object_unit_type_name,
            );
            if let AuthzResult::Permitted(_) = permissions_result {
                debug!(
                    "Permitted by permissions: {subject_fqin} to access {object_log_identifier}"
                );
                return self.handle_authz_result(
                    subject_fqin,
                    object_log_identifier,
                    permissions_result,
                );
            }
        }

        // Neither permitted. Return ACL result for logging.
        self.handle_authz_result(subject_fqin, object_log_identifier, acl_result)
    }

    fn handle_authz_result(
        &self,
        subject_fqin: &ServiceFqin,
        object_log_identifier: String,
        authz_result: AuthzResult,
    ) -> bool {
        let (is_authorized, audit_msg) = convert_authz_result(authz_result);

        if is_authorized {
            debug!(
                "{object_log_identifier} allows access to {subject_fqin}, audit message: {audit_msg}"
            );
            return true;
        }

        if !self.authz_enforcement_flag.is_enabled() {
            // This is so called soft enforcement:
            //   1. Access is granted even though `Subject` is not authorized to the `Object`;
            //   2. Access violation is logged.
            warn!(
                "Authz violation. Access is granted by flag. {object_log_identifier} denies access to {subject_fqin}, audit message: {audit_msg}"
            );
            return true;
        }

        error!(
            "Authz violation. {object_log_identifier} denies access to {subject_fqin}, audit message: {audit_msg}"
        );
        false
    }
}

// Converts AuthzResult to a tuple of bool and audit message.
fn convert_authz_result(authz_result: AuthzResult) -> (bool, String) {
    match authz_result {
        AuthzResult::Permitted(audit_msg) => (true, audit_msg),
        AuthzResult::ExplicitlyDenied(audit_msg) => (false, audit_msg),
        AuthzResult::ImplicitlyDenied(audit_msg) => (false, audit_msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::AclPolicyModel;

    use std::collections::HashMap;
    use std::str::FromStr;

    use anyhow::anyhow;
    use protobuf::text_format;
    use sdv_authz_policy_proto_rust::acl_policy::AclPolicy as AclPolicyProto;
    use sdv_comms_id::names::{
        PackageName, ServiceName as ServiceBundleName, ServiceUnitName, VmName,
    };

    struct TestAuthzEnforcementFlag {
        enabled: bool,
    }

    impl TestAuthzEnforcementFlag {
        fn new(enabled: bool) -> Self {
            Self { enabled }
        }
    }

    impl AuthzEnforcementFlag for TestAuthzEnforcementFlag {
        fn is_enabled(&self) -> bool {
            self.enabled
        }
    }

    struct TestAclPolicyProvider {
        mapping: HashMap<(VmName, PackageName, Option<ServiceBundleName>), AclPolicyModel>,
    }

    impl TestAclPolicyProvider {
        fn new() -> Self {
            Self { mapping: HashMap::new() }
        }

        fn set_alc_policy(
            &mut self,
            vm_name: VmName,
            package_name: PackageName,
            service_name: Option<ServiceBundleName>,
            acl_policy: AclPolicyModel,
        ) {
            self.mapping.insert(
                (vm_name.clone(), package_name.clone(), service_name.clone()),
                acl_policy.clone(),
            );
            if service_name.is_some() {
                self.mapping.insert((vm_name, package_name, None), acl_policy);
            }
        }
    }

    impl AclPolicyProvider for TestAclPolicyProvider {
        fn get_acl_policy(
            &self,
            vm_name: &VmName,
            package_name: &PackageName,
            service_bundle_name: Option<&ServiceBundleName>,
        ) -> Result<Arc<AclPolicyModel>, anyhow::Error> {
            self.mapping
                .get(&(vm_name.clone(), package_name.clone(), service_bundle_name.cloned()))
                .ok_or(anyhow!(
                    "ACLs for {:?}:{}.{:?} are not specified",
                    vm_name,
                    package_name,
                    service_bundle_name
                ))
                .cloned()
                .map(Arc::new)
        }

        fn get_available_vm_names(&self) -> Vec<VmName> {
            let mut vm_names: Vec<VmName> = self.mapping.keys().map(|k| k.0.clone()).collect();
            vm_names.sort_by_key(|vm_name| vm_name.name().to_string());
            vm_names
        }
    }

    fn create_test_authz_service(
        object_fqin: &ServiceFqin,
        acl_policy_text_proto: &str,
        is_enforcement_flag_enabled: bool,
    ) -> AuthzService {
        let acl_policy_model = AclPolicyModel::try_from(
            text_format::parse_from_str::<AclPolicyProto>(acl_policy_text_proto).unwrap(),
        )
        .unwrap();

        let mut test_acl_provider = TestAclPolicyProvider::new();
        test_acl_provider.set_alc_policy(
            object_fqin.vm_name().clone(),
            object_fqin.package_name().clone(),
            Some(object_fqin.service_name().clone()),
            acl_policy_model,
        );

        AuthzService::new(
            Arc::new(test_acl_provider),
            Arc::new(TestAuthzEnforcementFlag::new(is_enforcement_flag_enabled)),
        )
    }

    fn setup_test(
        is_enforcement_flag_enabled: bool,
    ) -> (AuthzService, ServiceFqin, ServiceUnitName, UnitTypeName) {
        let object_fqin = ServiceFqin::from_str("vm:com.google.sdv.WindowManager/main").unwrap();

        let authz_service = create_test_authz_service(
            &object_fqin,
            r#"
                acls {
                    name: "TirePressureAcl"
                    include: "vm-allow:*"
                    never_include: "vm-deny:*"
                }
                service_units {
                    name: "tire-pressure-publisher"
                    acl_names: "TirePressureAcl"
                }
                unit_types {
                    name: "TirePressureTopic"
                    acl_names: "TirePressureAcl"
                }
            "#,
            is_enforcement_flag_enabled,
        );

        let object_service_unit_name =
            ServiceUnitName::from_str("tire-pressure-publisher").unwrap();

        let object_unit_type_name = UnitTypeName::from_str("TirePressureTopic").unwrap();

        (authz_service, object_fqin, object_service_unit_name, object_unit_type_name)
    }

    #[test]
    fn test_is_authorized_for_service_unit_permitted() {
        let (authz_service, object_fqin, object_service_unit_name, _) =
            setup_test(/*is_enforcement_flag_enabled=*/ true);

        let subject_fqin = ServiceFqin::from_str("vm-allow:com.oem.TunnelService/main").unwrap();

        assert!(authz_service.is_authorized(
            &subject_fqin,
            &object_fqin,
            &object_service_unit_name
        ));
    }

    #[test]
    fn test_is_authorized_for_service_unit_denied() {
        let (authz_service, object_fqin, object_service_unit_name, _) =
            setup_test(/*is_enforcement_flag_enabled=*/ true);

        for subject_fqin in [
            ServiceFqin::from_str("vm-deny:com.oem.TunnelService/main").unwrap(),
            ServiceFqin::from_str("vm-deny-implicit:com.oem.TunnelService/main").unwrap(),
        ] {
            assert!(!authz_service.is_authorized(
                &subject_fqin,
                &object_fqin,
                &object_service_unit_name
            ));
        }
    }

    #[test]
    fn test_is_authorized_for_service_unit_permitted_by_flag() {
        let (authz_service, object_fqin, object_service_unit_name, _) =
            setup_test(/*is_enforcement_flag_enabled=*/ false);

        for subject_fqin in [
            ServiceFqin::from_str("vm-deny:com.oem.TunnelService/main").unwrap(),
            ServiceFqin::from_str("vm-deny-implicit:com.oem.TunnelService/main").unwrap(),
        ] {
            assert!(authz_service.is_authorized(
                &subject_fqin,
                &object_fqin,
                &object_service_unit_name
            ));
        }
    }

    #[test]
    fn test_is_authorized_for_unit_type_permitted() {
        let (authz_service, object_fqin, _, object_unit_type_name) =
            setup_test(/*is_enforcement_flag_enabled=*/ true);

        let subject_fqin = ServiceFqin::from_str("vm-allow:com.oem.TunnelService/main").unwrap();

        assert!(authz_service.is_authorized_for_unit_type(
            &subject_fqin,
            object_fqin.package_name(),
            &object_unit_type_name,
        ));
    }

    #[test]
    fn test_is_authorized_for_unit_type_denied() {
        let (authz_service, object_fqin, _, object_unit_type_name) =
            setup_test(/*is_enforcement_flag_enabled=*/ true);

        for subject_fqin in [
            ServiceFqin::from_str("vm-deny:com.oem.TunnelService/main").unwrap(),
            ServiceFqin::from_str("vm-deny-implicit:com.oem.TunnelService/main").unwrap(),
        ] {
            assert!(!authz_service.is_authorized_for_unit_type(
                &subject_fqin,
                object_fqin.package_name(),
                &object_unit_type_name,
            ));
        }
    }

    #[test]
    fn test_is_authorized_for_unit_type_permitted_by_flag() {
        let (authz_service, object_fqin, _, object_unit_type_name) =
            setup_test(/*is_enforcement_flag_enabled=*/ false);

        for subject_fqin in [
            ServiceFqin::from_str("vm-deny:com.oem.TunnelService/main").unwrap(),
            ServiceFqin::from_str("vm-deny-implicit:com.oem.TunnelService/main").unwrap(),
        ] {
            assert!(authz_service.is_authorized_for_unit_type(
                &subject_fqin,
                object_fqin.package_name(),
                &object_unit_type_name,
            ));
        }
    }
}
