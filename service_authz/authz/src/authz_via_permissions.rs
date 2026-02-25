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

use crate::{AclPolicyProvider, AuthzResult};
use anyhow::{anyhow, Context};
use sdv_comms::sd::UnitType;
use sdv_comms_id::{
    names::{PackageName, ServiceUnitName, UnitTypeName},
    ServiceFqin,
};
use std::sync::Arc;

use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent::{
    BpServiceDiscoveryAgent, IServiceDiscoveryAgent
};
use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinAidl;

/// Returns a Binder proxy for Service Discovery interface.
fn get_service_discovery_agent() -> anyhow::Result<binder::Strong<dyn IServiceDiscoveryAgent>> {
    let descriptor = <BpServiceDiscoveryAgent as IServiceDiscoveryAgent>::get_descriptor()
        .to_owned()
        + "/default";
    binder::get_interface(&descriptor).context("Failed to get ServiceDiscoveryAgent")
}

type ServiceDiscoveryAgentProvider =
    Box<dyn Fn() -> anyhow::Result<binder::Strong<dyn IServiceDiscoveryAgent>> + Send + Sync>;

/// AuthzViaPermissions is an entry point for permission-based authorization checks.
pub struct AuthzViaPermissions {
    provider: Arc<dyn AclPolicyProvider>,
    sd_agent_provider: ServiceDiscoveryAgentProvider,
}

impl AuthzViaPermissions {
    /// Create new AuthzViaPermissions instance.
    pub fn new(provider: Arc<dyn AclPolicyProvider>) -> Self {
        Self { provider, sd_agent_provider: Box::new(get_service_discovery_agent) }
    }

    /// Internal constructor for dependency injection.
    #[cfg(test)]
    fn new_with_sd_agent(
        provider: Arc<dyn AclPolicyProvider>,
        sd_agent: binder::Strong<dyn IServiceDiscoveryAgent>,
    ) -> Self {
        Self { provider, sd_agent_provider: Box::new(move || Ok(sd_agent.clone())) }
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

        let object_unit_type = match self.get_unit_type(object_fqin, object_service_unit_name) {
            Ok(unit_type) => unit_type,
            Err(e) => {
                return AuthzResult::ImplicitlyDenied(format!("Failed to get unit type: {}", e))
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

    fn get_unit_type(
        &self,
        fqin: &ServiceFqin,
        unit_name: &ServiceUnitName,
    ) -> anyhow::Result<UnitType> {
        let sd_agent = (self.sd_agent_provider)()?;

        let fqin_aidl = ServiceFqinAidl::from(fqin.clone());
        let unit_definition = sd_agent
            .getServiceUnit(&fqin_aidl, &unit_name.to_string())
            .context("Failed to get service unit")?
            .ok_or(anyhow!("No service unit with fqin {}", fqin))?;

        let unit_type = unit_definition.unitType;

        UnitType::builder()
            .set_sdv_package_name(unit_type.sdvPackageName)
            .set_service_bundle_name(unit_type.serviceBundleName)
            .set_unit_type_name(unit_type.typeName)
            .build()
            .context("SD returned an invalid unit type")
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
    use binder::BinderFeatures;
    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::UnitType::UnitType as AidlUnitType;
    use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::ServiceUnitDefinition;
    use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent::MockIServiceDiscoveryAgent;
    use protobuf::text_format;
    use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
    use sdv_comms_id::names::{ServiceName as ServiceBundleName, VmName};
    use std::str::FromStr;

    use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent::BnServiceDiscoveryAgent;

    fn setup_mock_sd_agent(
        response: binder::Result<Option<ServiceUnitDefinition>>,
    ) -> binder::Strong<dyn IServiceDiscoveryAgent> {
        let mut mock = MockIServiceDiscoveryAgent::new();
        mock.expect_getServiceUnit().return_once(move |_fqin, _unit_name| response);

        BnServiceDiscoveryAgent::new_binder(mock, BinderFeatures::default())
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

        // 2. Setup Mock SD: Return a Unit Type that matches the policy's 'service'
        let fake_unit_type = AidlUnitType {
            sdvPackageName: "com.server".to_string(),
            serviceBundleName: "ServerBundle".to_string(),
            typeName: "TargetType".to_string(),
        };
        let fake_definition =
            ServiceUnitDefinition { unitType: fake_unit_type, ..Default::default() };
        let mock_agent = setup_mock_sd_agent(Ok(Some(fake_definition)));

        // 3. Initialize Service
        let service = AuthzViaPermissions::new_with_sd_agent(Arc::new(test_provider), mock_agent);

        // 4. Perform Check
        let result = service.check_authorization_for_service_unit(
            &ServiceFqin::from_str("vm:com.client.ClientBundle/service").unwrap(),
            &ServiceFqin::from_str("vm:com.server.ServerBundle/service").unwrap(),
            &ServiceUnitName::from_str("correct-unit").unwrap(), // Matches policy
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

        let service = AuthzViaPermissions::new(Arc::new(test_provider));

        let result = service.check_authorization_for_service_unit(
            &ServiceFqin::from_str("vm:com.unknown.App/service").unwrap(),
            &ServiceFqin::from_str("vm:com.d.E/f").unwrap(),
            &ServiceUnitName::from_str("unit").unwrap(),
        );

        match result {
            AuthzResult::ImplicitlyDenied(msg) => {
                assert!(msg.contains("ACL policy not found"), "Unexpected msg: {}", msg);
            }
            _ => panic!("Expected ImplicitlyDenied (No Policy), got {:?}", result),
        }
    }
}
