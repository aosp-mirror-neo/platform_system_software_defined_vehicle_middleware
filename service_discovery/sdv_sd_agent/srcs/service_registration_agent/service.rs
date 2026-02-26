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
//! SDV Service Registration Agent

use binder::{ExceptionCode, Interface, Status};

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
    AccessControl::AccessControl, RegistrationToken::RegistrationToken as RegistrationTokenV2, UnitType::UnitType as UnitTypeAidl,
};
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::ApplicationMetadata::{
    ApplicationMetadata, TypedMetadata::TypedMetadata};
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceRegistrationAgent::IServiceRegistrationAgentAsyncServer;
use google_sdv_identity::aidl::google::sdv::identity::ProcessIdentifier::ProcessIdentifier;
use google_sdv_privileged_service_discovery::aidl::google::sdv::privileged::service_discovery::IPrivilegedServiceRegistrationAgent::IPrivilegedServiceRegistrationAgentAsyncServer as IPrivilegedServiceRegistrationAgentAsyncServer;

use crate::kv::process_identifier::get_calling_process_identifier;
use crate::kv::ServiceRegistry;

use std::str::FromStr;
use std::sync::Arc;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use log::{debug, error};
use tracing::instrument;

use sdv_authz::AuthzService;
use sdv_comms_id::{names::PackageName, names::ServiceUnitName, names::UnitTypeName, ServiceFqin};

/// Populates the topic name if it is currently empty.
///
/// Legacy publishers may not have a defined topic field. This function ensures
/// these publishers remain discoverable by SOA-based subscribers by defaulting
/// the topic to the publisher's `service_unit_name`.
fn ensure_publisher_has_topic(
    app_metadata: &ApplicationMetadata,
    service_unit_name: &str,
) -> ApplicationMetadata {
    let mut modified_app_metadata = app_metadata.clone();
    if let TypedMetadata::PublisherUnit(ref mut publisher_metadata) =
        modified_app_metadata.typedMetadata
    {
        if publisher_metadata.topic_name.is_empty() {
            debug!("Publisher topic empty; defaulting to service_unit_name: {}", service_unit_name);
            publisher_metadata.topic_name = service_unit_name.to_string();
        }
    }
    modified_app_metadata
}

/// Service responsible for (de) registration of service units.
pub(crate) struct ServiceRegistrationAgent {
    service_registry: Arc<ServiceRegistry>,
    authz_service: Arc<AuthzService>,
}

impl ServiceRegistrationAgent {
    pub(crate) fn new(
        service_registry: Arc<ServiceRegistry>,
        authz_service: Arc<AuthzService>,
    ) -> Self {
        ServiceRegistrationAgent { service_registry, authz_service }
    }

