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
use crate::{AclPolicyModel, AclPolicyProvider, AuthzResult, VmAuthzPolicyModel};
use log::error;
use sdv_comms_id::{
    names::{PackageName, ServiceUnitName, TopicName, UnitTypeName, VmName},
    ServiceFqin,
};
use sdv_comms_sd::discovery::UnitType;
use std::str::FromStr;
use std::sync::Arc;

use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinAidl;
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::ApplicationMetadata::{
    ApplicationMetadata, TypedMetadata::TypedMetadata};
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::PublisherUnitMetadata::PublisherUnitMetadata;
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::ServiceUnitDefinition;

/// AuthzViaPermissions is an entry point for permission-based authorization checks.
pub struct AuthzViaPermissions {
    current_vm_name: VmName,
    provider: Arc<dyn AclPolicyProvider>,
    registry: Arc<dyn ServiceUnitRegistry>,
}

impl AuthzViaPermissions {
    /// Create new AuthzViaPermissions instance.
    pub fn new(
        current_vm_name: VmName,
        provider: Arc<dyn AclPolicyProvider>,
        registry: Arc<dyn ServiceUnitRegistry>,
    ) -> Self {
        Self { current_vm_name, provider, registry }
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

        let subject_vm_policy = match self.get_vm_authz_policy(subject_fqin.vm_name()) {
            Ok(policy) => policy,
            Err(e) => {
                return AuthzResult::ImplicitlyDenied(format!(
                    "VM Authz policy not found for subject from VM '{}': {}",
                    subject_fqin.vm_name(),
                    e
                ));
            }
        };

        match &object_sud.appMetadata.typedMetadata {
            TypedMetadata::ServerUnit(_) => self.check_call_permission(
                &subject_policy,
                subject_vm_policy,
                subject_fqin,
                &object_unit_type,
                &object_service_unit_name,
            ),

            TypedMetadata::PublisherUnit(publisher_metadata) => self.check_subscribe_permissions(
                &subject_policy,
                subject_vm_policy,
                subject_fqin,
                &object_unit_type,
                publisher_metadata,
            ),
        }
    }

