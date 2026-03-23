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

// Service Discovery Agent.
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ITopicFetchListener::ITopicFetchListener;
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent::IServiceDiscoveryAgentAsyncServer;
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceUnitChangeListener::IServiceUnitChangeListener;
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceUnitFetchListener::IServiceUnitFetchListener;

use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::ServiceUnitDefinition;
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::UnitNameParameters::UnitNameParameters;

use google_sdv_identity::aidl::google::sdv::identity::ProcessIdentifier::ProcessIdentifier;
use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as AidlServiceFqin;
use google_sdv_privileged_service_discovery::aidl::google::sdv::privileged::service_discovery::IPrivilegedServiceDiscoveryAgent::IPrivilegedServiceDiscoveryAgentAsyncServer;
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::UnitType::UnitType;
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::ApplicationMetadata::
{ApplicationMetadata, TypedMetadata::TypedMetadata};

use sdv_authz::AuthzService;
use sdv_comms_id::names::TopicName;
use sdv_comms_id::ServiceFqin;

use crate::authz::create_authz_filter_for_sud;
use crate::kv::process_identifier::get_calling_process_identifier;
use crate::listeners::ServiceUnitChangeListener;
use crate::unit_type_utils::deprecate_unit_type_owner;
use crate::ServiceRegistry;
use async_trait::async_trait;
use binder::{ExceptionCode, Status};
use log::{error, warn};
use std::ffi::CStr;
use std::io::{Error as IoError, Write};
use std::str::FromStr;
use std::sync::Arc;
use tracing::instrument;

pub(crate) struct ServiceDiscoveryAgentService {
    service_registry: Arc<ServiceRegistry>,
    authz_service: Arc<AuthzService>,
}

impl ServiceDiscoveryAgentService {
    pub(crate) fn new(
        service_registry: Arc<ServiceRegistry>,
        authz_service: Arc<AuthzService>,
    ) -> Self {
        ServiceDiscoveryAgentService { service_registry, authz_service }
    }

