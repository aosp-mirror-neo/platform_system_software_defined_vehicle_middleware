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
use crate::kv::ServiceRegistry;
use async_trait::async_trait;
use binder::{Interface, Result as BinderResult, Status};
use std::sync::Arc;
use tracing::instrument;

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
    metadata::TransportMetadata::TransportMetadata,
    RegistrationToken::RegistrationToken as RegistrationTokenV2,
};
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::{
    ITransportSupportAgent::ITransportSupportAgentAsyncServer,
    ServiceUnitDefinition::ServiceUnitDefinition,
};

// Implementation of the ITransportSupportAgent AIDL Interface.
impl Interface for TransportSupportAgentService {}
pub(crate) struct TransportSupportAgentService {
    service_registry: Arc<ServiceRegistry>,
}

impl TransportSupportAgentService {
    // This is used only in tests so far.
    pub fn new(service_registry: Arc<ServiceRegistry>) -> Self {
        TransportSupportAgentService { service_registry }
    }
}

#[async_trait]
impl ITransportSupportAgentAsyncServer for TransportSupportAgentService {
    #[instrument(name = "sd::ITransportSupportAgent::getServiceUnitForToken", skip_all)]
    async fn getServiceUnitForToken(
        &self,
        token: &RegistrationTokenV2,
    ) -> BinderResult<ServiceUnitDefinition> {
        if token.token.is_none() {
            return Err(Status::new_service_specific_error_str(
                -1,
                Some("Failed to get service unit: token is not set"),
            ));
        }

        self.service_registry.get_service_unit_for_token(&token.token.clone().unwrap()).ok_or(
            Status::new_service_specific_error_str(
                -1,
                Some("Failed to get service unit: not found"),
            ),
        )
    }

    #[instrument(name = "sd::ITransportSupportAgent::setAgentMetadata", skip_all)]
    async fn setAgentMetadata(
        &self,
        token: &RegistrationTokenV2,
        transport_metadata: &TransportMetadata,
    ) -> Result<(), Status> {
        let token = token.token.as_ref().ok_or(Status::new_service_specific_error_str(
            -1,
            Some("Failed to set agent metadata: token is not set"),
        ))?;
        self.service_registry
            .finalize_service_unit_registration(token, transport_metadata.clone())
            .map_err(|err| {
                Status::new_service_specific_error_str(
                    -1,
                    Some(format!("Failed to set agent metadata: {err:?}")),
                )
            })
    }

    #[instrument(name = "sd::ITransportSupportAgent::getAgentMetadata", skip_all)]
    async fn getAgentMetadata(
        &self,
        id: i64,
        service_unit_name: &str,
        service_unit_definition: &mut ServiceUnitDefinition,
        transport_metadata: &mut TransportMetadata,
    ) -> BinderResult<()> {
        match self.service_registry.get_transport_metadata_for_service_unit(
            id,
            service_unit_name,
            service_unit_definition,
            transport_metadata,
        ) {
            true => Ok(()),
            false => Err(Status::new_service_specific_error_str(
                -1,
                Some("Failed to get metadata: not found"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::kv::ServiceRegistry;
    use crate::transport_agent::service::Status;
    use crate::transport_agent::service::TransportMetadata;
    use crate::transport_agent::service::TransportSupportAgentService;

    use std::default::Default;
    use std::sync::Arc;

    use google_sdv_identity::aidl::google::sdv::identity::{
        ProcessIdentifier::ProcessIdentifier, ServiceIdentity::ServiceIdentity,
    };
    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
        metadata::ApplicationMetadata::ApplicationMetadata, AccessControl::AccessControl,
        RegistrationToken::RegistrationToken as RegistrationTokenV2, UnitType::UnitType,
    };
    use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::{
        ITransportSupportAgent::ITransportSupportAgentAsyncServer,
        ServiceUnitDefinition::ServiceUnitDefinition,
    };

    fn create_transport_support_agent_for_test(
        service_registry: ServiceRegistry,
    ) -> TransportSupportAgentService {
        TransportSupportAgentService::new(Arc::new(service_registry))
    }

    #[tokio::test]
    async fn transport_agent_full_flow_test() {
        let service_registry = ServiceRegistry::default();
        let process_identifier = ProcessIdentifier { pid: 42, uid: 1001 };
        let result = service_registry.register_identity_v2(
            ServiceIdentity::default(),
            None,
            &process_identifier,
        );
        assert!(result.is_ok());

        let unit_type = UnitType {
            sdvPackageName: "sdv.package.name".to_string(),
            serviceBundleName: "".to_string(), // service bundle names as part of unit types are deprecated
            typeName: "default".to_string(),
        };

        let token = service_registry
            .preregister_service_unit(
                "service_unit_name",
                &unit_type,
                &AccessControl::default(),
                &ApplicationMetadata::default(),
                &process_identifier,
            )
            .unwrap();

        let agent = create_transport_support_agent_for_test(service_registry);
        let sud = agent.getServiceUnitForToken(&token).await.unwrap();
        assert_eq!(sud.unitType, unit_type);

        let transport_metadata = TransportMetadata { value_holder: "123".into() };
        let set_agent_metadata_result = agent.setAgentMetadata(&token, &transport_metadata).await;
        assert!(set_agent_metadata_result.is_ok());

        let mut transport_metadata = TransportMetadata::default();
        let mut service_unit_definition = ServiceUnitDefinition::default();
        agent
            .getAgentMetadata(
                sud.identity.sid,
                "service_unit_name",
                &mut service_unit_definition,
                &mut transport_metadata,
            )
            .await
            .unwrap();
        assert_eq!(String::from_utf8(transport_metadata.value_holder).unwrap(), "123");
    }

    #[tokio::test]
    async fn get_agent_metadata_error_test() {
        let agent = create_transport_support_agent_for_test(ServiceRegistry::default());
        let mut transport_metadata = TransportMetadata::default();
        let mut service_unit_definition = ServiceUnitDefinition::default();

        let result = agent
            .getAgentMetadata(0, "unit name", &mut service_unit_definition, &mut transport_metadata)
            .await;

        assert_eq!(
            result.unwrap_err(),
            Status::new_service_specific_error_str(-1, Some("Failed to get metadata: not found"))
        );
    }

    #[tokio::test]
    async fn get_service_unit_error_test() {
        let agent = create_transport_support_agent_for_test(ServiceRegistry::default());
        let result = agent.getServiceUnitForToken(&RegistrationTokenV2::default()).await;
        assert_eq!(
            result.unwrap_err(),
            Status::new_service_specific_error_str(
                -1,
                Some("Failed to get service unit: token is not set")
            )
        );
    }
}
