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
use sdv_comms::sd::UnitType;
use sdv_comms_id::{
    names::{PackageName, ServiceUnitName, UnitTypeName},
    ServiceFqin,
};
use std::sync::Arc;

use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinAidl;

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

        let object_unit_type = match UnitType::builder()
            .set_sdv_package_name(unit_definition.unitType.sdvPackageName.clone())
            .set_service_bundle_name(unit_definition.unitType.serviceBundleName.clone())
            .set_unit_type_name(unit_definition.unitType.typeName.clone())
            .build()
        {
            Ok(ut) => ut,
            Err(_) => {
                return AuthzResult::ImplicitlyDenied(
                    "Registry returned invalid UnitType data".to_string(),
                )
            }
        };

        if subject_policy.can_call(&object_unit_type, object_service_unit_name) {
            return AuthzResult::Permitted(format!(
                "Permitted by 'client' permission on Unit Type '{}'",
                object_unit_type.get_unit_type_name()
            ));
        }

        AuthzResult::ExplicitlyDenied(format!(
            "Subject '{}' lacks 'client' permission for service unit '{}' on object '{}' with unit type '{}'",
            subject_fqin,
            object_service_unit_name.name(),
            object_fqin,
            object_unit_type.get_unit_type_name()
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
        let test_provider = TestAclPolicyProvider::new();
        let registry = Arc::new(MockRegistry { definition: ServiceUnitDefinition::default() });
        let service = AuthzViaPermissions::new(Arc::new(test_provider), registry);

        let result = service.check_authorization_for_service_unit(
            &ServiceFqin::from_str("vm:com.unknown.App/service").unwrap(),
            &ServiceFqin::from_str("vm:com.d.E/f").unwrap(),
            &ServiceUnitName::from_str("unit").unwrap(),
        );

        assert!(matches!(result, AuthzResult::ImplicitlyDenied(_)));
    }
}