    fn register_service_unit_for_process(
        &self,
        service_unit_name: &str,
        unit_type: &UnitTypeAidl,
        acls: &AccessControl,
        app_metadata: &ApplicationMetadata,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<RegistrationTokenV2> {
        let service_unit_name_obj = self.validate_service_unit_name(service_unit_name)?;

        let updated_metadata = ensure_publisher_has_topic(app_metadata, service_unit_name);

        match self.is_process_authorized_for_unit_type(
            process_identifier,
            unit_type,
            &updated_metadata,
            &service_unit_name_obj,
        ) {
            Ok(true) => {
                // Process is authorized, so do nothing.
            }
            Ok(false) => {
                let msg = format!(
                    r#"Process is not authorized to register service unit for Unit Type "{}.{}""#,
                    unit_type.sdvPackageName, unit_type.typeName
                );
                error!("Authz violation. {msg}");
                return Err(Status::new_exception_str(ExceptionCode::SECURITY, Some(msg)));
            }
            Err(err) => {
                let msg = format!(
                    r#"Failed to validate register service unit input "{}.{}": {:?}"#,
                    unit_type.sdvPackageName, unit_type.typeName, err
                );
                error!("{msg}");
                return Err(Status::new_exception_str(ExceptionCode::ILLEGAL_ARGUMENT, Some(msg)));
            }
        };

        let token = self.service_registry.preregister_service_unit(
            service_unit_name,
            unit_type,
            acls,
            &updated_metadata,
            process_identifier,
        );
        token.ok_or(Status::new_exception(ExceptionCode::ILLEGAL_ARGUMENT, None))
    }

    fn unregister_service_unit_for_process(
        &self,
        service_unit_name: &str,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        let _ = self.validate_service_unit_name(service_unit_name)?;

        let Some(_) =
            self.service_registry.remove_local_service_unit(service_unit_name, process_identifier)
        else {
            return Err(Status::new_exception(ExceptionCode::ILLEGAL_ARGUMENT, None));
        };

        Ok(())
    }

    /// Returns Service FQIN of the process.
    fn get_process_fqin(
        &self,
        process_identifier: &ProcessIdentifier,
    ) -> Result<ServiceFqin, anyhow::Error> {
        let identity = self
            .service_registry
            .find_identity_by_process_identifier(process_identifier)
            .ok_or_else(|| anyhow!("Identity not found for process: {:?}", process_identifier))?;
        let aidl_fqin = identity.name;

        // TODO(b/374247340): Make SD properly validate FQIN and store
        // ServiceFqin instead of AIDL FQIN in the registry.
        let fqin = ServiceFqin::try_from(aidl_fqin).with_context(|| {
            format!("Failed to create valid FQIN for a process: {process_identifier:?}")
        })?;

        Ok(fqin)
    }

    /// Returns whether the process is authorized for the Unit Type.
    #[instrument(
        name = "sd::ServiceRegistrationAgent::is_process_authorized_for_unit_type",
        skip_all
    )]
    fn is_process_authorized_for_unit_type(
        &self,
        process_identifier: &ProcessIdentifier,
        unit_type: &UnitTypeAidl,
        app_metadata: &ApplicationMetadata,
        service_unit_name: &ServiceUnitName,
    ) -> Result<bool, anyhow::Error> {
        let subject_fqin = self
            .get_process_fqin(process_identifier)
            .with_context(|| "Failed to get process FQIN")?;

        let object_package_name = PackageName::from_str(&unit_type.sdvPackageName)
            .with_context(|| "Failed to validate Unit Type package name")?;

        let object_unit_type_name = UnitTypeName::from_str(&unit_type.typeName)
            .with_context(|| "Failed to validate Unit Type Name")?;

        Ok(self.authz_service.is_authorized_for_unit_type(
            &subject_fqin,
            &object_package_name,
            &object_unit_type_name,
            app_metadata,
            service_unit_name,
        ))
    }

    fn validate_service_unit_name(
        &self,
        service_unit_name: &str,
    ) -> binder::Result<ServiceUnitName> {
        ServiceUnitName::from_str(service_unit_name).map_err(|err| {
            let msg = format!("Invalid service unit name {service_unit_name} ({err:?}).");
            error!("{msg}");
            Status::new_exception_str(ExceptionCode::ILLEGAL_ARGUMENT, Some(msg))
        })
    }
}

impl Interface for ServiceRegistrationAgent {}

#[allow(non_snake_case)] // generated AIDL interface
#[async_trait]
impl IServiceRegistrationAgentAsyncServer for ServiceRegistrationAgent {
    #[instrument(name = "sd::IServiceRegistrationAgent::registerServiceUnit", skip_all)]
    async fn registerServiceUnit(
        &self,
        service_unit_name: &str,
        unit_type: &UnitTypeAidl,
        acls: &AccessControl,
        app_metadata: &ApplicationMetadata,
    ) -> binder::Result<RegistrationTokenV2> {
        self.register_service_unit_for_process(
            service_unit_name,
            unit_type,
            acls,
            app_metadata,
            &get_calling_process_identifier(),
        )
    }

    #[instrument(name = "sd::IServiceRegistrationAgent::unregisterServiceUnit", skip_all)]
    async fn unregisterServiceUnit(&self, service_unit_name: &str) -> binder::Result<()> {
        self.unregister_service_unit_for_process(
            service_unit_name,
            &get_calling_process_identifier(),
        )
    }
}

