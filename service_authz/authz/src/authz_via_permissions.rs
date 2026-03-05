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

//! Permissions based Authz.

use crate::authz_service::ServiceUnitRegistry;
use crate::{AclPolicyProvider, AuthzResult};
use log::error;
use sdv_comms_id::{
    names::{PackageName, ServiceUnitName, UnitTypeName},
    ServiceFqin,
};
use sdv_comms_sd::discovery::UnitType;
use std::str::FromStr;
use std::sync::Arc;

use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinAidl;
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::ServiceUnitDefinition;

/// AuthzViaPermissions is an entry point for permission-based authorization checks.
pub struct AuthzViaPermissions {
    provider: Arc<dyn AclPolicyProvider>,
    registry: Arc<dyn ServiceUnitRegistry>,
}

impl AuthzViaPermissions {
    /// Create new AuthzViaPermissions instance.
    pub fn new(
        provider: Arc<dyn AclPolicyProvider>,
        registry: Arc<dyn ServiceUnitRegistry>,
    ) -> Self {
        Self { provider, registry }
    }

    /// Returns Subject FQIN authorization result for the Object Service Unit.
    pub fn check_authorization_for_service_unit(
        &self,
        subject_fqin: &ServiceFqin,
        object_fqin: &ServiceFqin,
        object_service_unit_name: &ServiceUnitName,
    ) -> AuthzResult {
        let fqin_aidl = ServiceFqinAidl::from(object_fqin.clone());
        let unit_definition = match self
            .registry
            .get_service_unit(&fqin_aidl, &object_service_unit_name.to_string())
        {
            Some(def) => def,
            None => {
                return AuthzResult::ImplicitlyDenied("Failed to get unit definition".to_string())
            }
        };

        self.check_authorization_for_sud(subject_fqin, &unit_definition)
    }

    /// Returns Subject FQIN authorization result for the Object Service Unit.
    pub fn check_authorization_for_sud(
        &self,
        subject_fqin: &ServiceFqin,
        object_sud: &ServiceUnitDefinition,
    ) -> AuthzResult {
        let object_unit_type = match UnitType::builder()
            .package_name(object_sud.unitType.sdvPackageName.clone())
            .unit_type_name(object_sud.unitType.typeName.clone())
            .build()
        {
            Ok(ut) => ut,
            Err(_) => {
                error!("Invalid unit type data in SUD {:?}", object_sud);
                return AuthzResult::ImplicitlyDenied("Invalid UnitType data".to_string());
            }
        };

        let object_service_unit_name = match ServiceUnitName::from_str(&object_sud.serviceUnitName)
        {
            Ok(name) => name,
            Err(err) => {
                error!("Failed to parse service unit name: {:?}", err);
                return AuthzResult::ImplicitlyDenied("Invalid service unit name data".to_string());
            }
        };

        let subject_policy = match self.provider.get_acl_policy(
            subject_fqin.vm_name(),
            subject_fqin.package_name(),
            Some(subject_fqin.service_name()),
        ) {
            Ok(policy) => policy,
            Err(e) => {
                return AuthzResult::ImplicitlyDenied(format!(
                    "ACL policy not found for subject '{}': {}",
                    subject_fqin, e
                ));
            }
        };

        if subject_policy.can_call(&object_unit_type, &object_service_unit_name) {
            return AuthzResult::Permitted(format!(
                "Permitted by 'client' permission on Unit Type '{}'",
                object_unit_type
            ));
        }

        AuthzResult::ExplicitlyDenied(format!(
            "Subject '{}' lacks 'client' permission for service unit '{}' with unit type '{}'",
            subject_fqin,
            object_service_unit_name.name(),
            object_unit_type
        ))
    }