    // Returns Service FQIN of the caller.
    fn get_process_fqin(
        &self,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<ServiceFqin> {
        self.service_registry.get_process_fqin(process_identifier).map_err(|err| {
            let msg = format!("Failed to fetch valid caller FQIN: {err:?}");
            error!("{msg}");
            binder::Status::new_exception_str(binder::ExceptionCode::SECURITY, Some(msg))
        })
    }

    fn fetch_service_units_by_type(
        &self,
        unit_type: &UnitType,
        listener: &binder::Strong<dyn IServiceUnitFetchListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        let unit_type: UnitType = deprecate_unit_type_owner(unit_type.clone());

        let caller_fqin = self.get_process_fqin(process_identifier)?;
        let authz_filter = create_authz_filter_for_sud(caller_fqin, self.authz_service.clone());

        let units: Vec<ServiceUnitDefinition> = self
            .service_registry
            .list_service_units_by_unit_type(&unit_type)
            .into_iter()
            .filter(|sud| (*authz_filter)(sud))
            .collect();

        for item in units {
            if let Err(err) = listener.onServiceUnitFound(&[item]) {
                let err_msg = format!("Error sending Service Unit list: {err:?}");
                warn!("{err_msg}");
                return Err(binder::Status::new_exception_str(
                    binder::ExceptionCode::ILLEGAL_STATE,
                    Some(err_msg),
                ));
            }
        }

        Ok(())
    }

    fn fetch_service_units_by_name(
        &self,
        query: &UnitNameParameters,
        listener: &binder::Strong<dyn IServiceUnitFetchListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        let caller_fqin = self.get_process_fqin(process_identifier)?;
        let authz_filter = create_authz_filter_for_sud(caller_fqin, self.authz_service.clone());

        let units: Vec<ServiceUnitDefinition> = self
            .service_registry
            .list_service_units_by_name(query)
            .into_iter()
            .filter(|sud| (*authz_filter)(sud))
            .collect();

        for item in units {
            if let Err(err) = listener.onServiceUnitFound(&[item]) {
                let err_msg = format!("Error sending Service Unit list: {err:?}");
                warn!("{err_msg}");
                return Err(binder::Status::new_exception_str(
                    binder::ExceptionCode::ILLEGAL_STATE,
                    Some(err_msg),
                ));
            }
        }

        Ok(())
    }

    fn get_service_unit(
        &self,
        fqin: &AidlServiceFqin,
        service_unit_name: &str,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<Option<ServiceUnitDefinition>> {
        let caller_fqin = self.get_process_fqin(process_identifier)?;
        let sud_option = self.service_registry.get_service_unit(fqin, service_unit_name);

        let authz_filter = create_authz_filter_for_sud(caller_fqin, self.authz_service.clone());

        if let Some(ref sud) = sud_option {
            if !authz_filter(sud) {
                let msg = format!(
                    r#"Authz violation. Process is not authorized to get FQIN {fqin:?} service unit {service_unit_name}"#
                );
                error!("{msg}");
                return Err(Status::new_exception_str(ExceptionCode::SECURITY, Some(msg)));
            }
        }

        Ok(sud_option)
    }

    /// Creates listener, which checks authorization.
    fn create_authz_listener(
        &self,
        process_identifier: &ProcessIdentifier,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
    ) -> binder::Result<ServiceUnitChangeListener> {
        let caller_fqin = self.get_process_fqin(process_identifier)?;
        let authz_filter = create_authz_filter_for_sud(caller_fqin, self.authz_service.clone());

        Ok(ServiceUnitChangeListener::new(authz_filter, listener.clone()))
    }

    fn register_service_unit_change_listener_by_type(
        &self,
        unit_type: &UnitType,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        let unit_type: UnitType = deprecate_unit_type_owner(unit_type.clone());
        let authz_listener = self.create_authz_listener(process_identifier, listener)?;

        self.service_registry
            .register_service_unit_change_listener_by_type(&unit_type, authz_listener)
    }

    fn unregister_service_unit_change_listener_by_type(
        &self,
        unit_type: &UnitType,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        let unit_type: UnitType = deprecate_unit_type_owner(unit_type.clone());
        let authz_listener = self.create_authz_listener(process_identifier, listener)?;

        self.service_registry
            .unregister_service_unit_change_listener_by_type(&unit_type, authz_listener)
    }

    fn parse_topic_name(topic_name: &str) -> Result<TopicName, binder::Status> {
        TopicName::from_str(topic_name).map_err(|err| {
            let msg = format!("Invalid topic name {topic_name}: {err:?}");
            error!("{msg}");
            binder::Status::new_exception_str(binder::ExceptionCode::ILLEGAL_ARGUMENT, Some(msg))
        })
    }

    fn register_service_unit_change_listener_by_topic(
        &self,
        unit_type: &UnitType,
        topic_name: &str,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        let unit_type: UnitType = deprecate_unit_type_owner(unit_type.clone());
        let authz_listener = self.create_authz_listener(process_identifier, listener)?;
        let topic = Self::parse_topic_name(topic_name)?;

        self.service_registry.register_service_unit_change_listener_by_topic(
            &unit_type,
            &topic,
            authz_listener,
        )
    }

    fn unregister_service_unit_change_listener_by_topic(
        &self,
        unit_type: &UnitType,
        topic_name: &str,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        let unit_type: UnitType = deprecate_unit_type_owner(unit_type.clone());
        let authz_listener = self.create_authz_listener(process_identifier, listener)?;
        let topic = Self::parse_topic_name(topic_name)?;

        self.service_registry.unregister_service_unit_change_listener_by_topic(
            &unit_type,
            &topic,
            authz_listener,
        )
    }

    fn register_service_unit_change_listener_by_name(
        &self,
        unit_name_params: &UnitNameParameters,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        let authz_listener = self.create_authz_listener(process_identifier, listener)?;

        self.service_registry
            .register_service_unit_change_listener_by_name(unit_name_params, authz_listener)
    }

    fn unregister_service_unit_change_listener_by_name(
        &self,
        unit_name_params: &UnitNameParameters,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        let authz_listener = self.create_authz_listener(process_identifier, listener)?;

        self.service_registry
            .unregister_service_unit_change_listener_by_name(unit_name_params, authz_listener)
    }

    fn unregister_service_unit_change_listener(
        &self,
        _listener: &binder::Strong<dyn IServiceUnitChangeListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        let _caller_fqin = self.get_process_fqin(process_identifier)?;
        unimplemented!()
    }
}

/// Pretty-prints a ServiceFqin in a compact format.
/// TODO(b/374247340): replace this with ServiceFqin::to_string()
fn pretty_print_fqin(fqin: &AidlServiceFqin, writer: &mut dyn Write) -> Result<(), IoError> {
    write!(
        writer,
        "{}:{}.{}/{}",
        fqin.sdvVmName, fqin.sdvPackageName, fqin.serviceBundleName, fqin.serviceInstanceName
    )?;
    Ok(())
}

/// Pretty-prints a UnitType in a compact format.
/// TODO(b/374247340): replace this with ServiceFqin::to_string()
fn pretty_print_unit_type(unit_type: &UnitType, writer: &mut dyn Write) -> Result<(), IoError> {
    write!(writer, "{}.{}", unit_type.sdvPackageName, unit_type.typeName)?;
    Ok(())
}

fn pretty_print_application_metadata(
    metadata: &ApplicationMetadata,
    writer: &mut dyn Write,
) -> Result<(), IoError> {
    match &metadata.typedMetadata {
        TypedMetadata::ServerUnit(_) => {
            writeln!(writer, "Server")?;
        }
        TypedMetadata::PublisherUnit(pub_meta) => {
            writeln!(writer, "Publisher: topic={}", pub_meta.topic_name)?;
        }
    }
    Ok(())
}

fn pretty_print_vm_service_units(
    service_registry: &ServiceRegistry,
    vm_name: &str,
    writer: &mut dyn Write,
) -> Result<(), IoError> {
    let service_units = service_registry.get_all_service_units_by_vm_name(vm_name);

    writeln!(writer, "##########  VM: {vm_name}  ##########")?;
    for unit in service_units {
        let sud = unit.get_service_unit_definition();

        writeln!(writer, "Service Unit: {}", sud.serviceUnitName)?;

        write!(writer, "    FQIN: ")?;
        pretty_print_fqin(&sud.identity.name, writer)?;
        writeln!(writer)?;

        write!(writer, "    Unit Type: ")?;
        pretty_print_unit_type(&sud.unitType, writer)?;
        writeln!(writer)?;

        write!(writer, "    ")?;
        pretty_print_application_metadata(&sud.appMetadata, writer)?;
        writeln!(writer)?;
    }

    Ok(())
}

impl binder::Interface for ServiceDiscoveryAgentService {
    fn dump(&self, writer: &mut dyn Write, _args: &[&CStr]) -> Result<(), binder::StatusCode> {
        for vm_name in &self.service_registry.get_known_vm_names() {
            pretty_print_vm_service_units(&self.service_registry, vm_name, writer)
                .map_err(|_| binder::StatusCode::UNKNOWN_ERROR)?;
        }

        Ok(())
    }
}

#[async_trait]
impl IServiceDiscoveryAgentAsyncServer for ServiceDiscoveryAgentService {
    #[instrument(name = "sd::IServiceDiscoveryAgent::listServiceUnitsByType", skip_all)]
    async fn listServiceUnitsByType(
        &self,
        unit_type: &UnitType,
    ) -> binder::Result<Vec<ServiceUnitDefinition>> {
        let unit_type: UnitType = deprecate_unit_type_owner(unit_type.clone());

        let caller_fqin = self.get_process_fqin(&get_calling_process_identifier())?;
        let authz_filter = create_authz_filter_for_sud(caller_fqin, self.authz_service.clone());

        Ok(self
            .service_registry
            .list_service_units_by_unit_type(&unit_type)
            .into_iter()
            .filter(|sud| (*authz_filter)(sud))
            .collect())
    }

    #[instrument(name = "sd::IServiceDiscoveryAgent::listServiceUnitsByName", skip_all)]
    async fn listServiceUnitsByName(
        &self,
        query: &UnitNameParameters,
    ) -> binder::Result<Vec<ServiceUnitDefinition>> {
        let caller_fqin = self.get_process_fqin(&get_calling_process_identifier())?;
        let authz_filter = create_authz_filter_for_sud(caller_fqin, self.authz_service.clone());

        Ok(self
            .service_registry
            .list_service_units_by_name(query)
            .into_iter()
            .filter(|sud| (*authz_filter)(sud))
            .collect())
    }

    #[instrument(name = "sd::IServiceDiscoveryAgent::fetchServiceUnitsByType", skip_all)]
    async fn fetchServiceUnitsByType(
        &self,
        unit_type: &UnitType,
        listener: &binder::Strong<dyn IServiceUnitFetchListener>,
    ) -> binder::Result<()> {
        let unit_type: UnitType = deprecate_unit_type_owner(unit_type.clone());

        self.fetch_service_units_by_type(&unit_type, listener, &get_calling_process_identifier())
    }

    #[instrument(name = "sd::IServiceDiscoveryAgent::fetchServiceUnitsByName", skip_all)]
    async fn fetchServiceUnitsByName(
        &self,
        query: &UnitNameParameters,
        listener: &binder::Strong<dyn IServiceUnitFetchListener>,
    ) -> binder::Result<()> {
        self.fetch_service_units_by_name(query, listener, &get_calling_process_identifier())
    }

    #[instrument(name = "sd::IServiceDiscoveryAgent::getServiceUnit", skip_all)]
    async fn getServiceUnit(
        &self,
        fqin: &AidlServiceFqin,
        service_unit_name: &str,
    ) -> binder::Result<Option<ServiceUnitDefinition>> {
        self.get_service_unit(fqin, service_unit_name, &get_calling_process_identifier())
    }

    #[instrument(
        name = "sd::IServiceDiscoveryAgent::registerServiceUnitChangeListenerByType",
        skip_all
    )]
    async fn registerServiceUnitChangeListenerByType(
        &self,
        unit_type: &UnitType,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
    ) -> binder::Result<()> {
        let unit_type: UnitType = deprecate_unit_type_owner(unit_type.clone());

        self.register_service_unit_change_listener_by_type(
            &unit_type,
            listener,
            &get_calling_process_identifier(),
        )
    }

