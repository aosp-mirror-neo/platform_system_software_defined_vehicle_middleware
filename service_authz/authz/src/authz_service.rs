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
use crate::{AclPolicyProvider, AuthzEnforcementFlag, AuthzResult, AuthzStrategy};

use sdv_comms_id::names::{PackageName, ServiceUnitName, UnitTypeName, VmName};
use sdv_comms_id::ServiceFqin;

use anyhow::Context;
use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinAidl;
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::ApplicationMetadata::ApplicationMetadata;
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
        current_vm_name: VmName,
        acl_policy_provider: Arc<dyn AclPolicyProvider>,
        authz_enforcement_flag: Arc<dyn AuthzEnforcementFlag>,
        service_unit_registry: Arc<dyn ServiceUnitRegistry>,
    ) -> Self {
        let authz_via_acls = AuthzViaAcls::new(acl_policy_provider.clone());
        let authz_via_permissions =
            AuthzViaPermissions::new(current_vm_name, acl_policy_provider, service_unit_registry);

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

        let permission_result = self.authz_via_permissions.check_authorization_for_service_unit(
            subject_fqin,
            object_fqin,
            object_service_unit_name,
        );

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

        let permission_result =
            self.authz_via_permissions.check_authorization_for_sud(subject_fqin, object_sud);

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
        app_metadata: &ApplicationMetadata,
        service_unit_name: &ServiceUnitName,
    ) -> bool {
        let object_log_identifier = format!("{object_package_name}.{object_unit_type_name}");

        let acl_result = self.authz_via_acls.check_authorization_for_unit_type(
            subject_fqin,
            object_package_name,
            object_unit_type_name,
        );

        let permission_result = self.authz_via_permissions.check_authorization_for_unit_type(
            subject_fqin,
            object_package_name,
            object_unit_type_name,
            app_metadata,
            service_unit_name,
        );

        // Use both results to make final decision and log denial if needed.
        self.handle_authz_result(subject_fqin, object_log_identifier, acl_result, permission_result)
    }

    fn log_permissions_violation(
        subject_fqin: &ServiceFqin,
        object_log_identifier: &str,
        audit_msg: &str,
    ) {
        error!(
            "Authz violation. {subject_fqin} does not declare permission to access {object_log_identifier}, audit message: {audit_msg}"
        );
    }

    fn log_permissions_soft_violation(
        subject_fqin: &ServiceFqin,
        object_log_identifier: &str,
        audit_msg: &str,
    ) {
        debug!(
            "Permissions Authz violation (access granted by ACLs): {subject_fqin} does not declare permission to access {object_log_identifier}, audit message: {audit_msg}"
        );
    }

    fn log_permissions_granted(
        subject_fqin: &ServiceFqin,
        object_log_identifier: &str,
        audit_msg: &str,
    ) {
        debug!(
            "{subject_fqin} declares permission to access {object_log_identifier}, audit message: {audit_msg}"
        );
    }

    fn log_acls_violation(
        subject_fqin: &ServiceFqin,
        object_log_identifier: &str,
        audit_msg: &str,
    ) {
        error!(
            "Authz violation. {object_log_identifier} denies access to {subject_fqin}, audit message: {audit_msg}"
        );
    }

    fn log_acls_granted(subject_fqin: &ServiceFqin, object_log_identifier: &str, audit_msg: &str) {
        debug!(
            "{object_log_identifier} allows access to {subject_fqin}, audit message: {audit_msg}"
        );
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

        match self.authz_enforcement_flag.get_enforcement_strategy() {
            AuthzStrategy::PermissionsOnly => {
                if !is_authorized_by_permissions {
                    Self::log_permissions_violation(
                        subject_fqin,
                        &object_log_identifier,
                        &audit_msg_from_permissions,
                    );
                } else {
                    Self::log_permissions_granted(
                        subject_fqin,
                        &object_log_identifier,
                        &audit_msg_from_permissions,
                    );
                }
                is_authorized_by_permissions
            }
            AuthzStrategy::Lenient => {
                if is_authorized_by_acls || is_authorized_by_permissions {
                    if is_authorized_by_acls {
                        Self::log_acls_granted(
                            subject_fqin,
                            &object_log_identifier,
                            &audit_msg_from_acls,
                        );
                    }
                    if is_authorized_by_permissions {
                        Self::log_permissions_granted(
                            subject_fqin,
                            &object_log_identifier,
                            &audit_msg_from_permissions,
                        );
                    }

                    // Log soft violation if ACLs grant access, but permissions do not, to simplify migration.
                    if is_authorized_by_acls && !is_authorized_by_permissions {
                        Self::log_permissions_soft_violation(
                            subject_fqin,
                            &object_log_identifier,
                            &audit_msg_from_permissions,
                        );
                    }
                } else {
                    Self::log_permissions_violation(
                        subject_fqin,
                        &object_log_identifier,
                        &audit_msg_from_permissions,
                    );
                    Self::log_acls_violation(
                        subject_fqin,
                        &object_log_identifier,
                        &audit_msg_from_acls,
                    );
                }
                is_authorized_by_acls || is_authorized_by_permissions
            }
            AuthzStrategy::Strict => {
                if is_authorized_by_acls && is_authorized_by_permissions {
                    Self::log_acls_granted(
                        subject_fqin,
                        &object_log_identifier,
                        &audit_msg_from_acls,
                    );
                    Self::log_permissions_granted(
                        subject_fqin,
                        &object_log_identifier,
                        &audit_msg_from_permissions,
                    );
                } else {
                    if !is_authorized_by_permissions {
                        Self::log_permissions_violation(
                            subject_fqin,
                            &object_log_identifier,
                            &audit_msg_from_permissions,
                        );
                    }
                    if !is_authorized_by_acls {
                        Self::log_acls_violation(
                            subject_fqin,
                            &object_log_identifier,
                            &audit_msg_from_acls,
                        );
                    }
                }
                is_authorized_by_acls && is_authorized_by_permissions
            }
            AuthzStrategy::AclsOnly => {
                if !is_authorized_by_acls {
                    Self::log_acls_violation(
                        subject_fqin,
                        &object_log_identifier,
                        &audit_msg_from_acls,
                    );
                } else {
                    Self::log_acls_granted(
                        subject_fqin,
                        &object_log_identifier,
                        &audit_msg_from_acls,
                    );
                }
                is_authorized_by_acls
            }
            AuthzStrategy::Disabled => true,
        }
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

    use crate::{AclPolicyModel, TestAclPolicyProvider};

    use std::str::FromStr;

    use protobuf::text_format;
    use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
    use sdv_comms_id::names::ServiceUnitName;
    use sdv_comms_sd::discovery::UnitType;

    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::
    {ApplicationMetadata::TypedMetadata::TypedMetadata, PublisherUnitMetadata::PublisherUnitMetadata};

    struct TestAuthzEnforcementFlag {
        enforcement_strategy: AuthzStrategy,
    }

    impl TestAuthzEnforcementFlag {
        fn new(enforcement_strategy: AuthzStrategy) -> Self {
            Self { enforcement_strategy }
        }
    }

    impl AuthzEnforcementFlag for TestAuthzEnforcementFlag {
        fn get_enforcement_strategy(&self) -> AuthzStrategy {
            self.enforcement_strategy
        }
    }

    struct TestRegistry {}

    impl ServiceUnitRegistry for TestRegistry {
        fn get_service_unit(
            &self,
            _fqin: &ServiceFqinAidl,
            _unit_name: &str,
        ) -> Option<ServiceUnitDefinition> {
            let object_service_unit_name = object_service_unit_name();
            let app_metadata = app_metadata();
            Some(ServiceUnitDefinition {
                serviceUnitName: object_service_unit_name.to_string(),
                unitType: UnitType::from_str("com.android.TirePressureTopic").unwrap().into(),
                appMetadata: app_metadata,
                ..Default::default()
            })
        }
    }

    fn subject_with_allowed_by_permissions_and_acls() -> ServiceFqin {
        ServiceFqin::from_str("vm:com.android.permissions_and_acls.Bundle/default").unwrap()
    }

    fn subject_with_allowed_by_permissions_only() -> ServiceFqin {
        ServiceFqin::from_str("vm:com.android.permissions_only.Bundle/default").unwrap()
    }

    fn subject_with_allowed_by_acls_only() -> ServiceFqin {
        ServiceFqin::from_str("vm:com.android.acls_only.Bundle/default").unwrap()
    }

    fn subject_with_denied_by_permissions_and_acls() -> ServiceFqin {
        ServiceFqin::from_str("vm:com.android.none.SubjectBundle/default").unwrap()
    }

    fn object_fqin() -> ServiceFqin {
        ServiceFqin::from_str("vm:com.android.ObjectBundle/default").unwrap()
    }

    fn object_service_unit_name() -> ServiceUnitName {
        ServiceUnitName::from_str("tire-pressure-publisher").unwrap()
    }

    fn object_unit_type_name() -> UnitTypeName {
        UnitTypeName::from_str("TirePressureTopic").unwrap()
    }

    fn app_metadata() -> ApplicationMetadata {
        ApplicationMetadata {
            typedMetadata: TypedMetadata::PublisherUnit(PublisherUnitMetadata {
                topic_name: "tire-pressure-publisher".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn set_authz_policy(
        acl_provider: &mut TestAclPolicyProvider,
        object_fqin: &ServiceFqin,
        policy_textproto: &str,
    ) {
        acl_provider.set_authz_policy(
            object_fqin.vm_name().clone(),
            object_fqin.package_name().clone(),
            Some(object_fqin.service_name().clone()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(policy_textproto)
                    .expect("Valid policy textproto"),
            )
            .expect("Valid policy"),
        );
    }

    fn setup_test(enforcement_strategy: AuthzStrategy) -> AuthzService {
        let object_fqin = object_fqin();

        let mut test_acl_provider = TestAclPolicyProvider::new();
        set_authz_policy(
            &mut test_acl_provider,
            &object_fqin,
            r#"
                acls {
                    name: "TirePressureAcl"
                    include: "vm:com.android.permissions_and_acls.Bundle/default"
                    include: "vm:com.android.acls_only.Bundle/default"
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
        );

        set_authz_policy(
            &mut test_acl_provider,
            &subject_with_allowed_by_permissions_and_acls(),
            r#"
                publisher {
                    topic: "tire-pressure-publisher"
                    message: "com.android.TirePressureTopic"
                }
                subscriber {
                    topic: "tire-pressure-publisher"
                    message: "com.android.TirePressureTopic"
                }
            "#,
        );

        set_authz_policy(
            &mut test_acl_provider,
            &subject_with_allowed_by_permissions_only(),
            r#"
                publisher {
                    topic: "tire-pressure-publisher"
                    message: "com.android.TirePressureTopic"
                }
                subscriber {
                    topic: "tire-pressure-publisher"
                    message: "com.android.TirePressureTopic"
                }
            "#,
        );

        AuthzService::new(
            VmName::from_str("vm").unwrap(),
            Arc::new(test_acl_provider),
            Arc::new(TestAuthzEnforcementFlag::new(enforcement_strategy)),
            Arc::new(TestRegistry {}),
        )
    }

    #[test]
    fn test_is_authorized_for_service_unit_disabled() {
        let authz_service = setup_test(AuthzStrategy::Disabled);

        let allowed_subject_fqins = [
            subject_with_allowed_by_permissions_and_acls(),
            subject_with_allowed_by_permissions_only(),
            subject_with_allowed_by_acls_only(),
            subject_with_denied_by_permissions_and_acls(),
        ];

        for allowed_subject_fqin in allowed_subject_fqins {
            assert!(authz_service.is_authorized(
                &allowed_subject_fqin,
                &object_fqin(),
                &object_service_unit_name()
            ));
        }
    }

    #[test]
    fn test_is_authorized_for_service_unit_acls_only() {
        let authz_service = setup_test(AuthzStrategy::AclsOnly);

        let allowed_subject_fqins =
            [subject_with_allowed_by_permissions_and_acls(), subject_with_allowed_by_acls_only()];

        for allowed_subject_fqin in allowed_subject_fqins {
            assert!(authz_service.is_authorized(
                &allowed_subject_fqin,
                &object_fqin(),
                &object_service_unit_name()
            ));
        }

        for denied_subject_fqin in [
            subject_with_allowed_by_permissions_only(),
            subject_with_denied_by_permissions_and_acls(),
        ] {
            assert!(!authz_service.is_authorized(
                &denied_subject_fqin,
                &object_fqin(),
                &object_service_unit_name()
            ));
        }
    }

    #[test]
    fn test_is_authorized_for_service_unit_permissions_only() {
        let authz_service = setup_test(AuthzStrategy::PermissionsOnly);

        let allowed_subject_fqins = [
            subject_with_allowed_by_permissions_and_acls(),
            subject_with_allowed_by_permissions_only(),
        ];

        for allowed_subject_fqin in allowed_subject_fqins {
            assert!(authz_service.is_authorized(
                &allowed_subject_fqin,
                &object_fqin(),
                &object_service_unit_name()
            ));
        }

        for denied_subject_fqin in
            [subject_with_allowed_by_acls_only(), subject_with_denied_by_permissions_and_acls()]
        {
            assert!(!authz_service.is_authorized(
                &denied_subject_fqin,
                &object_fqin(),
                &object_service_unit_name()
            ));
        }
    }

    #[test]
    fn test_is_authorized_for_service_unit_lenient() {
        let authz_service = setup_test(AuthzStrategy::Lenient);

        let allowed_subject_fqins = [
            subject_with_allowed_by_permissions_and_acls(),
            subject_with_allowed_by_permissions_only(),
            subject_with_allowed_by_acls_only(),
        ];

        for allowed_subject_fqin in allowed_subject_fqins {
            assert!(authz_service.is_authorized(
                &allowed_subject_fqin,
                &object_fqin(),
                &object_service_unit_name()
            ));
        }

        assert!(!authz_service.is_authorized(
            &subject_with_denied_by_permissions_and_acls(),
            &object_fqin(),
            &object_service_unit_name()
        ));
    }

    #[test]
    fn test_is_authorized_for_service_unit_strict() {
        let authz_service = setup_test(AuthzStrategy::Strict);

        assert!(authz_service.is_authorized(
            &subject_with_allowed_by_permissions_and_acls(),
            &object_fqin(),
            &object_service_unit_name()
        ));

        for denied_subject_fqin in [
            subject_with_allowed_by_permissions_only(),
            subject_with_allowed_by_acls_only(),
            subject_with_denied_by_permissions_and_acls(),
        ] {
            assert!(!authz_service.is_authorized(
                &denied_subject_fqin,
                &object_fqin(),
                &object_service_unit_name()
            ));
        }
    }

    #[test]
    fn test_is_authorized_for_unit_type_disabled() {
        let authz_service = setup_test(AuthzStrategy::Disabled);

        let object_fqin = object_fqin();
        let object_unit_type_name = object_unit_type_name();
        let object_service_unit_name = object_service_unit_name();
        let app_metadata = app_metadata();

        for allowed_subject_fqin in [
            subject_with_allowed_by_permissions_and_acls(),
            subject_with_allowed_by_permissions_only(),
            subject_with_allowed_by_acls_only(),
            subject_with_denied_by_permissions_and_acls(),
        ] {
            assert!(authz_service.is_authorized_for_unit_type(
                &allowed_subject_fqin,
                object_fqin.package_name(),
                &object_unit_type_name,
                &app_metadata,
                &object_service_unit_name,
            ));
        }
    }

    #[test]
    fn test_is_authorized_for_unit_type_permissions_only() {
        let authz_service = setup_test(AuthzStrategy::PermissionsOnly);

        let object_fqin = object_fqin();
        let object_unit_type_name = object_unit_type_name();
        let object_service_unit_name = object_service_unit_name();
        let app_metadata = app_metadata();

        for allowed_subject_fqin in [
            subject_with_allowed_by_permissions_and_acls(),
            subject_with_allowed_by_permissions_only(),
        ] {
            assert!(authz_service.is_authorized_for_unit_type(
                &allowed_subject_fqin,
                object_fqin.package_name(),
                &object_unit_type_name,
                &app_metadata,
                &object_service_unit_name,
            ));
        }

        for denied_subject_fqin in
            [subject_with_allowed_by_acls_only(), subject_with_denied_by_permissions_and_acls()]
        {
            assert!(!authz_service.is_authorized_for_unit_type(
                &denied_subject_fqin,
                object_fqin.package_name(),
                &object_unit_type_name,
                &app_metadata,
                &object_service_unit_name,
            ));
        }
    }

    #[test]
    fn test_is_authorized_for_unit_type_acls_only() {
        let authz_service = setup_test(AuthzStrategy::AclsOnly);

        let object_fqin = object_fqin();
        let object_unit_type_name = object_unit_type_name();
        let object_service_unit_name = object_service_unit_name();
        let app_metadata = app_metadata();

        for allowed_subject_fqin in
            [subject_with_allowed_by_permissions_and_acls(), subject_with_allowed_by_acls_only()]
        {
            assert!(authz_service.is_authorized_for_unit_type(
                &allowed_subject_fqin,
                object_fqin.package_name(),
                &object_unit_type_name,
                &app_metadata,
                &object_service_unit_name,
            ));
        }

        for denied_subject_fqin in [
            subject_with_allowed_by_permissions_only(),
            subject_with_denied_by_permissions_and_acls(),
        ] {
            assert!(!authz_service.is_authorized_for_unit_type(
                &denied_subject_fqin,
                object_fqin.package_name(),
                &object_unit_type_name,
                &app_metadata,
                &object_service_unit_name,
            ));
        }
    }

    #[test]
    fn test_is_authorized_for_unit_type_lenient() {
        let authz_service = setup_test(AuthzStrategy::Lenient);

        let object_fqin = object_fqin();
        let object_unit_type_name = object_unit_type_name();
        let object_service_unit_name = object_service_unit_name();
        let app_metadata = app_metadata();

        for allowed_subject_fqin in [
            subject_with_allowed_by_permissions_and_acls(),
            subject_with_allowed_by_permissions_only(),
            subject_with_allowed_by_acls_only(),
        ] {
            assert!(authz_service.is_authorized_for_unit_type(
                &allowed_subject_fqin,
                object_fqin.package_name(),
                &object_unit_type_name,
                &app_metadata,
                &object_service_unit_name,
            ));
        }

        assert!(!authz_service.is_authorized_for_unit_type(
            &subject_with_denied_by_permissions_and_acls(),
            object_fqin.package_name(),
            &object_unit_type_name,
            &app_metadata,
            &object_service_unit_name,
        ));
    }

    #[test]
    fn test_is_authorized_for_unit_type_strict() {
        let authz_service = setup_test(AuthzStrategy::Strict);

        let object_fqin = object_fqin();
        let object_unit_type_name = object_unit_type_name();
        let object_service_unit_name = object_service_unit_name();
        let app_metadata = app_metadata();

        assert!(authz_service.is_authorized_for_unit_type(
            &subject_with_allowed_by_permissions_and_acls(),
            object_fqin.package_name(),
            &object_unit_type_name,
            &app_metadata,
            &object_service_unit_name,
        ));

        for denied_subject_fqin in [
            subject_with_allowed_by_permissions_only(),
            subject_with_allowed_by_acls_only(),
            subject_with_denied_by_permissions_and_acls(),
        ] {
            assert!(!authz_service.is_authorized_for_unit_type(
                &denied_subject_fqin,
                object_fqin.package_name(),
                &object_unit_type_name,
                &app_metadata,
                &object_service_unit_name,
            ));
        }
    }
}