    /// Returns Subject FQIN authorization result for the Object Unit Type.
    pub fn check_authorization_for_unit_type(
        &self,
        _subject_fqin: &ServiceFqin,
        _object_package_name: &PackageName,
        _object_unit_type_name: &UnitTypeName,
    ) -> AuthzResult {
        AuthzResult::ImplicitlyDenied("permissions check not implemented".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AclPolicyModel;
    use crate::TestAclPolicyProvider;
    use crate::feature_flags;
    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::UnitType::UnitType as AidlUnitType;
    use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::ServiceUnitDefinition;
    use protobuf::text_format;
    use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
    use sdv_comms_id::names::{ServiceName as ServiceBundleName, VmName};
    use std::str::FromStr;

    struct MockRegistry {
        definition: ServiceUnitDefinition,
    }

    impl ServiceUnitRegistry for MockRegistry {
        fn get_service_unit(
            &self,
            _fqin: &ServiceFqinAidl,
            _unit_name: &str,
        ) -> Option<ServiceUnitDefinition> {
            // Simply return a clone of the pre-configured definition
            Some(self.definition.clone())
        }
    }

    #[test]
    fn test_check_authorization_success() {
        if !feature_flags::enable_permissions() {
            return;
        }

        // 1. Setup Policy: Allow calls to 'TargetType' on specific unit 'correct-unit'
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("vm").unwrap(),
            PackageName::from_str("com.client").unwrap(),
            Some(ServiceBundleName::from_str("ClientBundle").unwrap()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"
                        client {
                            service: "com.server.TargetType"
                            channel: "correct-unit"
                        }
                    "#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        // 2. Setup Mock Registry with a SUD
        let fake_definition = ServiceUnitDefinition {
            unitType: AidlUnitType {
                sdvPackageName: "com.server".to_string(),
                serviceBundleName: "ServerBundle".to_string(),
                typeName: "TargetType".to_string(),
            },
            serviceUnitName: "correct-unit".to_string(),
            ..Default::default()
        };
        let registry = Arc::new(MockRegistry { definition: fake_definition });

        let service = AuthzViaPermissions::new(Arc::new(test_provider), registry);

        // 4. Perform Check
        let result = service.check_authorization_for_service_unit(
            &ServiceFqin::from_str("vm:com.client.ClientBundle/service").unwrap(),
            &ServiceFqin::from_str("vm:com.server.ServerBundle/service").unwrap(),
            &ServiceUnitName::from_str("correct-unit").unwrap(),
        );

        // 5. Assert Success
        match result {
            AuthzResult::Permitted(msg) => {
                assert!(
                    msg.contains("Permitted by 'client' permission"),
                    "Unexpected msg: {}",
                    msg
                );
            }
            _ => panic!("Expected Permitted, got {:?}", result),
        }
    }

    #[test]
    fn test_check_authorization_no_policy_found() {
        if !feature_flags::enable_permissions() {
            return;
        }
        let test_provider = TestAclPolicyProvider::new();
        let fake_definition = ServiceUnitDefinition {
            unitType: AidlUnitType {
                sdvPackageName: "com.d".to_string(),
                serviceBundleName: "E".to_string(),
                typeName: "TargetType".to_string(),
            },
            serviceUnitName: "unit".to_string(),
            ..Default::default()
        };
        let registry = Arc::new(MockRegistry { definition: fake_definition });
        let service = AuthzViaPermissions::new(Arc::new(test_provider), registry);

        let result = service.check_authorization_for_service_unit(
            &ServiceFqin::from_str("vm:com.unknown.App/service").unwrap(),
            &ServiceFqin::from_str("vm:com.d.E/f").unwrap(),
            &ServiceUnitName::from_str("unit").unwrap(),
        );

        match result {
            AuthzResult::ImplicitlyDenied(msg) => {
                assert!(msg.contains("policy not found"), "Unexpected msg: {}", msg);
            }
            _ => panic!("Expected ImplicitlyDenied, got {:?}", result),
        }
    }

    #[test]
    fn test_check_authorization_registry_miss() {
        if !feature_flags::enable_permissions() {
            return;
        }
        struct EmptyRegistry;
        impl ServiceUnitRegistry for EmptyRegistry {
            fn get_service_unit(
                &self,
                _: &ServiceFqinAidl,
                _: &str,
            ) -> Option<ServiceUnitDefinition> {
                None
            }
        }

        let service = AuthzViaPermissions::new(
            Arc::new(TestAclPolicyProvider::new()),
            Arc::new(EmptyRegistry),
        );

        let result = service.check_authorization_for_service_unit(
            &ServiceFqin::from_str("vm:com.a.B/c").unwrap(),
            &ServiceFqin::from_str("vm:com.d.E/f").unwrap(),
            &ServiceUnitName::from_str("missing-unit").unwrap(),
        );

        match result {
            AuthzResult::ImplicitlyDenied(msg) => {
                assert!(msg.contains("Failed to get unit definition"));
            }
            _ => panic!("Expected ImplicitlyDenied, got {:?}", result),
        }
    }

    #[test]
    fn test_check_authorization_invalid_sud_unit_name() {
        if !feature_flags::enable_permissions() {
            return;
        }
        let fake_definition = ServiceUnitDefinition {
            unitType: AidlUnitType {
                sdvPackageName: "com.server".to_string(),
                serviceBundleName: "Bundle".to_string(),
                typeName: "Type".to_string(),
            },
            serviceUnitName: "".to_string(), // Invalid name
            ..Default::default()
        };

        let registry = Arc::new(MockRegistry { definition: fake_definition });
        let service = AuthzViaPermissions::new(Arc::new(TestAclPolicyProvider::new()), registry);

        let result = service.check_authorization_for_sud(
            &ServiceFqin::from_str("vm:com.client.App/s").unwrap(),
            &service.registry.get_service_unit(&ServiceFqinAidl::default(), "").unwrap(),
        );

        match result {
            AuthzResult::ImplicitlyDenied(msg) => {
                assert!(msg.contains("Invalid service unit name data"));
            }
            _ => panic!("Expected ImplicitlyDenied, got {:?}", result),
        }
    }

    #[test]
    fn test_check_authorization_explicit_denial() {
        if !feature_flags::enable_permissions() {
            return;
        }
        // 1. Setup Policy that allows "OtherType" but NOT "TargetType"
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("vm").unwrap(),
            PackageName::from_str("com.client").unwrap(),
            Some(ServiceBundleName::from_str("ClientBundle").unwrap()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"
                        client {
                            service: "com.server.OtherType"
                            channel: "correct-unit"
                        }
                    "#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        let fake_definition = ServiceUnitDefinition {
            unitType: AidlUnitType {
                sdvPackageName: "com.server".to_string(),
                serviceBundleName: "ServerBundle".to_string(),
                typeName: "TargetType".to_string(),
            },
            serviceUnitName: "correct-unit".to_string(),
            ..Default::default()
        };
        let registry = Arc::new(MockRegistry { definition: fake_definition });
        let service = AuthzViaPermissions::new(Arc::new(test_provider), registry);

        // 2. Perform Check - should fail because TargetType != OtherType
        let result = service.check_authorization_for_service_unit(
            &ServiceFqin::from_str("vm:com.client.ClientBundle/service").unwrap(),
            &ServiceFqin::from_str("vm:com.server.ServerBundle/service").unwrap(),
            &ServiceUnitName::from_str("correct-unit").unwrap(),
        );

        match result {
            AuthzResult::ExplicitlyDenied(msg) => {
                assert!(msg.contains("lacks 'client' permission"));
                assert!(msg.contains("TargetType"));
            }
            _ => panic!("Expected ExplicitlyDenied, got {:?}", result),
        }
    }
}
