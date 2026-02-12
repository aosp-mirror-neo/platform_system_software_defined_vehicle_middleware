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

//! Submodule that implements the Service Discovery manager functions.

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
    AccessControl::AccessControl as AccessControlAidl,
    RegistrationToken::RegistrationToken as RegistrationTokenAidl,
};

use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::{
    IServiceUnitChangeListener::BnServiceUnitChangeListener,
    IServiceUnitChangeListener::IServiceUnitChangeListener,
    IServiceUnitFetchListener::BnServiceUnitFetchListener,
    IServiceUnitFetchListener::IServiceUnitFetchListener,
    ServiceUnitDefinition::ServiceUnitDefinition as ServiceUnitDefinitionAidl,
};

use google_sdv_identity::binder::{
    BinderFeatures, Interface, Result as BinderResult, Status, Strong,
};
use log::error;
use std::ffi::CString;
use std::sync::{Arc, Mutex};

use sdv_comms_common::callback::ffi::Callback;
use sdv_comms_ctx::ContextImpl;
use sdv_comms_id::ServiceFqin;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};

use crate::discovery::{ServiceUnitDefinition, UnitNameDiscoveryArgs, UnitType};
use crate::ffi::ASDV_ServiceUnitChangeEventType::{
    ASDV_SERVICE_UNIT_REGISTERED, ASDV_SERVICE_UNIT_UNREGISTERED,
};
use crate::ffi::{ASDV_ServiceUnitChangeEvent, ASDV_ServiceUnitDefinition};
use crate::registration::UnitRegistrationArgs;

/// Registers a new Service Unit with the ServiceRegistrationAgent.
pub async fn register_service_unit(
    ctx: &ContextImpl,
    args: UnitRegistrationArgs,
) -> SdvResult<RegistrationTokenAidl> {
    let sr_agent = ctx.get_agents().get_sr_binder();
    let unit_name = args.get_unit_name().ok_or_else(|| {
        SdvStatus::with_message(SdvStatusCode::FailedPrecondition, "UnitName is not set")
    })?;
    let metadata = args.get_unit_metadata().ok_or_else(|| {
        SdvStatus::with_message(SdvStatusCode::FailedPrecondition, "UnitMetadata is not set")
    })?;
    let token = sr_agent.registerServiceUnit(
        unit_name.name(),
        &args.get_unit_type().clone().into(),
        &AccessControlAidl::default(),
        &metadata.clone().into(),
    )?;

    Ok(token)
}

/// Unregisters a Service Unit with the ServiceRegistrationAgent.
pub fn unregister_service_unit(ctx: &ContextImpl, unit_name: &str) -> SdvResult<()> {
    let sr_agent = ctx.get_agents().get_sr_binder();
    sr_agent.unregisterServiceUnit(unit_name).map_err(SdvStatus::from)
}

struct ServiceUnitFetchListener {
    sender: Arc<Mutex<Vec<ServiceUnitDefinition>>>,
}

impl Interface for ServiceUnitFetchListener {}

#[allow(non_snake_case)] // AIDL generated
impl IServiceUnitFetchListener for ServiceUnitFetchListener {
    fn onServiceUnitFound(&self, units: &[ServiceUnitDefinitionAidl]) -> BinderResult<()> {
        self.sender.lock().unwrap().extend(units.iter().map(|item| {
            ServiceUnitDefinition::try_from(item.clone())
                .expect("Trusted process side (agent) should pass valid unit name")
        }));
        Ok(())
    }
}

/// Returns list of ServiceUnitDefinition objects one for each of the discovered Unit
pub async fn list_service_units_by_type(
    ctx: &ContextImpl,
    unit_type: &UnitType,
) -> SdvResult<Vec<ServiceUnitDefinition>> {
    let sd_agent = ctx.get_agents().get_sd_binder();
    let unit_type_aidl = unit_type.clone().into();

    let sender = Arc::new(Mutex::new(Vec::new()));
    let listener = BnServiceUnitFetchListener::new_binder(
        ServiceUnitFetchListener { sender: Arc::clone(&sender) },
        BinderFeatures::default(),
    );

    sd_agent.fetchServiceUnitsByType(&unit_type_aidl, &listener).map_err(SdvStatus::from)?;
    let result = std::mem::take(&mut *sender.lock().unwrap());
    Ok(result)
}