    #[instrument(
        name = "sd::IServiceDiscoveryAgent::unregisterServiceUnitChangeListenerByType",
        skip_all
    )]
    async fn unregisterServiceUnitChangeListenerByType(
        &self,
        unit_type: &UnitType,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
    ) -> binder::Result<()> {
        let unit_type: UnitType = deprecate_unit_type_owner(unit_type.clone());

        self.unregister_service_unit_change_listener_by_type(
            &unit_type,
            listener,
            &get_calling_process_identifier(),
        )
    }

    #[instrument(
        name = "sd::IServiceDiscoveryAgent::registerServiceUnitChangeListenerByName",
        skip_all
    )]
    async fn registerServiceUnitChangeListenerByName(
        &self,
        unit_name_params: &UnitNameParameters,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
    ) -> binder::Result<()> {
        self.register_service_unit_change_listener_by_name(
            unit_name_params,
            listener,
            &get_calling_process_identifier(),
        )
    }

    #[instrument(name = "sd::IServiceDiscoveryAgent::listServiceUnitsByName", skip_all)]
    async fn unregisterServiceUnitChangeListenerByName(
        &self,
        unit_name_params: &UnitNameParameters,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
    ) -> binder::Result<()> {
        self.unregister_service_unit_change_listener_by_name(
            unit_name_params,
            listener,
            &get_calling_process_identifier(),
        )
    }

    #[instrument(
        name = "sd::IServiceDiscoveryAgent::registerServiceUnitChangeListenerByTopic",
        skip_all
    )]
    async fn registerServiceUnitChangeListenerByTopic(
        &self,
        unit_type: &UnitType,
        topic_name: &str,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
    ) -> binder::Result<()> {
        self.register_service_unit_change_listener_by_topic(
            unit_type,
            topic_name,
            listener,
            &get_calling_process_identifier(),
        )
    }

    #[instrument(
        name = "sd::IServiceDiscoveryAgent::unregisterServiceUnitChangeListenerByTopic",
        skip_all
    )]
    async fn unregisterServiceUnitChangeListenerByTopic(
        &self,
        unit_type: &UnitType,
        topic_name: &str,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
    ) -> binder::Result<()> {
        self.unregister_service_unit_change_listener_by_topic(
            unit_type,
            topic_name,
            listener,
            &get_calling_process_identifier(),
        )
    }

    #[instrument(name = "sd::IServiceDiscoveryAgent::fetchPublishersByTopicName", skip_all)]
    async fn fetchPublishersByTopicName(
        &self,
        _topic_name: &str,
        _listener: &binder::Strong<dyn IServiceUnitFetchListener>,
    ) -> binder::Result<()> {
        unimplemented!()
    }

    #[instrument(name = "sd::IServiceDiscoveryAgent::listServiceUnitsByName", skip_all)]
    async fn unregisterServiceUnitChangeListener(
        &self,
        _listener: &binder::Strong<dyn IServiceUnitChangeListener>,
    ) -> binder::Result<()> {
        self.unregister_service_unit_change_listener(_listener, &get_calling_process_identifier())
    }

    #[instrument(name = "sd::IServiceDiscoveryAgent::listTopics", skip_all)]
    async fn listTopics(
        &self,
        _listener: &binder::Strong<dyn ITopicFetchListener>,
    ) -> binder::Result<()> {
        unimplemented!()
    }

    #[instrument(name = "sd::IServiceDiscoveryAgent::listTopicsByType", skip_all)]
    async fn listTopicsByType(
        &self,
        _unit_type: &UnitType,
        _listener: &binder::Strong<dyn ITopicFetchListener>,
    ) -> binder::Result<()> {
        unimplemented!()
    }
}

