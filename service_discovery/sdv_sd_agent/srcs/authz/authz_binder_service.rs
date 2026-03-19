// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! SDV Authorization Binder Service.

use google_sdv_authz::aidl::google::sdv::authz::IAuthzService::IAuthzService;
use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinAidl;
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::UnitType::UnitType as UnitTypeAidl;

use sdv_authz::AuthzService;
use sdv_comms_id::{names::ServiceUnitName, ServiceFqin};

use anyhow::Context;
use binder::{ExceptionCode, Interface, Status};
use log::error;

use std::str::FromStr;
use std::sync::Arc;

use tracing::instrument;

/// Service responsible for checking access to bundles/service units by means of ACLs.
pub(crate) struct AuthzBinderService {
    authz_service: Arc<AuthzService>,
}

impl AuthzBinderService {
    pub(crate) fn new(authz_service: Arc<AuthzService>) -> Self {
        Self { authz_service }
    }
}

impl Interface for AuthzBinderService {}

#[allow(non_snake_case)] // generated AIDL interface
impl IAuthzService for AuthzBinderService {
    #[instrument(name = "authz_perf::binder::is_authorized", skip_all)]
    fn isAuthorized(
        &self,
        subject_fqin_input: &ServiceFqinAidl,
        object_fqin_input: &ServiceFqinAidl,
        object_service_unit_name_input: &str,
    ) -> binder::Result<bool> {
        let (subject_fqin, object_fqin, object_service_unit_name) =
            match validate_is_authorized_input(
                subject_fqin_input,
                object_fqin_input,
                object_service_unit_name_input,
            ) {
                Ok(value) => value,
                Err(err) => {
                    error!("Authz violation. Failed to validate isAuthorized input: {err:?}");
                    return Err(Status::new_exception_str(
                        ExceptionCode::ILLEGAL_ARGUMENT,
                        Some(err.to_string()),
                    ));
                }
            };

        Ok(self.authz_service.is_authorized(&subject_fqin, &object_fqin, &object_service_unit_name))
    }

    fn isAuthorizedForTopic(
        &self,
        _subject_fqin: &ServiceFqinAidl,
        _topic_name: &str,
        _object_unit_type: &UnitTypeAidl,
    ) -> binder::Result<bool> {
        unimplemented!()
    }
}

/// Validates IAuthzService::isAuthorized input and returns tuple of
///   1. Subject ServiceFQIN,
///   2. Object ServiceFQIN,
///   3. Object Service Unit Name.
#[instrument(name = "authz_perf::binder::validate_is_authorized_input", skip_all)]
fn validate_is_authorized_input(
    subject_fqin_input: &ServiceFqinAidl,
    object_fqin_input: &ServiceFqinAidl,
    object_service_unit_name_input: &str,
) -> Result<(ServiceFqin, ServiceFqin, ServiceUnitName), anyhow::Error> {
    let subject_fqin = ServiceFqin::try_from(subject_fqin_input.clone())
        .with_context(|| format!("Failed to parse subject FQIN: {subject_fqin_input:?}"))?;

    let object_fqin = ServiceFqin::try_from(object_fqin_input.clone())
        .with_context(|| format!("Failed to parse object FQIN: {object_fqin_input:?}"))?;

    let object_service_unit_name = ServiceUnitName::from_str(object_service_unit_name_input)
        .with_context(|| {
            format!("Failed to parse object service unit name: {object_service_unit_name_input:?}")
        })?;

    Ok((subject_fqin, object_fqin, object_service_unit_name))
}

#[cfg(test)]
mod test {
    use super::*;

    use std::collections::HashMap;
    use std::str::FromStr;