/// Returns list of ServiceUnitDefinition objects one for each of the discovered Unit
pub async fn list_service_units_by_name(
    ctx: &ContextImpl,
    unit_name: &UnitNameDiscoveryArgs,
) -> SdvResult<Vec<ServiceUnitDefinition>> {
    let sd_agent = ctx.get_agents().get_sd_binder();

    let sender = Arc::new(Mutex::new(Vec::new()));
    let listener = BnServiceUnitFetchListener::new_binder(
        ServiceUnitFetchListener { sender: Arc::clone(&sender) },
        BinderFeatures::default(),
    );
    sd_agent
        .fetchServiceUnitsByName(&unit_name.clone().into(), &listener)
        .map_err(SdvStatus::from)?;
    let result = std::mem::take(&mut *sender.lock().unwrap());
    Ok(result)
}

/// Returns Service Unit identified by the provided the FQIN (Fully * Qualified Instance Name),
/// and UnitName, if it exists.
pub async fn get_service_unit(
    ctx: &ContextImpl,
    fqin: &ServiceFqin,
    service_unit_name: &str,
) -> SdvResult<Option<ServiceUnitDefinition>> {
    let sd_agent = ctx.get_agents().get_sd_binder();

    let aidl_instance = sd_agent.getServiceUnit(&fqin.clone().into(), service_unit_name)?;

    match aidl_instance {
        Some(service_unit) => Ok(Some(service_unit.try_into()?)),
        None => Ok(None),
    }
}

struct ServiceUnitChangeListener {
    callback: Mutex<Callback<Box<ASDV_ServiceUnitChangeEvent>>>,
}

impl ServiceUnitChangeListener {
    fn into_service_unit_definition(
        unit: &ServiceUnitDefinitionAidl,
    ) -> BinderResult<ASDV_ServiceUnitDefinition> {
        let unit = ServiceUnitDefinition::try_from(unit.clone()).map_err(|err| {
            Status::new_service_specific_error(
                -1,
                Some(CString::new(err.to_string().as_str()).unwrap_or_default().as_c_str()),
            )
        })?;
        Ok(ASDV_ServiceUnitDefinition(unit))
    }
}

impl Interface for ServiceUnitChangeListener {}

#[allow(non_snake_case)] // AIDL generated
impl IServiceUnitChangeListener for ServiceUnitChangeListener {
    fn onServiceUnitRegistered(&self, unit: &ServiceUnitDefinitionAidl) -> BinderResult<()> {
        let service_unit_definition = Self::into_service_unit_definition(unit)?;
        let event = Box::new(ASDV_ServiceUnitChangeEvent::new(
            ASDV_SERVICE_UNIT_REGISTERED,
            Box::new(service_unit_definition),
        ));
        if let Err(status) = self.callback.lock().unwrap().invoke(event) {
            error!("onServiceUnitRegistered callback failed with error: {status}");
        }
        Ok(())
    }

    fn onServiceUnitUnregistered(&self, unit: &ServiceUnitDefinitionAidl) -> BinderResult<()> {
        let service_unit_definition = Self::into_service_unit_definition(unit)?;
        let event = Box::new(ASDV_ServiceUnitChangeEvent::new(
            ASDV_SERVICE_UNIT_UNREGISTERED,
            Box::new(service_unit_definition),
        ));
        if let Err(status) = self.callback.lock().unwrap().invoke(event) {
            error!("onServiceUnitUnregistered callback failed with error: {status}");
        }
        Ok(())
    }

    fn onServiceUnitChanged(&self, _unit: &ServiceUnitDefinitionAidl) -> BinderResult<()> {
        todo!()
    }
}

enum ServiceUnitChangeSubscriptionArgument {
    UnitType(UnitType),
    UnitNameDiscoveryArgs(UnitNameDiscoveryArgs),
    Topic(UnitType, String),
}

pub struct ServiceUnitChangeSubscriptionHandle {
    argument: ServiceUnitChangeSubscriptionArgument,
    listener: Strong<dyn IServiceUnitChangeListener>,
}

/// Registers service unit change listener by UnitType.
pub async fn register_service_unit_change_listener_by_type(
    ctx: &ContextImpl,
    unit_type: &UnitType,
    callback: Callback<Box<ASDV_ServiceUnitChangeEvent>>,
) -> SdvResult<ServiceUnitChangeSubscriptionHandle> {
    let sd_agent = ctx.get_agents().get_sd_binder();

    let listener = BnServiceUnitChangeListener::new_binder(
        ServiceUnitChangeListener { callback: Mutex::new(callback) },
        BinderFeatures::default(),
    );

    // TODO use asynchronous version of the interface
    sd_agent
        .registerServiceUnitChangeListenerByType(&unit_type.clone().into(), &listener)
        .map_err(SdvStatus::from)?;

    Ok(ServiceUnitChangeSubscriptionHandle {
        argument: ServiceUnitChangeSubscriptionArgument::UnitType(unit_type.clone()),
        listener,
    })
}