#[allow(non_snake_case)] // generated AIDL interface
#[async_trait]
impl IPrivilegedServiceDiscoveryAgentAsyncServer for ServiceDiscoveryAgentService {
    #[instrument(
        name = "sd::IPrivilegedServiceDiscoveryAgent::fetchServiceUnitsByTypeForProcess",
        skip_all
    )]
    async fn fetchServiceUnitsByTypeForProcess(
        &self,
        unit_type: &UnitType,
        listener: &binder::Strong<dyn IServiceUnitFetchListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        let unit_type: UnitType = deprecate_unit_type_owner(unit_type.clone());

        self.fetch_service_units_by_type(&unit_type, listener, process_identifier)
    }

    #[instrument(
        name = "sd::IPrivilegedServiceDiscoveryAgent::fetchServiceUnitsByNameForProcess",
        skip_all
    )]
    async fn fetchServiceUnitsByNameForProcess(
        &self,
        query: &UnitNameParameters,
        listener: &binder::Strong<dyn IServiceUnitFetchListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        self.fetch_service_units_by_name(query, listener, process_identifier)
    }

    #[instrument(name = "sd::IPrivilegedServiceDiscoveryAgent::getServiceUnitForProcess", skip_all)]
    async fn getServiceUnitForProcess(
        &self,
        fqin: &AidlServiceFqin,
        service_unit_name: &str,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<Option<ServiceUnitDefinition>> {
        self.get_service_unit(fqin, service_unit_name, process_identifier)
    }

    #[instrument(
        name = "sd::IPrivilegedServiceDiscoveryAgent::registerServiceUnitChangeListenerByTypeForProcess",
        skip_all
    )]
    async fn registerServiceUnitChangeListenerByTypeForProcess(
        &self,
        unit_type: &UnitType,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        let unit_type: UnitType = deprecate_unit_type_owner(unit_type.clone());

        self.register_service_unit_change_listener_by_type(&unit_type, listener, process_identifier)
    }

    #[instrument(
        name = "sd::IPrivilegedServiceDiscoveryAgent::unregisterServiceUnitChangeListenerByTypeForProcess",
        skip_all
    )]
    async fn unregisterServiceUnitChangeListenerByTypeForProcess(
        &self,
        unit_type: &UnitType,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        let unit_type: UnitType = deprecate_unit_type_owner(unit_type.clone());

        self.unregister_service_unit_change_listener_by_type(
            &unit_type,
            listener,
            process_identifier,
        )
    }

    #[instrument(
        name = "sd::IPrivilegedServiceDiscoveryAgent::registerServiceUnitChangeListenerByNameForProcess",
        skip_all
    )]
    async fn registerServiceUnitChangeListenerByNameForProcess(
        &self,
        unit_name_params: &UnitNameParameters,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        self.register_service_unit_change_listener_by_name(
            unit_name_params,
            listener,
            process_identifier,
        )
    }

    #[instrument(
        name = "sd::IPrivilegedServiceDiscoveryAgent::unregisterServiceUnitChangeListenerByNameForProcess",
        skip_all
    )]
    async fn unregisterServiceUnitChangeListenerByNameForProcess(
        &self,
        unit_name_params: &UnitNameParameters,
        listener: &binder::Strong<dyn IServiceUnitChangeListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        self.unregister_service_unit_change_listener_by_name(
            unit_name_params,
            listener,
            process_identifier,
        )
    }

    #[instrument(
        name = "sd::IPrivilegedServiceDiscoveryAgent::unregisterServiceUnitChangeListenerForProcess",
        skip_all
    )]
    async fn unregisterServiceUnitChangeListenerForProcess(
        &self,
        _listener: &binder::Strong<dyn IServiceUnitChangeListener>,
        process_identifier: &ProcessIdentifier,
    ) -> binder::Result<()> {
        self.unregister_service_unit_change_listener(_listener, process_identifier)
    }
}