    /// Returns Subject FQIN authorization result for the Object Unit Type.
    pub fn check_authorization_for_unit_type(
        &self,
        subject_fqin: &ServiceFqin,
        object_package_name: &PackageName,
        object_unit_type_name: &UnitTypeName,
        app_metadata: &ApplicationMetadata,
        service_unit_name: &ServiceUnitName,
    ) -> AuthzResult {
        let object_unit_type = match UnitType::builder()
            .package_name(object_package_name.to_string())
            .unit_type_name(object_unit_type_name.to_string())
            .build()
        {
            Ok(ut) => ut,
            Err(_) => return AuthzResult::ImplicitlyDenied("Invalid UnitType data".to_string()),
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

        let subject_vm_policy = match self.get_vm_authz_policy(subject_fqin.vm_name()) {
            Ok(policy) => policy,
            Err(e) => {
                return AuthzResult::ImplicitlyDenied(format!(
                    "VM Authz policy not found for subject from VM '{}': {}",
                    subject_fqin.vm_name(),
                    e
                ));
            }
        };

        match &app_metadata.typedMetadata {
            TypedMetadata::ServerUnit(_) => self.check_serve_permission(
                &subject_policy,
                subject_vm_policy,
                subject_fqin,
                &object_unit_type,
                service_unit_name,
            ),

            TypedMetadata::PublisherUnit(publisher_metadata) => self.check_publish_permissions(
                &subject_policy,
                subject_vm_policy,
                subject_fqin,
                &object_unit_type,
                publisher_metadata,
            ),
        }
    }

    /// Returns VM Authz policy to enforce permissions for the given VM name.
    ///
    /// If the VM name is the same as the current VM name, returns None,
    /// indicating that no VM-level permissions have to be enforced.
    fn get_vm_authz_policy(
        &self,
        vm_name: &VmName,
    ) -> Result<Option<Arc<VmAuthzPolicyModel>>, anyhow::Error> {
        if vm_name == &self.current_vm_name {
            return Ok(None);
        }

        self.provider.get_vm_authz_policy(vm_name).map(Some)
    }

    fn check_call_permission(
        &self,
        subject_policy: &AclPolicyModel,
        subject_vm_policy: Option<Arc<VmAuthzPolicyModel>>,
        subject_fqin: &ServiceFqin,
        object_unit_type: &UnitType,
        object_service_unit_name: &ServiceUnitName,
    ) -> AuthzResult {
        if subject_policy.can_call(object_unit_type, object_service_unit_name) {
            if subject_vm_policy
                .is_none_or(|policy| policy.can_call(object_unit_type, object_service_unit_name))
            {
                AuthzResult::Permitted(format!(
                    "Permitted by 'client' permission on service '{}' with channel '{}'",
                    object_unit_type, object_service_unit_name
                ))
            } else {
                AuthzResult::ExplicitlyDenied(format!(
                    "Subject's VM '{}' lacks 'client' permission for service '{}' with channel '{}'. \
                    Add 'client {{ service: \"{}\" channel: \"{}\" }}' to the VM's Authz policy",
                    subject_fqin.vm_name(),
                    object_unit_type,
                    object_service_unit_name,
                    object_unit_type,
                    object_service_unit_name
                ))
            }
        } else {
            AuthzResult::ExplicitlyDenied(format!(
                "Subject '{}' lacks 'client' permission for service '{}' with channel '{}'. \
                Add 'client {{ service: \"{}\" channel: \"{}\" }}' to the subject's Authz policy",
                subject_fqin,
                object_unit_type,
                object_service_unit_name,
                object_unit_type,
                object_service_unit_name
            ))
        }
    }

    fn check_serve_permission(
        &self,
        subject_policy: &AclPolicyModel,
        subject_vm_policy: Option<Arc<VmAuthzPolicyModel>>,
        subject_fqin: &ServiceFqin,
        object_unit_type: &UnitType,
        service_unit_name: &ServiceUnitName,
    ) -> AuthzResult {
        if subject_policy.can_serve(object_unit_type, service_unit_name) {
            if subject_vm_policy
                .is_none_or(|policy| policy.can_serve(object_unit_type, service_unit_name))
            {
                AuthzResult::Permitted(format!(
                    "Permitted by 'server' permission on service '{}' with channel '{}'",
                    object_unit_type, service_unit_name
                ))
            } else {
                AuthzResult::ExplicitlyDenied(format!(
                    "Subject's VM '{}' lacks 'server' permission on service '{}' with channel '{}'. \
                    Add 'server {{ service: \"{}\" channel: \"{}\" }}' to the VM's Authz policy",
                    subject_fqin.vm_name(),
                    object_unit_type,
                    service_unit_name,
                    object_unit_type,
                    service_unit_name
                ))
            }
        } else {
            AuthzResult::ExplicitlyDenied(format!(
                "Subject '{}' lacks 'server' permission on service '{}' with channel '{}'. \
                Add 'server {{ service: \"{}\" channel: \"{}\" }}' to the subject's Authz policy",
                subject_fqin,
                object_unit_type,
                service_unit_name,
                object_unit_type,
                service_unit_name
            ))
        }
    }

    fn check_publish_permissions(
        &self,
        subject_policy: &AclPolicyModel,
        subject_vm_policy: Option<Arc<VmAuthzPolicyModel>>,
        subject_fqin: &ServiceFqin,
        object_unit_type: &UnitType,
        publisher_metadata: &PublisherUnitMetadata,
    ) -> AuthzResult {
        let topic_name = match TopicName::from_str(&publisher_metadata.topic_name) {
            Ok(tn) => tn,
            Err(e) => {
                return AuthzResult::ImplicitlyDenied(format!(
                    "Invalid TopicName '{}' in publisher metadata: {:?}",
                    publisher_metadata.topic_name, e
                ));
            }
        };

        if subject_policy.can_publish(object_unit_type, &topic_name) {
            if subject_vm_policy
                .is_none_or(|policy| policy.can_publish(object_unit_type, &topic_name))
            {
                AuthzResult::Permitted(format!(
                    "Permitted by 'publisher' permission on message '{}' for topic '{}'",
                    object_unit_type, topic_name
                ))
            } else {
                AuthzResult::ExplicitlyDenied(format!(
                    "Subject's VM '{}' lacks 'publisher' permission on message '{}' for topic '{}'. \
                    Add 'publisher {{ message: \"{}\" topic: \"{}\" }}' to the VM's Authz policy",
                    subject_fqin.vm_name(),
                    object_unit_type,
                    topic_name,
                    object_unit_type,
                    topic_name
                ))
            }
        } else {
            AuthzResult::ExplicitlyDenied(format!(
                "Subject '{}' lacks 'publisher' permission on message '{}' for topic '{}'. \
                Add 'publisher {{ message: \"{}\" topic: \"{}\" }}' to the subject's Authz policy",
                subject_fqin, object_unit_type, topic_name, object_unit_type, topic_name
            ))
        }
    }

    fn check_subscribe_permissions(
        &self,
        subject_policy: &AclPolicyModel,
        subject_vm_policy: Option<Arc<VmAuthzPolicyModel>>,
        subject_fqin: &ServiceFqin,
        object_unit_type: &UnitType,
        publisher_metadata: &PublisherUnitMetadata,
    ) -> AuthzResult {
        let topic_name = match TopicName::from_str(&publisher_metadata.topic_name) {
            Ok(tn) => tn,
            Err(e) => return AuthzResult::ImplicitlyDenied(format!("Invalid Topic: {:?}", e)),
        };

        if subject_policy.can_subscribe(object_unit_type, &topic_name) {
            if subject_vm_policy
                .is_none_or(|policy| policy.can_subscribe(object_unit_type, &topic_name))
            {
                AuthzResult::Permitted(format!(
                    "Permitted by 'subscriber' permission on message '{}' with topic '{}'",
                    object_unit_type, topic_name
                ))
            } else {
                AuthzResult::ExplicitlyDenied(format!(
                    "Subject's VM '{}' lacks 'subscriber' permission on message '{}' with topic '{}'. \
                    Add 'subscriber {{ message: \"{}\" topic: \"{}\" }}' to the VM's Authz policy",
                    subject_fqin.vm_name(),
                    object_unit_type,
                    topic_name,
                    object_unit_type,
                    topic_name
                ))
            }
        } else {
            AuthzResult::ExplicitlyDenied(format!(
                "Subject '{}' lacks 'subscriber' permission on message '{}' with topic '{}'. \
                Add 'subscriber {{ message: \"{}\" topic: \"{}\" }}' to the subject's Authz policy",
                subject_fqin, object_unit_type, topic_name, object_unit_type, topic_name
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AclPolicyModel;
    use crate::TestAclPolicyProvider;
    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::UnitType::UnitType as AidlUnitType;
    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::
    {PublisherUnitMetadata::PublisherUnitMetadata, ServerUnitMetadata::ServerUnitMetadata};
    use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::ServiceUnitDefinition;
    use protobuf::text_format;
    use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
    use sdv_authz_policy_proto_rust::vm_authz_policy::VmAuthzPolicy as VmAuthzPolicyProto;
    use sdv_comms_id::names::{ServiceName as ServiceBundleName, VmName};
    use std::str::FromStr;
    use crate::VmAuthzPolicyModel;

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
        // 1. Setup Policy: Allow calls to 'TargetType' on specific unit 'correct-unit'
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("local-vm").unwrap(),
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

        let service = AuthzViaPermissions::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(test_provider),
            registry,
        );

        // 4. Perform Check
        let result = service.check_authorization_for_service_unit(
            &ServiceFqin::from_str("local-vm:com.client.ClientBundle/service").unwrap(),
            &ServiceFqin::from_str("local-vm:com.server.ServerBundle/service").unwrap(),
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
        let service = AuthzViaPermissions::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(test_provider),
            registry,
        );

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
            VmName::from_str("local-vm").unwrap(),
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
        let service = AuthzViaPermissions::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(TestAclPolicyProvider::new()),
            registry,
        );

        let result = service.check_authorization_for_sud(
            &ServiceFqin::from_str("local-vm:com.client.App/s").unwrap(),
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
        // 1. Setup Policy that allows "OtherType" but NOT "TargetType"
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("local-vm").unwrap(),
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
        let service = AuthzViaPermissions::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(test_provider),
            registry,
        );

        // 2. Perform Check - should fail because TargetType != OtherType
        let result = service.check_authorization_for_service_unit(
            &ServiceFqin::from_str("local-vm:com.client.ClientBundle/service").unwrap(),
            &ServiceFqin::from_str("local-vm:com.server.ServerBundle/service").unwrap(),
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

    #[test]
    fn test_check_authorization_unit_type_server_success() {
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("local-vm").unwrap(),
            PackageName::from_str("com.subject").unwrap(),
            Some(ServiceBundleName::from_str("SubjectBundle").unwrap()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"server { service: "com.obj.Type" channel: "my-unit" }"#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        let service = AuthzViaPermissions::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(test_provider),
            Arc::new(MockRegistry { definition: Default::default() }),
        );

        let app_metadata = ApplicationMetadata {
            typedMetadata: TypedMetadata::ServerUnit(ServerUnitMetadata::default()),
            ..Default::default()
        };

        let result = service.check_authorization_for_unit_type(
            &ServiceFqin::from_str("local-vm:com.subject.SubjectBundle/s").unwrap(),
            &PackageName::from_str("com.obj").unwrap(),
            &UnitTypeName::from_str("Type").unwrap(),
            &app_metadata,
            &ServiceUnitName::from_str("my-unit").unwrap(),
        );

        assert!(matches!(result, AuthzResult::Permitted(_)));
    }

    #[test]
    fn test_check_authorization_unit_type_publisher_success() {
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("local-vm").unwrap(),
            PackageName::from_str("com.pub").unwrap(),
            Some(ServiceBundleName::from_str("PubBundle").unwrap()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"publisher { message: "com.obj.Type" topic: "telemetry" }"#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        let service = AuthzViaPermissions::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(test_provider),
            Arc::new(MockRegistry { definition: Default::default() }),
        );

        let app_metadata = ApplicationMetadata {
            typedMetadata: TypedMetadata::PublisherUnit(PublisherUnitMetadata {
                topic_name: "telemetry".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = service.check_authorization_for_unit_type(
            &ServiceFqin::from_str("local-vm:com.pub.PubBundle/s").unwrap(),
            &PackageName::from_str("com.obj").unwrap(),
            &UnitTypeName::from_str("Type").unwrap(),
            &app_metadata,
            &ServiceUnitName::from_str("any-unit").unwrap(),
        );

        if let AuthzResult::Permitted(msg) = result {
            assert!(msg.contains("Permitted by 'publisher'"));
        } else {
            panic!("Expected Permitted for publisher, got {:?}", result);
        }
    }

    #[test]
    fn test_check_authorization_unit_type_server_denied_by_vm_permissions() {
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("vm").unwrap(),
            PackageName::from_str("com.subject").unwrap(),
            Some(ServiceBundleName::from_str("SubjectBundle").unwrap()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"server { service: "com.obj.Type" channel: "my-unit" }"#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        test_provider.set_vm_authz_policy(
            VmName::from_str("vm").unwrap(),
            VmAuthzPolicyModel::try_from(
                text_format::parse_from_str::<VmAuthzPolicyProto>(
                    r#"
                        allow_server { service: "com.obj.Type" channel: "*"}
                        deny_server { service: "com.obj.Type" channel: "my-unit" }
                    "#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        let service = AuthzViaPermissions::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(test_provider),
            Arc::new(MockRegistry { definition: Default::default() }),
        );

        let app_metadata = ApplicationMetadata {
            typedMetadata: TypedMetadata::ServerUnit(ServerUnitMetadata::default()),
            ..Default::default()
        };

        let result = service.check_authorization_for_unit_type(
            &ServiceFqin::from_str("vm:com.subject.SubjectBundle/s").unwrap(),
            &PackageName::from_str("com.obj").unwrap(),
            &UnitTypeName::from_str("Type").unwrap(),
            &app_metadata,
            &ServiceUnitName::from_str("my-unit").unwrap(),
        );

        assert!(matches!(result, AuthzResult::ExplicitlyDenied(_)));
    }

    #[test]
    fn test_check_authorization_unit_type_publisher_denied_by_vm_permissions() {
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("vm").unwrap(),
            PackageName::from_str("com.pub").unwrap(),
            Some(ServiceBundleName::from_str("PubBundle").unwrap()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"publisher { message: "com.obj.Type" topic: "telemetry" }"#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        test_provider.set_vm_authz_policy(
            VmName::from_str("vm").unwrap(),
            VmAuthzPolicyModel::try_from(
                text_format::parse_from_str::<VmAuthzPolicyProto>(
                    r#"
                        allow_publisher { message: "com.obj.Type" topic: "*" }
                        deny_publisher { message: "com.obj.Type" topic: "telemetry" }
                    "#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        let service = AuthzViaPermissions::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(test_provider),
            Arc::new(MockRegistry { definition: Default::default() }),
        );

        let app_metadata = ApplicationMetadata {
            typedMetadata: TypedMetadata::PublisherUnit(PublisherUnitMetadata {
                topic_name: "telemetry".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = service.check_authorization_for_unit_type(
            &ServiceFqin::from_str("vm:com.pub.PubBundle/s").unwrap(),
            &PackageName::from_str("com.obj").unwrap(),
            &UnitTypeName::from_str("Type").unwrap(),
            &app_metadata,
            &ServiceUnitName::from_str("any-unit").unwrap(),
        );

        assert!(matches!(result, AuthzResult::ExplicitlyDenied(_)));
    }

    #[test]
    fn test_check_authorization_unit_type_server_denied_wrong_unit() {
        let mut test_provider = TestAclPolicyProvider::new();
        // Policy allows 'server' on 'correct-unit'
        test_provider.set_authz_policy(
            VmName::from_str("local-vm").unwrap(),
            PackageName::from_str("com.subject").unwrap(),
            Some(ServiceBundleName::from_str("SubjectBundle").unwrap()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"server { service: "com.obj.Type" channel: "correct-unit" }"#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        let service = AuthzViaPermissions::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(test_provider),
            Arc::new(MockRegistry { definition: Default::default() }),
        );

        let app_metadata = ApplicationMetadata {
            typedMetadata: TypedMetadata::ServerUnit(ServerUnitMetadata::default()),
            ..Default::default()
        };

        // Subject attempts to serve on 'wrong-unit'
        let result = service.check_authorization_for_unit_type(
            &ServiceFqin::from_str("local-vm:com.subject.SubjectBundle/s").unwrap(),
            &PackageName::from_str("com.obj").unwrap(),
            &UnitTypeName::from_str("Type").unwrap(),
            &app_metadata,
            &ServiceUnitName::from_str("wrong-unit").unwrap(),
        );

        match result {
            AuthzResult::ExplicitlyDenied(msg) => {
                assert!(msg.contains("lacks 'server' permission"));
                assert!(msg.contains("wrong-unit"));
            }
            _ => panic!("Expected ExplicitlyDenied for wrong server unit, got {:?}", result),
        }
    }

    #[test]
    fn test_check_authorization_unit_type_publisher_denied_wrong_topic() {
        let mut test_provider = TestAclPolicyProvider::new();
        // Policy allows publishing to 'telemetry'
        test_provider.set_authz_policy(
            VmName::from_str("local-vm").unwrap(),
            PackageName::from_str("com.pub").unwrap(),
            Some(ServiceBundleName::from_str("PubBundle").unwrap()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"publisher { message: "com.obj.Type" topic: "telemetry" }"#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        let service = AuthzViaPermissions::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(test_provider),
            Arc::new(MockRegistry { definition: Default::default() }),
        );

        // Subject attempts to publish to 'secret-topic'
        let app_metadata = ApplicationMetadata {
            typedMetadata: TypedMetadata::PublisherUnit(PublisherUnitMetadata {
                topic_name: "secret-topic".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = service.check_authorization_for_unit_type(
            &ServiceFqin::from_str("local-vm:com.pub.PubBundle/s").unwrap(),
            &PackageName::from_str("com.obj").unwrap(),
            &UnitTypeName::from_str("Type").unwrap(),
            &app_metadata,
            &ServiceUnitName::from_str("any-unit").unwrap(),
        );

        match result {
            AuthzResult::ExplicitlyDenied(msg) => {
                assert!(msg.contains("lacks 'publisher' permission"));
                assert!(msg.contains("secret-topic"));
            }
            _ => panic!("Expected ExplicitlyDenied for wrong topic, got {:?}", result),
        }
    }

    #[test]
    fn test_check_authorization_unit_type_server_denied_wrong_type() {
        let mut test_provider = TestAclPolicyProvider::new();
        // Policy allows serving 'com.obj.AllowedType'
        test_provider.set_authz_policy(
            VmName::from_str("local-vm").unwrap(),
            PackageName::from_str("com.subject").unwrap(),
            Some(ServiceBundleName::from_str("SubjectBundle").unwrap()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"server { service: "com.obj.AllowedType" channel: "my-unit" }"#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        let service = AuthzViaPermissions::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(test_provider),
            Arc::new(MockRegistry { definition: Default::default() }),
        );

        let app_metadata = ApplicationMetadata {
            typedMetadata: TypedMetadata::ServerUnit(ServerUnitMetadata::default()),
            ..Default::default()
        };

        // Subject attempts to serve 'com.obj.ForbiddenType' on the correct unit name
        let result = service.check_authorization_for_unit_type(
            &ServiceFqin::from_str("local-vm:com.subject.SubjectBundle/s").unwrap(),
            &PackageName::from_str("com.obj").unwrap(),
            &UnitTypeName::from_str("ForbiddenType").unwrap(),
            &app_metadata,
            &ServiceUnitName::from_str("my-unit").unwrap(),
        );

        match result {
            AuthzResult::ExplicitlyDenied(msg) => {
                assert!(msg.contains("lacks 'server' permission"));
                assert!(msg.contains("ForbiddenType"));
            }
            _ => panic!("Expected ExplicitlyDenied for wrong unit type, got {:?}", result),
        }
    }

    #[test]
    fn test_check_authorization_subscribe_success() {
        // 1. Setup Policy: Allow subject to subscribe to 'com.sensor.Temp' on topic 'ambient'
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("local-vm").unwrap(),
            PackageName::from_str("com.subscriber").unwrap(),
            Some(ServiceBundleName::from_str("SubBundle").unwrap()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"
                        subscriber {
                            message: "com.sensor.Temp"
                            topic: "ambient"
                        }
                    "#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        // 2. Setup Mock Registry with a Publisher SUD
        let publisher_definition = ServiceUnitDefinition {
            unitType: AidlUnitType {
                sdvPackageName: "com.sensor".to_string(),
                serviceBundleName: "SensorBundle".to_string(),
                typeName: "Temp".to_string(),
            },
            serviceUnitName: "temp-sensor-unit".to_string(),
            appMetadata: ApplicationMetadata {
                typedMetadata: TypedMetadata::PublisherUnit(PublisherUnitMetadata {
                    topic_name: "ambient".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let registry = Arc::new(MockRegistry { definition: publisher_definition });

        let service = AuthzViaPermissions::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(test_provider),
            registry,
        );

        // 3. Perform Check
        let result = service.check_authorization_for_service_unit(
            &ServiceFqin::from_str("local-vm:com.subscriber.SubBundle/sub").unwrap(),
            &ServiceFqin::from_str("local-vm:com.sensor.SensorBundle/pub").unwrap(),
            &ServiceUnitName::from_str("temp-sensor-unit").unwrap(),
        );

        // 4. Assert Success
        match result {
            AuthzResult::Permitted(msg) => {
                assert!(msg.contains("Permitted by 'subscriber' permission"));
                assert!(msg.contains("ambient"));
            }
            _ => panic!("Expected Permitted, got {:?}", result),
        }
    }

    #[test]
    fn test_check_authorization_subscribe_denied_by_vm_permissions() {
        // 1a. Setup Policy: Allow subject to subscribe to 'com.sensor.Temp' on topic 'ambient'
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("vm").unwrap(),
            PackageName::from_str("com.subscriber").unwrap(),
            Some(ServiceBundleName::from_str("SubBundle").unwrap()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"
                        subscriber {
                            message: "com.sensor.Temp"
                            topic: "ambient"
                        }
                    "#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        // 1b. Setup VM-level Policy: Allow subject to subscribe to 'com.sensor.Temp', but not on topic 'ambient'
        test_provider.set_vm_authz_policy(
            VmName::from_str("vm").unwrap(),
            VmAuthzPolicyModel::try_from(
                text_format::parse_from_str::<VmAuthzPolicyProto>(
                    r#"
                        allow_subscriber { message: "com.sensor.Temp" topic: "*" }
                        deny_subscriber { message: "com.sensor.Temp" topic: "ambient" }
                    "#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        // 2. Setup Mock Registry with a Publisher SUD
        let publisher_definition = ServiceUnitDefinition {
            unitType: AidlUnitType {
                sdvPackageName: "com.sensor".to_string(),
                serviceBundleName: "SensorBundle".to_string(),
                typeName: "Temp".to_string(),
            },
            serviceUnitName: "temp-sensor-unit".to_string(),
            appMetadata: ApplicationMetadata {
                typedMetadata: TypedMetadata::PublisherUnit(PublisherUnitMetadata {
                    topic_name: "ambient".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let registry = Arc::new(MockRegistry { definition: publisher_definition });

        let service = AuthzViaPermissions::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(test_provider),
            registry,
        );

        // 3. Perform Check
        let result = service.check_authorization_for_service_unit(
            &ServiceFqin::from_str("vm:com.subscriber.SubBundle/sub").unwrap(),
            &ServiceFqin::from_str("vm:com.sensor.SensorBundle/pub").unwrap(),
            &ServiceUnitName::from_str("temp-sensor-unit").unwrap(),
        );

        // 4. Assert Success
        assert!(matches!(result, AuthzResult::ExplicitlyDenied(_)));
    }

    #[test]
    fn test_check_authorization_subscribe_denied_wrong_topic() {
        // 1. Setup Policy: Allow subscribing to 'ambient', but object is 'internal'
        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_authz_policy(
            VmName::from_str("local-vm").unwrap(),
            PackageName::from_str("com.subscriber").unwrap(),
            Some(ServiceBundleName::from_str("SubBundle").unwrap()),
            AclPolicyModel::try_from(
                text_format::parse_from_str::<AclPolicyProto>(
                    r#"
                        subscriber {
                            message: "com.sensor.Temp"
                            topic: "ambient"
                        }
                    "#,
                )
                .unwrap(),
            )
            .unwrap(),
        );

        let publisher_definition = ServiceUnitDefinition {
            unitType: AidlUnitType {
                sdvPackageName: "com.sensor".to_string(),
                serviceBundleName: "SensorBundle".to_string(),
                typeName: "Temp".to_string(),
            },
            serviceUnitName: "temp-sensor-unit".to_string(),
            appMetadata: ApplicationMetadata {
                typedMetadata: TypedMetadata::PublisherUnit(PublisherUnitMetadata {
                    topic_name: "internal-secret-topic".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let registry = Arc::new(MockRegistry { definition: publisher_definition });
        let service = AuthzViaPermissions::new(
            VmName::from_str("local-vm").unwrap(),
            Arc::new(test_provider),
            registry,
        );

        // 2. Perform Check
        let result = service.check_authorization_for_service_unit(
            &ServiceFqin::from_str("local-vm:com.subscriber.SubBundle/sub").unwrap(),
            &ServiceFqin::from_str("local-vm:com.sensor.SensorBundle/pub").unwrap(),
            &ServiceUnitName::from_str("temp-sensor-unit").unwrap(),
        );

        // 3. Assert Denial
        match result {
            AuthzResult::ExplicitlyDenied(msg) => {
                assert!(msg.contains("lacks 'subscriber' permission"));
                assert!(msg.contains("internal-secret-topic"));
            }
            _ => panic!("Expected ExplicitlyDenied, got {:?}", result),
        }
    }
}