/// Registers service unit change listener by UnitNameDiscoveryArgs.
pub async fn register_service_unit_change_listener_by_name(
    ctx: &ContextImpl,
    unit_name: &UnitNameDiscoveryArgs,
    callback: Callback<Box<ASDV_ServiceUnitChangeEvent>>,
) -> SdvResult<ServiceUnitChangeSubscriptionHandle> {
    let sd_agent = ctx.get_agents().get_sd_binder();

    let listener = BnServiceUnitChangeListener::new_binder(
        ServiceUnitChangeListener { callback: Mutex::new(callback) },
        BinderFeatures::default(),
    );

    // TODO use asynchronous version of the interface
    sd_agent
        .registerServiceUnitChangeListenerByName(&unit_name.clone().into(), &listener)
        .map_err(SdvStatus::from)?;

    Ok(ServiceUnitChangeSubscriptionHandle {
        argument: ServiceUnitChangeSubscriptionArgument::UnitNameDiscoveryArgs(unit_name.clone()),
        listener,
    })
}

/// Registers service unit change listener by Topic.
pub async fn register_service_unit_change_listener_by_topic(
    ctx: &ContextImpl,
    unit_type: &UnitType,
    topic_name: &str,
    callback: Callback<Box<ASDV_ServiceUnitChangeEvent>>,
) -> SdvResult<ServiceUnitChangeSubscriptionHandle> {
    let sd_agent = ctx.get_agents().get_sd_binder();

    let listener = BnServiceUnitChangeListener::new_binder(
        ServiceUnitChangeListener { callback: Mutex::new(callback) },
        BinderFeatures::default(),
    );

    sd_agent
        .registerServiceUnitChangeListenerByTopic(&unit_type.clone().into(), topic_name, &listener)
        .map_err(SdvStatus::from)?;

    Ok(ServiceUnitChangeSubscriptionHandle {
        argument: ServiceUnitChangeSubscriptionArgument::Topic(
            unit_type.clone(),
            topic_name.to_string(),
        ),
        listener,
    })
}

/// Registers service unit change listener by UnitType.
pub async fn unregister_service_unit_change_listener_by_type(
    ctx: &ContextImpl,
    handle: ServiceUnitChangeSubscriptionHandle,
) -> SdvResult<()> {
    let sd_agent = ctx.get_agents().get_sd_binder();

    // TODO use asynchronous version of the interface
    match handle.argument {
        ServiceUnitChangeSubscriptionArgument::UnitType(unit_type) => sd_agent
            .unregisterServiceUnitChangeListenerByType(&unit_type.into(), &handle.listener)
            .map_err(SdvStatus::from)?,

        ServiceUnitChangeSubscriptionArgument::UnitNameDiscoveryArgs(unit_name) => sd_agent
            .unregisterServiceUnitChangeListenerByName(&unit_name.into(), &handle.listener)
            .map_err(SdvStatus::from)?,

        ServiceUnitChangeSubscriptionArgument::Topic(unit_type, topic_name) => sd_agent
            .unregisterServiceUnitChangeListenerByTopic(
                &unit_type.into(),
                &topic_name,
                &handle.listener,
            )
            .map_err(SdvStatus::from)?,
    }
    Ok(())
}