#[cfg(test)]
mod test {
    use crate::{
        kv::ServiceRegistry, service_discovery_agent::service::ServiceUnitDefinition,
        ServiceDiscoveryAgentService,
    };
    use anyhow::anyhow;
    use binder::BinderFeatures;
    use google_sdv_identity::aidl::google::sdv::identity::{
        ProcessIdentifier::ProcessIdentifier,
        ServiceFqin::ServiceFqin as ServiceFqinV2,
        ServiceIdentity::{EcPublicKey::EcPublicKey, ServiceIdentity as ServiceIdentityV2},
    };
    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
        metadata::{
            ApplicationMetadata::ApplicationMetadata, TransportMetadata::TransportMetadata,
        },
        AccessControl::AccessControl,
        UnitType::UnitType,
    };
    use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::{
        IServiceUnitChangeListener::{BnServiceUnitChangeListener, IServiceUnitChangeListener},
        IServiceUnitFetchListener::{BnServiceUnitFetchListener, IServiceUnitFetchListener},
        UnitNameParameters::UnitNameParameters,
    };
    use protobuf::text_format;
    use sdv_authz::{
        AclPolicyModel, AclPolicyProvider, AuthzEnforcementFlag, AuthzService, AuthzStrategy,
        VmAuthzPolicyModel,
    };
    use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
    use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName, VmName};
    use std::{
        collections::{HashMap, HashSet},
        str::FromStr,
        sync::{Arc, Mutex},
    };

    const VM_NAME: &str = "vm";
    const CALLER_PACKAGE_NAME: &str = "caller.package";
    const TARGET_PACKAGE_NAME: &str = "target.package";
    const CALLER_SERVICE_BUNDLE_NAME: &str = "CallerServiceBundle";
    const TARGET_SERVICE_BUNDLE_NAME: &str = "TargetServiceBundle";
    const INVALID_TARGET_SERVICE_BUNDLE_NAME: &str = "invalid-target-service-bundle";
    const CALLER_SERVICE_INSTANCE_NAME: &str = "caller-instance";
    const TARGET_SERVICE_INSTANCE_NAME: &str = "target-instance";
    const ALLOWED_SERVICE_UNIT_NAME: &str = "allow-target-service-unit-name";
    const DISALLOWED_SERVICE_UNIT_NAME: &str = "disallow-target-service-unit-name";
    const INVALID_SERVICE_UNIT_NAME: &str = "InvalidTargetServiceUnitName";
    const CALLER_PID: ProcessIdentifier = ProcessIdentifier { pid: 111111, uid: 1000 };
    const TARGET_PID: ProcessIdentifier = ProcessIdentifier { pid: 222222, uid: 2000 };
    const INVALID_TARGET_PID: ProcessIdentifier = ProcessIdentifier { pid: 333333, uid: 3000 };

    struct TestAuthzEnforcementFlag(bool);

    impl AuthzEnforcementFlag for TestAuthzEnforcementFlag {
        fn get_enforcement_strategy(&self) -> AuthzStrategy {
            if self.0 {
                AuthzStrategy::default()
            } else {
                AuthzStrategy::Disabled
            }
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
        ) -> anyhow::Result<Arc<AclPolicyModel>> {
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
        ) -> anyhow::Result<Arc<VmAuthzPolicyModel>> {
            unimplemented!("get_vm_authz_policy is not implemented for test")
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

    struct TestServiceUnitFetchListener {
        suds: Arc<Mutex<Vec<ServiceUnitDefinition>>>,
    }

    impl binder::Interface for TestServiceUnitFetchListener {}

    impl IServiceUnitFetchListener for TestServiceUnitFetchListener {
        fn onServiceUnitFound(&self, units: &[ServiceUnitDefinition]) -> binder::Result<()> {
            self.suds.lock().unwrap().extend_from_slice(units);
            Ok(())
        }
    }

    struct TestServiceUnitChangeListener {
        suds: Arc<Mutex<Vec<ServiceUnitDefinition>>>,
    }

    impl binder::Interface for TestServiceUnitChangeListener {}

    impl IServiceUnitChangeListener for TestServiceUnitChangeListener {
        fn onServiceUnitRegistered(&self, sud: &ServiceUnitDefinition) -> binder::Result<()> {
            self.suds.lock().unwrap().push(sud.clone());
            Ok(())
        }

        fn onServiceUnitUnregistered(&self, sud: &ServiceUnitDefinition) -> binder::Result<()> {
            self.suds.lock().unwrap().push(sud.clone());
            Ok(())
        }

        fn onServiceUnitChanged(&self, sud: &ServiceUnitDefinition) -> binder::Result<()> {
            self.suds.lock().unwrap().push(sud.clone());
            Ok(())
        }
    }

    fn get_caller_identity_pid() -> ServiceIdentityV2 {
        ServiceIdentityV2 {
            name: ServiceFqinV2 {
                sdvVmName: VM_NAME.to_string(),
                sdvPackageName: CALLER_PACKAGE_NAME.to_string(),
                serviceBundleName: CALLER_SERVICE_BUNDLE_NAME.to_string(),
                serviceInstanceName: CALLER_SERVICE_INSTANCE_NAME.to_string(),
            },
            sid: 1234,
            publicKey: EcPublicKey { keyValue: [45_u8; 32] },
        }
    }

    fn get_target_identity_pid() -> ServiceIdentityV2 {
        ServiceIdentityV2 {
            name: ServiceFqinV2 {
                sdvVmName: VM_NAME.to_string(),
                sdvPackageName: TARGET_PACKAGE_NAME.to_string(),
                serviceBundleName: TARGET_SERVICE_BUNDLE_NAME.to_string(),
                serviceInstanceName: TARGET_SERVICE_INSTANCE_NAME.to_string(),
            },
            sid: 1235,
            publicKey: EcPublicKey { keyValue: [46_u8; 32] },
        }
    }

    fn get_invalid_target_identity_pid() -> ServiceIdentityV2 {
        let target_identity = get_target_identity_pid();
        ServiceIdentityV2 {
            name: ServiceFqinV2 {
                serviceBundleName: INVALID_TARGET_SERVICE_BUNDLE_NAME.to_string(),
                ..target_identity.name
            },
            sid: 1236,
            ..target_identity
        }
    }

    fn create_target_unit_type() -> UnitType {
        UnitType {
            sdvPackageName: TARGET_PACKAGE_NAME.to_string(),
            serviceBundleName: TARGET_SERVICE_BUNDLE_NAME.to_string(),
            typeName: "TargetUnitType".to_string(),
        }
    }

    fn create_invalid_target_unit_type() -> UnitType {
        UnitType {
            sdvPackageName: TARGET_PACKAGE_NAME.to_string(),
            serviceBundleName: INVALID_TARGET_SERVICE_BUNDLE_NAME.to_string(),
            typeName: "TargetUnit-Type".to_string(),
        }
    }

    fn create_test_acl_policy_provider() -> anyhow::Result<TestAclPolicyProvider> {
        let acl_policy_model =
            AclPolicyModel::try_from(text_format::parse_from_str::<AclPolicyProto>(
                r#"
                acls {
                    name: "AllowTargetAcl"
                    include: "vm:*"
                    never_include: "other-vm:*"
                }
                service_units {
                    name: "allow-target-service-unit-name"
                    acl_names: "AllowTargetAcl"
                }
                acls {
                    name: "DisallowTargetAcl"
                    never_include: "vm:*"
                }
                service_units {
                    name: "disallow-target-service-unit-name"
                    acl_names: "DisallowTargetAcl"
                }
                "#,
            )?)?;
        let mut acl_policy_provider = TestAclPolicyProvider::new();
        acl_policy_provider.set_acl_policy(
            VmName::from_str(VM_NAME)?,
            PackageName::from_str(TARGET_PACKAGE_NAME)?,
            Some(ServiceBundleName::from_str(TARGET_SERVICE_BUNDLE_NAME)?),
            acl_policy_model,
        );
        Ok(acl_policy_provider)
    }

    fn register_service_units(service_registry: Arc<ServiceRegistry>) -> anyhow::Result<()> {
        let valid_target_unit_type = create_target_unit_type();
        let invalid_target_unit_type = create_invalid_target_unit_type();
        for (service_unit_name, unit_type, pid) in [
            (ALLOWED_SERVICE_UNIT_NAME, &valid_target_unit_type, &TARGET_PID),
            (DISALLOWED_SERVICE_UNIT_NAME, &valid_target_unit_type, &TARGET_PID),
            (INVALID_SERVICE_UNIT_NAME, &valid_target_unit_type, &TARGET_PID),
            (ALLOWED_SERVICE_UNIT_NAME, &invalid_target_unit_type, &INVALID_TARGET_PID),
        ] {
            let token = service_registry
                .preregister_service_unit(
                    service_unit_name,
                    unit_type,
                    &AccessControl::default(),
                    &ApplicationMetadata::default(),
                    pid,
                )
                .ok_or(anyhow!("Preregister failed"))?;
            service_registry.finalize_service_unit_registration(
                token.token.as_ref().ok_or(anyhow!("token.as_ref()"))?,
                TransportMetadata::default(),
            )?;
        }

        Ok(())
    }

    fn create_sd_agent_service(enable_authz: bool) -> anyhow::Result<ServiceDiscoveryAgentService> {
        let service_registry = Arc::new(ServiceRegistry::default());
        let caller_identity = get_caller_identity_pid();
        let target_identity = get_target_identity_pid();
        let invalid_target_identity = get_invalid_target_identity_pid();
        assert!(service_registry.register_identity_v2(caller_identity, None, &CALLER_PID).is_ok());
        assert!(service_registry.register_identity_v2(target_identity, None, &TARGET_PID).is_ok());
        assert!(service_registry
            .register_identity_v2(invalid_target_identity, None, &INVALID_TARGET_PID)
            .is_ok());
        register_service_units(Arc::clone(&service_registry))?;

        let acl_policy_provider: Arc<dyn AclPolicyProvider> =
            Arc::new(create_test_acl_policy_provider()?);
        let authz_enforcement_flag: Arc<dyn AuthzEnforcementFlag> =
            Arc::new(TestAuthzEnforcementFlag(enable_authz));
        let authz_service = Arc::new(AuthzService::new(
            VmName::from_str(VM_NAME).unwrap(),
            Arc::clone(&acl_policy_provider),
            Arc::clone(&authz_enforcement_flag),
            Arc::new(TestRegistry {}),
        ));
        Ok(ServiceDiscoveryAgentService::new(
            Arc::clone(&service_registry),
            Arc::clone(&authz_service),
        ))
    }

    #[test]
    fn fetch_service_units_by_type_authz_disabled_test() -> anyhow::Result<()> {
        let sd_agent_service = create_sd_agent_service(false)?;
        let suds = Arc::new(Mutex::new(Vec::new()));
        let listener = BnServiceUnitFetchListener::new_binder(
            TestServiceUnitFetchListener { suds: Arc::clone(&suds) },
            BinderFeatures::default(),
        );

        let mut unit_type = create_target_unit_type();
        sd_agent_service.fetch_service_units_by_type(&unit_type, &listener, &CALLER_PID)?;
        assert_eq!(
            suds.lock()
                .unwrap()
                .iter()
                .map(|sud| sud.serviceUnitName.clone())
                .collect::<HashSet<_>>(),
            HashSet::from([
                ALLOWED_SERVICE_UNIT_NAME.to_string(),
                DISALLOWED_SERVICE_UNIT_NAME.to_string()
            ])
        );
        suds.lock().unwrap().clear();

        unit_type.typeName = "UnitTypeWithoutServiceUnits".to_string();
        sd_agent_service.fetch_service_units_by_type(&unit_type, &listener, &CALLER_PID)?;
        assert!(suds.lock().unwrap().is_empty());

        Ok(())
    }

    #[test]
    fn fetch_service_units_by_type_authz_enforced_test() -> anyhow::Result<()> {
        let sd_agent_service = create_sd_agent_service(true)?;
        let suds = Arc::new(Mutex::new(Vec::new()));
        let listener = BnServiceUnitFetchListener::new_binder(
            TestServiceUnitFetchListener { suds: Arc::clone(&suds) },
            BinderFeatures::default(),
        );

        let mut unit_type = create_target_unit_type();
        sd_agent_service.fetch_service_units_by_type(&unit_type, &listener, &CALLER_PID)?;
        assert_eq!(
            suds.lock()
                .unwrap()
                .iter()
                .map(|sud| sud.serviceUnitName.clone())
                .collect::<HashSet<_>>(),
            HashSet::from([ALLOWED_SERVICE_UNIT_NAME.to_string()])
        );
        suds.lock().unwrap().clear();

        unit_type.typeName = "UnitTypeWithoutServiceUnits".to_string();
        sd_agent_service.fetch_service_units_by_type(&unit_type, &listener, &CALLER_PID)?;
        assert!(suds.lock().unwrap().is_empty());

        Ok(())
    }

    #[test]
    fn fetch_service_units_by_name_authz_disabled_test() -> anyhow::Result<()> {
        let sd_agent_service = create_sd_agent_service(false)?;
        let suds = Arc::new(Mutex::new(Vec::new()));
        let listener = BnServiceUnitFetchListener::new_binder(
            TestServiceUnitFetchListener { suds: Arc::clone(&suds) },
            BinderFeatures::default(),
        );

        let params = UnitNameParameters {
            sdvVmName: Some(VM_NAME.to_string()),
            sdvPackageName: TARGET_PACKAGE_NAME.to_string(),
            serviceBundleName: TARGET_SERVICE_BUNDLE_NAME.to_string(),
            unitName: None,
        };
        sd_agent_service.fetch_service_units_by_name(&params, &listener, &CALLER_PID)?;
        assert_eq!(
            suds.lock()
                .unwrap()
                .iter()
                .map(|sud| sud.serviceUnitName.clone())
                .collect::<HashSet<_>>(),
            HashSet::from([
                ALLOWED_SERVICE_UNIT_NAME.to_string(),
                DISALLOWED_SERVICE_UNIT_NAME.to_string()
            ])
        );
        suds.lock().unwrap().clear();

        let wrong_params = UnitNameParameters {
            sdvVmName: Some(VM_NAME.to_string()),
            sdvPackageName: TARGET_PACKAGE_NAME.to_string(),
            serviceBundleName: "UnitTypeWithoutServiceUnits".to_string(),
            unitName: None,
        };
        sd_agent_service.fetch_service_units_by_name(&wrong_params, &listener, &CALLER_PID)?;
        assert!(suds.lock().unwrap().is_empty());

        Ok(())
    }

    #[test]
    fn fetch_service_units_by_name_authz_enforced_test() -> anyhow::Result<()> {
        let sd_agent_service = create_sd_agent_service(true)?;
        let suds = Arc::new(Mutex::new(Vec::new()));
        let listener = BnServiceUnitFetchListener::new_binder(
            TestServiceUnitFetchListener { suds: Arc::clone(&suds) },
            BinderFeatures::default(),
        );

        let params = UnitNameParameters {
            sdvVmName: Some(VM_NAME.to_string()),
            sdvPackageName: TARGET_PACKAGE_NAME.to_string(),
            serviceBundleName: TARGET_SERVICE_BUNDLE_NAME.to_string(),
            unitName: None,
        };
        sd_agent_service.fetch_service_units_by_name(&params, &listener, &CALLER_PID)?;
        assert_eq!(
            suds.lock()
                .unwrap()
                .iter()
                .map(|sud| sud.serviceUnitName.clone())
                .collect::<HashSet<_>>(),
            HashSet::from([ALLOWED_SERVICE_UNIT_NAME.to_string()])
        );
        suds.lock().unwrap().clear();

        let wrong_params = UnitNameParameters {
            sdvVmName: Some(VM_NAME.to_string()),
            sdvPackageName: TARGET_PACKAGE_NAME.to_string(),
            serviceBundleName: "UnitTypeWithoutServiceUnits".to_string(),
            unitName: None,
        };
        sd_agent_service.fetch_service_units_by_name(&wrong_params, &listener, &CALLER_PID)?;
        assert!(suds.lock().unwrap().is_empty());

        Ok(())
    }

    #[test]
    fn get_service_unit_authz_disabled_test() -> anyhow::Result<()> {
        let sd_agent_service = create_sd_agent_service(false)?;
        let target_fqin = get_target_identity_pid().name;
        let invalid_target_fqin = get_invalid_target_identity_pid().name;
        let allowed = "allow-target-service-unit-name";
        let disallowed = "disallow-target-service-unit-name";
        let nonexistent = "nonexistent-target-service-unit-name";
        let invalid = "InvalidTargetServiceUnitName";
        assert!(matches!(
            sd_agent_service.get_service_unit(&target_fqin, allowed, &CALLER_PID),
            Ok(Some(_))
        ));
        assert!(matches!(
            sd_agent_service.get_service_unit(&target_fqin, disallowed, &CALLER_PID),
            Ok(Some(_))
        ));
        assert!(matches!(
            sd_agent_service.get_service_unit(&target_fqin, nonexistent, &CALLER_PID),
            Ok(None)
        ));
        assert!(sd_agent_service.get_service_unit(&target_fqin, invalid, &CALLER_PID).is_err());
        assert!(sd_agent_service
            .get_service_unit(&invalid_target_fqin, allowed, &CALLER_PID)
            .is_err());
        Ok(())
    }

    #[test]
    fn get_service_unit_authz_enforced_test() -> anyhow::Result<()> {
        let sd_agent_service = create_sd_agent_service(true)?;
        let target_fqin = get_target_identity_pid().name;
        let invalid_target_fqin = get_invalid_target_identity_pid().name;
        let allowed = "allow-target-service-unit-name";
        let disallowed = "disallow-target-service-unit-name";
        let nonexistent = "nonexistent-target-service-unit-name";
        let invalid = "InvalidTargetServiceUnitName";
        assert!(matches!(
            sd_agent_service.get_service_unit(&target_fqin, allowed, &CALLER_PID),
            Ok(Some(_))
        ));
        assert!(sd_agent_service.get_service_unit(&target_fqin, disallowed, &CALLER_PID).is_err());
        assert!(matches!(
            sd_agent_service.get_service_unit(&target_fqin, nonexistent, &CALLER_PID),
            Ok(None)
        ));
        assert!(sd_agent_service.get_service_unit(&target_fqin, invalid, &CALLER_PID).is_err());
        assert!(sd_agent_service
            .get_service_unit(&invalid_target_fqin, allowed, &CALLER_PID)
            .is_err());
        assert!(sd_agent_service
            .get_service_unit(&invalid_target_fqin, allowed, &CALLER_PID)
            .is_err());
        Ok(())
    }

    #[test]
    fn register_unregister_service_unit_change_listener_by_type_authz_disabled_test(
    ) -> anyhow::Result<()> {
        let sd_agent_service = create_sd_agent_service(false)?;
        let suds = Arc::new(Mutex::new(Vec::new()));
        let listener = BnServiceUnitChangeListener::new_binder(
            TestServiceUnitChangeListener { suds: Arc::clone(&suds) },
            BinderFeatures::default(),
        );

        // Register
        let unit_type = create_target_unit_type();
        sd_agent_service.register_service_unit_change_listener_by_type(
            &unit_type,
            &listener,
            &CALLER_PID,
        )?;
        assert_eq!(
            suds.lock()
                .unwrap()
                .iter()
                .map(|sud| sud.serviceUnitName.clone())
                .collect::<HashSet<_>>(),
            HashSet::from([
                ALLOWED_SERVICE_UNIT_NAME.to_string(),
                DISALLOWED_SERVICE_UNIT_NAME.to_string()
            ])
        );
        suds.lock().unwrap().clear();

        // Unregister
        sd_agent_service.unregister_service_unit_change_listener_by_type(
            &unit_type,
            &listener,
            &CALLER_PID,
        )?;
        register_service_units(Arc::clone(&sd_agent_service.service_registry))?;
        assert!(suds.lock().unwrap().is_empty());

        Ok(())
    }

    #[test]
    fn register_unregister_service_unit_change_listener_by_type_authz_enforced_test(
    ) -> anyhow::Result<()> {
        let sd_agent_service = create_sd_agent_service(true)?;
        let suds = Arc::new(Mutex::new(Vec::new()));
        let listener = BnServiceUnitChangeListener::new_binder(
            TestServiceUnitChangeListener { suds: Arc::clone(&suds) },
            BinderFeatures::default(),
        );

        // Register
        let unit_type = create_target_unit_type();
        sd_agent_service.register_service_unit_change_listener_by_type(
            &unit_type,
            &listener,
            &CALLER_PID,
        )?;
        assert_eq!(
            suds.lock()
                .unwrap()
                .iter()
                .map(|sud| sud.serviceUnitName.clone())
                .collect::<HashSet<_>>(),
            HashSet::from([ALLOWED_SERVICE_UNIT_NAME.to_string(),])
        );
        suds.lock().unwrap().clear();

        // Unregister
        sd_agent_service.unregister_service_unit_change_listener_by_type(
            &unit_type,
            &listener,
            &CALLER_PID,
        )?;
        register_service_units(Arc::clone(&sd_agent_service.service_registry))?;
        assert!(suds.lock().unwrap().is_empty());

        Ok(())
    }

    #[test]
    fn register_unregister_service_unit_change_listener_by_name_authz_disabled_test(
    ) -> anyhow::Result<()> {
        let sd_agent_service = create_sd_agent_service(false)?;
        let suds = Arc::new(Mutex::new(Vec::new()));
        let listener = BnServiceUnitChangeListener::new_binder(
            TestServiceUnitChangeListener { suds: Arc::clone(&suds) },
            BinderFeatures::default(),
        );
        let params = UnitNameParameters {
            sdvVmName: Some(VM_NAME.to_string()),
            sdvPackageName: TARGET_PACKAGE_NAME.to_string(),
            serviceBundleName: TARGET_SERVICE_BUNDLE_NAME.to_string(),
            unitName: None,
        };

        // Register
        sd_agent_service.register_service_unit_change_listener_by_name(
            &params,
            &listener,
            &CALLER_PID,
        )?;
        assert_eq!(
            suds.lock()
                .unwrap()
                .iter()
                .map(|sud| sud.serviceUnitName.clone())
                .collect::<HashSet<_>>(),
            HashSet::from([
                ALLOWED_SERVICE_UNIT_NAME.to_string(),
                DISALLOWED_SERVICE_UNIT_NAME.to_string()
            ])
        );
        suds.lock().unwrap().clear();

        // Unregister
        sd_agent_service.unregister_service_unit_change_listener_by_name(
            &params,
            &listener,
            &CALLER_PID,
        )?;
        register_service_units(Arc::clone(&sd_agent_service.service_registry))?;
        assert!(suds.lock().unwrap().is_empty());

        Ok(())
    }

    #[test]
    fn register_unregister_service_unit_change_listener_by_name_authz_enforced_test(
    ) -> anyhow::Result<()> {
        let sd_agent_service = create_sd_agent_service(true)?;
        let suds = Arc::new(Mutex::new(Vec::new()));
        let listener = BnServiceUnitChangeListener::new_binder(
            TestServiceUnitChangeListener { suds: Arc::clone(&suds) },
            BinderFeatures::default(),
        );
        let params = UnitNameParameters {
            sdvVmName: Some(VM_NAME.to_string()),
            sdvPackageName: TARGET_PACKAGE_NAME.to_string(),
            serviceBundleName: TARGET_SERVICE_BUNDLE_NAME.to_string(),
            unitName: None,
        };

        // Register
        sd_agent_service.register_service_unit_change_listener_by_name(
            &params,
            &listener,
            &CALLER_PID,
        )?;
        assert_eq!(
            suds.lock()
                .unwrap()
                .iter()
                .map(|sud| sud.serviceUnitName.clone())
                .collect::<HashSet<_>>(),
            HashSet::from([ALLOWED_SERVICE_UNIT_NAME.to_string(),])
        );
        suds.lock().unwrap().clear();

        // Unregister
        sd_agent_service.unregister_service_unit_change_listener_by_name(
            &params,
            &listener,
            &CALLER_PID,
        )?;
        register_service_units(Arc::clone(&sd_agent_service.service_registry))?;
        assert!(suds.lock().unwrap().is_empty());

        Ok(())
    }
}
