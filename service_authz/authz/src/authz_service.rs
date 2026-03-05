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
use crate::authz_via_permissions::AuthzViaPermissions;
use crate::feature_flags;
use crate::{AclPolicyProvider, AuthzEnforcementFlag, AuthzResult};

use sdv_comms_id::names::{PackageName, ServiceUnitName, UnitTypeName};
use sdv_comms_id::ServiceFqin;

use anyhow::Context;
use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinAidl;
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::ServiceUnitDefinition;
use log::{debug, error, warn};
use tracing::instrument;

use std::str::FromStr;
use std::sync::Arc;

/// Trait for retrieving the full definition of a service unit.
pub trait ServiceUnitRegistry: Send + Sync {
    /// Returns service unit definition for the specified unit.
    fn get_service_unit(
        &self,
        fqin: &ServiceFqinAidl,
        unit_name: &str,
    ) -> Option<ServiceUnitDefinition>;
}

/// Authz Service.
pub struct AuthzService {
    authz_via_acls: AuthzViaAcls,
    authz_via_permissions: AuthzViaPermissions,
    authz_enforcement_flag: Arc<dyn AuthzEnforcementFlag>,
}

impl AuthzService {
    /// Creates new instance of Authz Service.
    pub fn new(
        acl_policy_provider: Arc<dyn AclPolicyProvider>,
        authz_enforcement_flag: Arc<dyn AuthzEnforcementFlag>,
        service_unit_registry: Arc<dyn ServiceUnitRegistry>,
    ) -> Self {
        let authz_via_acls = AuthzViaAcls::new(acl_policy_provider.clone());
        let authz_via_permissions =
            AuthzViaPermissions::new(acl_policy_provider, service_unit_registry);

        AuthzService { authz_via_acls, authz_via_permissions, authz_enforcement_flag }
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

        let permission_result = if feature_flags::enable_permissions() {
            self.authz_via_permissions.check_authorization_for_service_unit(
                subject_fqin,
                object_fqin,
                object_service_unit_name,
            )
        } else {
            AuthzResult::ImplicitlyDenied("Permissions are disabled".to_string())
        };

        // Use both results to make final decision and log denial if needed.
        self.handle_authz_result(subject_fqin, object_log_identifier, acl_result, permission_result)
    }

    /// Returns whether Subject FQIN is authorized for the Object Service Unit.
    #[instrument(name = "authz_perf::service::is_authorized_for_sud", skip_all)]
    pub fn is_authorized_for_sud(
        &self,
        subject_fqin: &ServiceFqin,
        object_sud: &ServiceUnitDefinition,
    ) -> bool {
        let (object_fqin, object_service_unit_name) = match validate_sud_input(object_sud) {
            Ok(values) => values,
            Err(err) => {
                error!("Failed to validate authz SUD filter input: {:?}", err);
                return false;
            }
        };

        let object_log_identifier = format!("{object_fqin}#{object_service_unit_name}");

        let acl_result = self.authz_via_acls.check_authorization_for_service_unit(
            subject_fqin,
            &object_fqin,
            &object_service_unit_name,
        );

        let permission_result = if feature_flags::enable_permissions() {
            self.authz_via_permissions.check_authorization_for_sud(subject_fqin, object_sud)
        } else {
            AuthzResult::ImplicitlyDenied("Permissions are disabled".to_string())
        };

        // Use both results to make final decision and log denial if needed.
        self.handle_authz_result(subject_fqin, object_log_identifier, acl_result, permission_result)
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

        let permission_result = if feature_flags::enable_permissions() {
            self.authz_via_permissions.check_authorization_for_unit_type(
                subject_fqin,
                object_package_name,
                object_unit_type_name,
            )
        } else {
            AuthzResult::ImplicitlyDenied("Permissions are disabled".to_string())
        };

        // Use both results to make final decision and log denial if needed.
        self.handle_authz_result(subject_fqin, object_log_identifier, acl_result, permission_result)
    }

    fn handle_authz_result(
        &self,
        subject_fqin: &ServiceFqin,
        object_log_identifier: String,
        authz_result_from_acls: AuthzResult,
        authz_result_from_permissions: AuthzResult,
    ) -> bool {
        let (is_authorized_by_acls, audit_msg_from_acls) =
            convert_authz_result(authz_result_from_acls);
        let (is_authorized_by_permissions, audit_msg_from_permissions) =
            convert_authz_result(authz_result_from_permissions);

        if is_authorized_by_acls {
            debug!(
                "{object_log_identifier} allows access to {subject_fqin}, audit message: {audit_msg_from_acls}"
            );
        }

        if is_authorized_by_permissions {
            debug!(
                "{subject_fqin} declares permission to access {object_log_identifier}, audit message: {audit_msg_from_permissions}"
            );
        }

        if is_authorized_by_acls != is_authorized_by_permissions {
            debug!("Access [subject: {subject_fqin}] to [object: {object_log_identifier}] mismatch: ACLs[{is_authorized_by_acls}], Permissions[{is_authorized_by_permissions}]");
        }

        if is_authorized_by_acls || is_authorized_by_permissions {
            return true;
        }

        if !self.authz_enforcement_flag.is_enabled() {
            // This is so called soft enforcement:
            //   1. Access is granted even though `Subject` is not authorized to the `Object`;
            //   2. Access violation is logged.
            warn!(
                "Authz violation. Access is granted by flag. {object_log_identifier} denies access to {subject_fqin}, audit message: {audit_msg_from_acls}"
            );
            warn!(
                "Authz violation. Access is granted by flag. {subject_fqin} does not declare permission to access {object_log_identifier}, audit message: {audit_msg_from_permissions}"
            );
            return true;
        }

        error!(
            "Authz violation. {object_log_identifier} denies access to {subject_fqin}, audit message: {audit_msg_from_acls}"
        );
        error!(
            "Authz violation. {subject_fqin} does not declare permission to access {object_log_identifier}, audit message: {audit_msg_from_permissions}"
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

/// Validates the input and returns tuple of
///   1. Object ServiceFQIN,
///   2. Object Service Unit Name.
fn validate_sud_input(
    sud: &ServiceUnitDefinition,
) -> Result<(ServiceFqin, ServiceUnitName), anyhow::Error> {
    let object_fqin = ServiceFqin::try_from(sud.identity.name.clone()).with_context(|| {
        format!(
            "Failed to create valid FQIN from a ServiceUnitDefinition ({:?})",
            sud.identity.name
        )
    })?;

    let object_service_unit_name =
        ServiceUnitName::from_str(&sud.serviceUnitName).with_context(|| {
            format!("Failed to validate service unit name {:?}", &sud.serviceUnitName)
        })?;

    Ok((object_fqin, object_service_unit_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::AclPolicyModel;

    use std::collections::HashMap;
    use std::str::FromStr;

    use anyhow::anyhow;
    use protobuf::text_format;
    use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
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

    struct TestRegistry {}

    impl ServiceUnitRegistry for TestRegistry {
        fn get_service_unit(
            &self,
            _fqin: &ServiceFqinAidl,
            _unit_name: &str,
        ) -> Option<ServiceUnitDefinition> {
            None
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
            Arc::new(TestRegistry {}),
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