/// Unregisters service unit change listener by Topic.
pub async fn unregister_service_unit_change_listener_by_topic(
    ctx: &ContextImpl,
    unit_type: &UnitType,
    topic_name: &str,
    handle: ServiceUnitChangeSubscriptionHandle,
) -> SdvResult<()> {
    let sd_agent = ctx.get_agents().get_sd_binder();

    sd_agent
        .unregisterServiceUnitChangeListenerByTopic(
            &unit_type.clone().into(),
            topic_name,
            &handle.listener,
        )
        .map_err(SdvStatus::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use sdv_comms_ctx::initialise;
    use sdv_comms_id::names::ServiceUnitName;
    use sdv_comms_id::{ServiceFqin, SERVICE_PUBLIC_KEY_SIZE_BYTES};
    use std::str::FromStr;

    use crate::discovery::UnitType;
    use crate::metadata::ServerUnitMetadata;

    use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::{
        ITransportSupportAgent::BpTransportSupportAgent,
        ITransportSupportAgent::ITransportSupportAgent,
    };

    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
        metadata::TransportMetadata::TransportMetadata,
        // rustfmt single element in {} workaround
    };
    use google_sdv_service_discovery_common::binder::{self, Strong};

    use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::EcPublicKey::EcPublicKey as EcPublicKeyAidl;
    use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::ServiceIdentity as ServiceIdentityAidl;
    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::{
        ApplicationMetadata::{ApplicationMetadata as ApplicationMetadataAidl},
        InferredMetadata::{
            InferredMetadata as InferredMetadataAidl,
            RelativeLocation::RelativeLocation as RelativeLocationAidl
        }
    };

    // Warning: this is integration tests that interact with state of the SD agent.
    // Each device test in Rust executed as a separate process, but test from single compilation unit can execute concurrently.
    // We rely on identity to be cleaned by the death callback after test process death.

    fn create_unit_registration_args(unit_name: &str) -> UnitRegistrationArgs {
        let mut args = UnitRegistrationArgs::default();
        args.set_unit_name(ServiceUnitName::from_str(unit_name).unwrap());
        args.set_unit_type(
            UnitType::builder()
                .package_name("com.test.pkg")
                .service_bundle_name("ServiceBundleSample")
                .unit_type_name("TestInterface")
                .build()
                .unwrap(),
        );
        let mut server_metadata = ServerUnitMetadata::default();
        server_metadata.set_version(1);
        let unit_metadata = args.get_mut_unit_metadata();
        unit_metadata.set_server_metadata(server_metadata);
        args
    }

    fn get_transport_support_agent() -> anyhow::Result<Strong<dyn ITransportSupportAgent>> {
        let transport_binder = {
            let descriptor = <BpTransportSupportAgent as ITransportSupportAgent>::get_descriptor()
                .to_owned()
                + "/default";
            binder::check_interface(&descriptor)?
        };

        Ok(transport_binder)
    }

    #[tokio::test]
    async fn test_register_success() {
        let fqin = ServiceFqin::from_str("vm-name:com.google.testpkg.ServiceName/instance1")
            .expect("ServiceFqin should not be malformed");
        let ctx = initialise(fqin).expect("Context initialisation failed");
        let args = create_unit_registration_args("test");

        register_service_unit(&ctx, args).await.expect("Registration failed");
    }

    #[test]
    fn test_unregister_non_existent_unit_fails() {
        let fqin = ServiceFqin::from_str("vm-name:com.google.testpkg.ServiceName/instance3")
            .expect("ServiceFqin should not be malformed");
        let ctx = initialise(fqin).expect("Context initialisation failed");

        assert!(unregister_service_unit(&ctx, "non-existent-unit").is_err());
    }

    #[tokio::test]
    async fn test_unregister_success() {
        let fqin = ServiceFqin::from_str("vm-name:com.google.testpkg.ServiceName/instance2")
            .expect("ServiceFqin should not be malformed");
        let ctx = initialise(fqin).expect("Context initialisation failed");
        let unit_name = "test-unit";
        let args = create_unit_registration_args(unit_name);

        let token = register_service_unit(&ctx, args).await.expect("Registration failed");

        // TODO(b/361716353) Cider revisit approach to test without dependency on ITransportSupportAgent
        let transport_agent = get_transport_support_agent().expect("Failed to get transport agent");
        let transport_metadata =
            TransportMetadata { value_holder: "some-value".to_string().into() };

        transport_agent
            .setAgentMetadata(&token, &transport_metadata)
            .expect("Failed to set agent metadata");

        unregister_service_unit(&ctx, unit_name).expect("Unregistration failed");
    }

    #[tokio::test]
    #[should_panic]
    async fn test_on_service_unit_found_panics_on_incorrect_unit() {
        let sender = Arc::new(Mutex::new(Vec::new()));
        let listener = BnServiceUnitFetchListener::new_binder(
            ServiceUnitFetchListener { sender: Arc::clone(&sender) },
            BinderFeatures::default(),
        );
        let fqin = ServiceFqin::from_str("vm-name:com.test.pkg.testBundleName/default").unwrap();
        let unit_type = UnitType::from_str("com.google.sdv.WindowManager#Temperature").unwrap();
        let identity_aidl = ServiceIdentityAidl {
            sid: 1234,
            publicKey: EcPublicKeyAidl { keyValue: [45_u8; SERVICE_PUBLIC_KEY_SIZE_BYTES] },
            name: fqin.clone().into(),
        };
        let aidl = ServiceUnitDefinitionAidl {
            // Create a unit name with incorrect format (starting with a capitalized letter)
            serviceUnitName: String::from("Service_unit_name"),
            identity: identity_aidl.clone(),
            unitType: unit_type.clone().into(),
            appMetadata: ApplicationMetadataAidl {
                valueHolder: [1, 1, 1].to_vec(),
                ..Default::default()
            },
            inferredMetadata: InferredMetadataAidl {
                location: RelativeLocationAidl::LOCATION_REMOTE,
            },
        };
        let _ = listener.onServiceUnitFound(&[aidl]);
    }
}