#[allow(non_snake_case)] // generated AIDL interface
#[async_trait]
impl IPrivilegedServiceRegistrationAgentAsyncServer for ServiceRegistrationAgent {
    #[instrument(name = "sd::IServiceRegistrationAgent::registerServiceUnitForProcess", skip_all)]
    async fn registerServiceUnitForProcess(
        &self,
        service_unit_name: &str,
        unit_type: &UnitTypeAidl,
        acls: &AccessControl,
        app_metadata: &ApplicationMetadata,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<RegistrationTokenV2> {
        self.register_service_unit_for_process(
            service_unit_name,
            unit_type,
            acls,
            app_metadata,
            process_identifier,
        )
    }

    #[instrument(name = "sd::IServiceRegistrationAgent::unregisterServiceUnitForProcess", skip_all)]
    async fn unregisterServiceUnitForProcess(
        &self,
        service_unit_name: &str,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        self.unregister_service_unit_for_process(service_unit_name, process_identifier)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::anyhow;
    use google_sdv_identity::aidl::google::sdv::identity::{
        ProcessIdentifier::ProcessIdentifier, ServiceFqin::ServiceFqin as ServiceFqinV2,
        ServiceIdentity::ServiceIdentity as ServiceIdentityV2,
    };
    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
        metadata::{
            ApplicationMetadata::ApplicationMetadata,
            ApplicationMetadata::TypedMetadata::TypedMetadata::PublisherUnit,
            ApplicationMetadata::TypedMetadata::TypedMetadata::ServerUnit,
            PublisherUnitMetadata::PublisherUnitMetadata,
            ServerUnitMetadata::ServerUnitMetadata,
        },
        AccessControl::AccessControl,
        UnitType::UnitType as UnitTypeAidl,
    };
    use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::ServiceUnitDefinition;
    use protobuf::text_format;
    use sdv_authz::{AclPolicyModel, AclPolicyProvider, AuthzEnforcementFlag};
    use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
    use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName, VmName};
    use std::collections::HashMap;

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
    }

    struct TestRegistry {}

    impl sdv_authz::ServiceUnitRegistry for TestRegistry {
        fn get_service_unit(
            &self,
            _fqin: &ServiceFqinV2,
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
        test_provider.set_alc_policy(
            object_fqin.vm_name().clone(),
            object_fqin.package_name().clone(),
            Some(object_fqin.service_name().clone()),
            acl_policy_model,
        );

        AuthzService::new(
            Arc::new(test_provider),
            authz_enforcement_flag,
            Arc::new(TestRegistry {}),
        )
    }

    fn setup_authz_service(authz_enforcement_flag: Arc<dyn AuthzEnforcementFlag>) -> AuthzService {
        let object_fqin =
            ServiceFqin::from_str("vm-allow:com.google.sdv.TireManager/main").unwrap();

        create_test_authz_service(
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
        )
    }

    fn create_agent(
        vm_name: String,
        unit_type: &UnitTypeAidl,
        caller_pid: &ProcessIdentifier,
        is_enforcement_flag_enabled: bool,
    ) -> ServiceRegistrationAgent {
        let service_identity = ServiceIdentityV2 {
            name: ServiceFqinV2 {
                sdvVmName: vm_name,
                sdvPackageName: unit_type.sdvPackageName.clone(),
                serviceBundleName: unit_type.serviceBundleName.clone(),
                serviceInstanceName: "instance1".to_string(),
            },
            ..Default::default()
        };

        let service_registry = ServiceRegistry::default();
        let _ = service_registry
            .register_identity_v2(service_identity.clone(), None, caller_pid)
            .unwrap();
        let authz_flag = Arc::new(TestAuthzEnforcementFlag::new(is_enforcement_flag_enabled));
        let authz_service = setup_authz_service(authz_flag);

        ServiceRegistrationAgent {
            service_registry: Arc::new(service_registry),
            authz_service: Arc::new(authz_service),
        }
    }

    #[test]
    fn test_register_service_unit_allow() {
        // GIVEN a valid request
        let unit_type = UnitTypeAidl {
            sdvPackageName: "com.google.sdv".into(),
            serviceBundleName: "TireManager".into(),
            typeName: "TirePressureTopic".into(),
        };

        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let agent = create_agent("vm-allow".to_string(), &unit_type, &caller_pid, true);

        let acl: AccessControl = AccessControl { ..Default::default() };
        let app_metadata = ApplicationMetadata {
            typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
            valueHolder: "test-value:1".into(),
        };

        // WHEN registering a service unit
        let result = agent.register_service_unit_for_process(
            "tire-pressure-publisher",
            &unit_type,
            &acl,
            &app_metadata,
            &caller_pid,
        );

        // THEN registration succeeds
        assert!(result.is_ok());
    }

    #[test]
    fn test_register_service_unit_unauthorized_deny() {
        // GIVEN a request that is not authorized (because coming from vm-deny)
        let unit_type = UnitTypeAidl {
            sdvPackageName: "com.google.sdv".into(),
            serviceBundleName: "TireManager".into(),
            typeName: "TirePressureTopic".into(),
        };

        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let agent = create_agent("vm-deny".to_string(), &unit_type, &caller_pid, true);

        let acl: AccessControl = AccessControl { ..Default::default() };
        let app_metadata = ApplicationMetadata {
            typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
            valueHolder: "test-value:1".into(),
        };

        // WHEN registering a service unit
        let result = agent.register_service_unit_for_process(
            "tire-pressure-publisher",
            &unit_type,
            &acl,
            &app_metadata,
            &caller_pid,
        );

        // THEN
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().exception_code(), ExceptionCode::SECURITY);
    }

    #[test]
    fn test_register_service_unit_invalid_service_unit_name_deny() {
        // GIVEN an invalid unit type (service unit name empty)
        let unit_type = UnitTypeAidl {
            sdvPackageName: "com.google.sdv".into(),
            serviceBundleName: "WindowManager".into(),
            typeName: "SomeType".into(),
        };

        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let agent = create_agent("vm-allow".to_string(), &unit_type, &caller_pid, true);

        let acl: AccessControl = AccessControl { ..Default::default() };
        let app_metadata = ApplicationMetadata {
            typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
            valueHolder: "test-value:1".into(),
        };

        // WHEN
        let result = agent.register_service_unit_for_process(
            "invalid_service_unit_name",
            &unit_type,
            &acl,
            &app_metadata,
            &caller_pid,
        );

        // THEN registration failed
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().exception_code(), ExceptionCode::ILLEGAL_ARGUMENT);
    }

    // TODO(b/374247340): Add test for checking that requests with invalid FQIN are denied

    #[test]
    fn test_register_service_unit_invalid_package_name_deny() {
        // GIVEN an invalid unit type (package name empty)
        let unit_type = UnitTypeAidl {
            sdvPackageName: "".into(),
            serviceBundleName: "WindowManager".into(),
            typeName: "SomeType".into(),
        };

        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let agent = create_agent("vm-allow".to_string(), &unit_type, &caller_pid, true);

        let acl: AccessControl = AccessControl { ..Default::default() };
        let app_metadata = ApplicationMetadata {
            typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
            valueHolder: "test-value:1".into(),
        };

        // WHEN
        let result = agent.register_service_unit_for_process(
            "tire-pressure-publisher",
            &unit_type,
            &acl,
            &app_metadata,
            &caller_pid,
        );

        // THEN registration failed
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().exception_code(), ExceptionCode::ILLEGAL_ARGUMENT);
    }

    #[test]
    fn test_register_service_unit_invalid_service_bundle_name_deny() {
        // GIVEN an invalid unit type (service bundle name empty)
        let unit_type = UnitTypeAidl {
            sdvPackageName: "com.google.sdv".into(),
            serviceBundleName: "".into(),
            typeName: "SomeType".into(),
        };

        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let agent = create_agent("vm-allow".to_string(), &unit_type, &caller_pid, true);

        let acl: AccessControl = AccessControl { ..Default::default() };
        let app_metadata = ApplicationMetadata {
            typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
            valueHolder: "test-value:1".into(),
        };

        // WHEN
        let result = agent.register_service_unit_for_process(
            "tire-pressure-publisher",
            &unit_type,
            &acl,
            &app_metadata,
            &caller_pid,
        );

        // THEN registration failed
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().exception_code(), ExceptionCode::ILLEGAL_ARGUMENT);
    }

    #[test]
    fn test_register_service_unit_invalid_unit_type_name_deny() {
        // GIVEN an invalid unit type (unit type name empty)
        let unit_type = UnitTypeAidl {
            sdvPackageName: "com.google.sdv".into(),
            serviceBundleName: "WindowManager     ".into(),
            typeName: "".into(),
        };

        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let agent = create_agent("vm-allow".to_string(), &unit_type, &caller_pid, true);

        let acl: AccessControl = AccessControl { ..Default::default() };
        let app_metadata = ApplicationMetadata {
            typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
            valueHolder: "test-value:1".into(),
        };

        // WHEN
        let result = agent.register_service_unit_for_process(
            "tire-pressure-publisher",
            &unit_type,
            &acl,
            &app_metadata,
            &caller_pid,
        );

        // THEN registration failed
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().exception_code(), ExceptionCode::ILLEGAL_ARGUMENT);
    }

    #[test]
    fn test_register_service_unit_allow_allow_by_flag() {
        // GIVEN a valid request
        let unit_type = UnitTypeAidl {
            sdvPackageName: "com.google.sdv".into(),
            serviceBundleName: "TireManager".into(),
            typeName: "TirePressureTopic".into(),
        };

        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let agent = create_agent("vm-allow".to_string(), &unit_type, &caller_pid, false);

        let acl: AccessControl = AccessControl { ..Default::default() };
        let app_metadata = ApplicationMetadata {
            typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
            valueHolder: "test-value:1".into(),
        };

        // WHEN registering a service unit
        let result = agent.register_service_unit_for_process(
            "tire-pressure-publisher",
            &unit_type,
            &acl,
            &app_metadata,
            &caller_pid,
        );

        // THEN registration succeeds
        assert!(result.is_ok());
    }

    #[test]
    fn test_register_service_unit_unauthorized_deny_allow_by_flag() {
        // GIVEN a request that is not authorized (because coming from vm-deny)
        let unit_type = UnitTypeAidl {
            sdvPackageName: "com.google.sdv".into(),
            serviceBundleName: "TireManager".into(),
            typeName: "TirePressureTopic".into(),
        };

        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let agent = create_agent("vm-deny".to_string(), &unit_type, &caller_pid, false);

        let acl: AccessControl = AccessControl { ..Default::default() };
        let app_metadata = ApplicationMetadata {
            typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
            valueHolder: "test-value:1".into(),
        };

        // WHEN registering a service unit
        let result = agent.register_service_unit_for_process(
            "tire-pressure-publisher",
            &unit_type,
            &acl,
            &app_metadata,
            &caller_pid,
        );

        // THEN registration is allowed by flag despite the unauthorized input
        assert!(result.is_ok());
    }

    #[test]
    fn test_register_service_unit_invalid_service_unit_name_deny_despite_the_flag() {
        // GIVEN an invalid unit type (service unit name empty)
        let unit_type = UnitTypeAidl {
            sdvPackageName: "com.google.sdv".into(),
            serviceBundleName: "WindowManager".into(),
            typeName: "SomeType".into(),
        };

        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let agent = create_agent("vm-allow".to_string(), &unit_type, &caller_pid, false);

        let acl: AccessControl = AccessControl { ..Default::default() };
        let app_metadata = ApplicationMetadata {
            typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
            valueHolder: "test-value:1".into(),
        };

        // WHEN
        let result = agent.register_service_unit_for_process(
            "invalid_service_unit_name",
            &unit_type,
            &acl,
            &app_metadata,
            &caller_pid,
        );

        // THEN registration failed
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().exception_code(), ExceptionCode::ILLEGAL_ARGUMENT);
    }

    #[test]
    fn test_register_service_unit_invalid_package_name_deny_despite_the_flag() {
        // GIVEN an invalid unit type (package name empty)
        let unit_type = UnitTypeAidl {
            sdvPackageName: "".into(),
            serviceBundleName: "WindowManager".into(),
            typeName: "SomeType".into(),
        };

        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let agent = create_agent("vm-allow".to_string(), &unit_type, &caller_pid, false);

        let acl: AccessControl = AccessControl { ..Default::default() };
        let app_metadata = ApplicationMetadata {
            typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
            valueHolder: "test-value:1".into(),
        };

        // WHEN
        let result = agent.register_service_unit_for_process(
            "tire-pressure-publisher",
            &unit_type,
            &acl,
            &app_metadata,
            &caller_pid,
        );

        // THEN registration failed
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().exception_code(), ExceptionCode::ILLEGAL_ARGUMENT);
    }

    #[test]
    fn test_register_service_unit_invalid_service_bundle_name_deny_despite_the_flag() {
        // GIVEN an invalid unit type (service bundle name empty)
        let unit_type = UnitTypeAidl {
            sdvPackageName: "com.google.sdv".into(),
            serviceBundleName: "".into(),
            typeName: "SomeType".into(),
        };

        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let agent = create_agent("vm-allow".to_string(), &unit_type, &caller_pid, false);

        let acl: AccessControl = AccessControl { ..Default::default() };
        let app_metadata = ApplicationMetadata {
            typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
            valueHolder: "test-value:1".into(),
        };

        // WHEN
        let result = agent.register_service_unit_for_process(
            "tire-pressure-publisher",
            &unit_type,
            &acl,
            &app_metadata,
            &caller_pid,
        );

        // THEN registration failed
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().exception_code(), ExceptionCode::ILLEGAL_ARGUMENT);
    }

    #[test]
    fn test_register_service_unit_invalid_unit_type_name_deny_despite_the_flag() {
        // GIVEN an invalid unit type (unit type name empty)
        let unit_type = UnitTypeAidl {
            sdvPackageName: "com.google.sdv".into(),
            serviceBundleName: "WindowManager     ".into(),
            typeName: "".into(),
        };

        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let agent = create_agent("vm-allow".to_string(), &unit_type, &caller_pid, false);

        let acl: AccessControl = AccessControl { ..Default::default() };
        let app_metadata = ApplicationMetadata {
            typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
            valueHolder: "test-value:1".into(),
        };

        // WHEN
        let result = agent.register_service_unit_for_process(
            "tire-pressure-publisher",
            &unit_type,
            &acl,
            &app_metadata,
            &caller_pid,
        );

        // THEN registration failed
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().exception_code(), ExceptionCode::ILLEGAL_ARGUMENT);
    }

    #[test]
    fn test_ensure_publisher_has_topic_empty_topic() {
        let service_unit_name = "test-service-unit";

        // Publisher metadata with empty topic name
        let app_metadata_empty_topic = ApplicationMetadata {
            typedMetadata: PublisherUnit(PublisherUnitMetadata {
                topic_name: "".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let updated_metadata =
            ensure_publisher_has_topic(&app_metadata_empty_topic, service_unit_name);
        if let PublisherUnit(publisher_metadata) = updated_metadata.typedMetadata {
            assert_eq!(publisher_metadata.topic_name, service_unit_name);
        } else {
            panic!("Expected PublisherUnit metadata");
        }
    }

    #[test]
    fn test_ensure_publisher_has_topic_existing_topic() {
        let service_unit_name = "test-service-unit";
        // Publisher metadata with non-empty topic name
        let app_metadata_with_topic = ApplicationMetadata {
            typedMetadata: PublisherUnit(PublisherUnitMetadata {
                topic_name: "existing-topic".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let updated_metadata =
            ensure_publisher_has_topic(&app_metadata_with_topic, service_unit_name);
        if let PublisherUnit(publisher_metadata) = updated_metadata.typedMetadata {
            assert_eq!(publisher_metadata.topic_name, "existing-topic");
        } else {
            panic!("Expected PublisherUnit metadata");
        }
    }

    #[test]
    fn test_ensure_publisher_has_topic_non_publisher() {
        let service_unit_name = "test-service-unit";
        // Non-publisher metadata
        let app_metadata_server = ApplicationMetadata {
            typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
            ..Default::default()
        };
        let updated_metadata = ensure_publisher_has_topic(&app_metadata_server, service_unit_name);
        assert_eq!(updated_metadata, app_metadata_server);
    }

    // TODO(b/384016427): Write test for the case that register_service_unit_for_process fails to create a token and returns ExceptionCode::ILLEGAL_ARGUMENT.
    // `preregister_service_unit` doesn't do any checks, so currently the token will always be valid.
}