    use anyhow::anyhow;
    use protobuf::text_format;
    use sdv_authz::{AclPolicyModel, AclPolicyProvider, AuthzEnforcementFlag, VmAuthzPolicyModel};
    use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
    use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName, VmName};

    use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::ServiceUnitDefinition;

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

        fn set_acl_policy(
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
            service_name: Option<&ServiceBundleName>,
        ) -> Result<Arc<AclPolicyModel>, anyhow::Error> {
            self.mapping
                .get(&(vm_name.clone(), package_name.clone(), service_name.cloned()))
                .ok_or(anyhow!(
                    "ACLs for {}:{}.{:?} are not specified",
                    vm_name,
                    package_name,
                    service_name
                ))
                .cloned()
                .map(Arc::new)
        }

        fn get_available_vm_names(&self) -> Vec<VmName> {
            let mut vm_names: Vec<VmName> = self.mapping.keys().map(|k| k.0.clone()).collect();
            vm_names.sort_by_key(|vm_name| vm_name.name().to_string());
            vm_names
        }

        fn get_vm_authz_policy(
            &self,
            _vm_name: &VmName,
        ) -> Result<Arc<VmAuthzPolicyModel>, anyhow::Error> {
            unimplemented!("get_vm_authz_policy is not implemented for test")
        }
    }

    struct TestRegistry {}

    impl sdv_authz::ServiceUnitRegistry for TestRegistry {
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
        authz_enforcement_flag: Arc<dyn AuthzEnforcementFlag>,
    ) -> AuthzService {
        let acl_policy_model = AclPolicyModel::try_from(
            text_format::parse_from_str::<AclPolicyProto>(acl_policy_text_proto).unwrap(),
        )
        .unwrap();

        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_acl_policy(
            object_fqin.vm_name().clone(),
            object_fqin.package_name().clone(),
            Some(object_fqin.service_name().clone()),
            acl_policy_model,
        );

        AuthzService::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(test_provider),
            authz_enforcement_flag,
            Arc::new(TestRegistry {}),
        )
    }

    fn setup_authz_binder_service(is_enforcement_flag_enabled: bool) -> AuthzBinderService {
        let object_fqin = ServiceFqin::from_str("vm:com.google.sdv.WindowManager/main").unwrap();
        let authz_enforcement_flag =
            Arc::new(TestAuthzEnforcementFlag::new(is_enforcement_flag_enabled));

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
            authz_enforcement_flag,
        );

        AuthzBinderService::new(Arc::new(authz_service))
    }

    fn create_service_fqin_aidl(
        sdv_vm_name: impl Into<String>,
        sdv_package_name: impl Into<String>,
        service_bundle_name: impl Into<String>,
        service_instance_name: impl Into<String>,
    ) -> ServiceFqinAidl {
        ServiceFqinAidl {
            sdvVmName: sdv_vm_name.into(),
            sdvPackageName: sdv_package_name.into(),
            serviceBundleName: service_bundle_name.into(),
            serviceInstanceName: service_instance_name.into(),
        }
    }

    /*
        Tests with is_enforcement_flag_enabled == true
    */

    #[test]
    fn test_is_authorized_allow() {
        // GIVEN a request from an explicitly allowed VM
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ true);
        let subject_fqin = create_service_fqin_aidl("vm-allow", "com.oem", "TunnelService", "main");
        let object_fqin = create_service_fqin_aidl("vm", "com.google.sdv", "WindowManager", "main");

        // WHEN
        let is_authorized = authz_binder_service.isAuthorized(
            &subject_fqin,
            &object_fqin,
            "tire-pressure-publisher",
        );

        // THEN did not fail
        assert!(is_authorized.is_ok());
        // THEN access is granted
        assert!(is_authorized.unwrap());
    }

    #[test]
    fn test_is_authorized_explicit_deny() {
        // GIVEN a request from an explicitly denied VM
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ true);
        let subject_fqin = create_service_fqin_aidl("vm-deny", "com.oem", "TunnelService", "main");
        let object_fqin = create_service_fqin_aidl("vm", "com.google.sdv", "WindowManager", "main");

        // WHEN
        let is_authorized = authz_binder_service.isAuthorized(
            &subject_fqin,
            &object_fqin,
            "tire-pressure-publisher",
        );

        // THEN did not fail
        assert!(is_authorized.is_ok());
        // THEN access is denied
        assert!(!is_authorized.unwrap());
    }

    #[test]
    fn test_is_authorized_implicit_deny() {
        // GIVEN a request from a VM that is not mentioned in the ACL
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ true);
        let subject_fqin =
            create_service_fqin_aidl("unknown-vm", "com.oem", "TunnelService", "main");
        let object_fqin = create_service_fqin_aidl("vm", "com.google.sdv", "WindowManager", "main");

        // WHEN
        let is_authorized = authz_binder_service.isAuthorized(
            &subject_fqin,
            &object_fqin,
            "tire-pressure-publisher",
        );

        // THEN did not fail
        assert!(is_authorized.is_ok());
        // THEN access is denied
        assert!(!is_authorized.unwrap());
    }

    #[test]
    fn test_is_authorized_missing_object_acls_deny() {
        // GIVEN a request from a VM that is not mentioned in the ACL
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ true);
        let subject_fqin = create_service_fqin_aidl("vm-allow", "com.oem", "TunnelService", "main");
        let object_fqin =
            create_service_fqin_aidl("unknown-vm", "com.google.sdv", "WindowManager", "main");

        // WHEN
        let is_authorized = authz_binder_service.isAuthorized(
            &subject_fqin,
            &object_fqin,
            "tire-pressure-publisher",
        );

        // THEN did not fail
        assert!(is_authorized.is_ok());
        // THEN access is denied
        assert!(!is_authorized.unwrap());
    }

    #[test]
    fn test_is_authorized_unknown_service_unit_deny() {
        // GIVEN a request from a VM that is not mentioned in the ACL
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ true);
        let subject_fqin = create_service_fqin_aidl("vm-allow", "com.oem", "TunnelService", "main");
        let object_fqin =
            create_service_fqin_aidl("unknown-vm", "com.google.sdv", "WindowManager", "main");

        // WHEN
        let is_authorized =
            authz_binder_service.isAuthorized(&subject_fqin, &object_fqin, "unknown-publisher");

        // THEN did not fail
        assert!(is_authorized.is_ok());
        // THEN access is denied
        assert!(!is_authorized.unwrap());
    }

    #[test]
    fn test_is_authorized_invalid_subject_deny() {
        // GIVEN a request with an invalid subject
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ true);
        let subject_fqin =
            create_service_fqin_aidl("vm-deny", "com...oem", "TunnelService", "main");
        let object_fqin = create_service_fqin_aidl("vm", "com.google.sdv", "WindowManager", "main");

        // WHEN
        let is_authorized = authz_binder_service.isAuthorized(
            &subject_fqin,
            &object_fqin,
            "tire-pressure-publisher",
        );

        // THEN result is error
        assert!(is_authorized.is_err());
    }

    #[test]
    fn test_is_authorized_invalid_object_deny() {
        // GIVEN a request with an invalid subject
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ true);
        let subject_fqin = create_service_fqin_aidl("vm-deny", "com.oem", "TunnelService", "main");
        let object_fqin =
            create_service_fqin_aidl("vm", "com.google.sdv", "WindowManager", "--main");

        // WHEN
        let is_authorized = authz_binder_service.isAuthorized(
            &subject_fqin,
            &object_fqin,
            "tire-pressure-publisher",
        );

        // THEN result is error
        assert!(is_authorized.is_err());
    }

    #[test]
    fn test_is_authorized_invalid_service_unit_deny() {
        // GIVEN a request with an invalid subject
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ true);
        let subject_fqin = create_service_fqin_aidl("vm-deny", "com.oem", "TunnelService", "main");
        let object_fqin = create_service_fqin_aidl("vm", "com.google.sdv", "WindowManager", "main");

        // WHEN
        let is_authorized = authz_binder_service.isAuthorized(
            &subject_fqin,
            &object_fqin,
            "--tire-pressure-publisher",
        );

        // THEN result is error
        assert!(is_authorized.is_err());
    }

    /*
        Tests with is_enforcement_flag_enabled == false
    */

    #[test]
    fn test_is_authorized_allow_allow_by_flag() {
        // GIVEN a request from an explicitly allowed VM
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ false);
        let subject_fqin = create_service_fqin_aidl("vm-allow", "com.oem", "TunnelService", "main");
        let object_fqin = create_service_fqin_aidl("vm", "com.google.sdv", "WindowManager", "main");

        // WHEN
        let is_authorized = authz_binder_service.isAuthorized(
            &subject_fqin,
            &object_fqin,
            "tire-pressure-publisher",
        );

        // THEN did not fail
        assert!(is_authorized.is_ok());
        // THEN access is granted
        assert!(is_authorized.unwrap());
    }

    #[test]
    fn test_is_authorized_explicit_deny_allow_by_flag() {
        // GIVEN a request from an explicitly denied VM
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ false);
        let subject_fqin = create_service_fqin_aidl("vm-deny", "com.oem", "TunnelService", "main");
        let object_fqin = create_service_fqin_aidl("vm", "com.google.sdv", "WindowManager", "main");

        // WHEN
        let is_authorized = authz_binder_service.isAuthorized(
            &subject_fqin,
            &object_fqin,
            "tire-pressure-publisher",
        );

        // THEN did not fail
        assert!(is_authorized.is_ok());
        // THEN access is granted
        assert!(is_authorized.unwrap());
    }

    #[test]
    fn test_is_authorized_implicit_deny_allow_by_flag() {
        // GIVEN a request from a VM that is not mentioned in the ACL
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ false);
        let subject_fqin =
            create_service_fqin_aidl("unknown-vm", "com.oem", "TunnelService", "main");
        let object_fqin = create_service_fqin_aidl("vm", "com.google.sdv", "WindowManager", "main");

        // WHEN
        let is_authorized = authz_binder_service.isAuthorized(
            &subject_fqin,
            &object_fqin,
            "tire-pressure-publisher",
        );

        // THEN did not fail
        assert!(is_authorized.is_ok());
        // THEN access is granted
        assert!(is_authorized.unwrap());
    }

    #[test]
    fn test_is_authorized_missing_object_acls_deny_allow_by_flag() {
        // GIVEN a request from a VM that is not mentioned in the ACL
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ false);
        let subject_fqin = create_service_fqin_aidl("vm-allow", "com.oem", "TunnelService", "main");
        let object_fqin =
            create_service_fqin_aidl("unknown-vm", "com.google.sdv", "WindowManager", "main");

        // WHEN
        let is_authorized = authz_binder_service.isAuthorized(
            &subject_fqin,
            &object_fqin,
            "tire-pressure-publisher",
        );

        // THEN did not fail
        assert!(is_authorized.is_ok());
        // THEN access is granted
        assert!(is_authorized.unwrap());
    }

    #[test]
    fn test_is_authorized_unknown_service_unit_deny_allow_by_flag() {
        // GIVEN a request from a VM that is not mentioned in the ACL
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ false);
        let subject_fqin = create_service_fqin_aidl("vm-allow", "com.oem", "TunnelService", "main");
        let object_fqin =
            create_service_fqin_aidl("unknown-vm", "com.google.sdv", "WindowManager", "main");

        // WHEN
        let is_authorized =
            authz_binder_service.isAuthorized(&subject_fqin, &object_fqin, "unknown-publisher");

        // THEN did not fail
        assert!(is_authorized.is_ok());
        // THEN access is granted
        assert!(is_authorized.unwrap());
    }

    #[test]
    fn test_is_authorized_invalid_subject_deny_despite_the_flag() {
        // GIVEN a request with an invalid subject
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ false);
        let subject_fqin =
            create_service_fqin_aidl("vm-deny", "com...oem", "TunnelService", "main");
        let object_fqin = create_service_fqin_aidl("vm", "com.google.sdv", "WindowManager", "main");

        // WHEN
        let is_authorized = authz_binder_service.isAuthorized(
            &subject_fqin,
            &object_fqin,
            "tire-pressure-publisher",
        );

        // THEN result is error
        assert!(is_authorized.is_err());
    }

    #[test]
    fn test_is_authorized_invalid_object_deny_despite_the_flag() {
        // GIVEN a request with an invalid subject
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ false);
        let subject_fqin = create_service_fqin_aidl("vm-deny", "com.oem", "TunnelService", "main");
        let object_fqin =
            create_service_fqin_aidl("vm", "com.google.sdv", "WindowManager", "--main");

        // WHEN
        let is_authorized = authz_binder_service.isAuthorized(
            &subject_fqin,
            &object_fqin,
            "tire-pressure-publisher",
        );

        // THEN result is error
        assert!(is_authorized.is_err());
    }

    #[test]
    fn test_is_authorized_invalid_service_unit_deny_despite_the_flag() {
        // GIVEN a request with an invalid subject
        let authz_binder_service =
            setup_authz_binder_service(/* is_enforcement_flag_enabled */ false);
        let subject_fqin = create_service_fqin_aidl("vm-deny", "com.oem", "TunnelService", "main");
        let object_fqin = create_service_fqin_aidl("vm", "com.google.sdv", "WindowManager", "main");

        // WHEN
        let is_authorized = authz_binder_service.isAuthorized(
            &subject_fqin,
            &object_fqin,
            "--tire-pressure-publisher",
        );

        // THEN result is error
        assert!(is_authorized.is_err());
    }
}
