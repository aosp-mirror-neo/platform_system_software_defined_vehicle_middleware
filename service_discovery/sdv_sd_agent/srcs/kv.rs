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
pub mod death_watcher;
mod identity_registry;
pub mod process_identifier;
mod service_unit_registry;
pub mod unit_name_parameters_wrapper;

#[cfg(test)]
pub(crate) mod test_support;

use anyhow::{anyhow, Context};
use binder::binder_impl::Binder;
use binder::{Interface, SpIBinder};

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
    metadata::ApplicationMetadata::TypedMetadata::TypedMetadata as TypedMetadataAidl,
    AccessControl::AccessControl, RegistrationToken::RegistrationToken as RegistrationTokenV2,
    UnitType::UnitType,
};
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::{
    ServiceUnitDefinition::ServiceUnitDefinition, UnitNameParameters::UnitNameParameters,
};

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::{
    ApplicationMetadata::ApplicationMetadata,
    InferredMetadata::{InferredMetadata, RelativeLocation::RelativeLocation},
    TransportMetadata::TransportMetadata,
};

use google_sdv_identity::aidl::google::sdv::identity::{
    ProcessIdentifier::ProcessIdentifier, ServiceFqin::ServiceFqin as ServiceFqinV2,
    ServiceIdentity::ServiceIdentity as ServiceIdentityV2,
};

use log::{debug, error, info, warn};
use sdv_comms_id::names::ChannelName;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::mpsc;

use self::identity_registry::IdentityRegistry;
pub(crate) use self::identity_registry::Sid;
pub(crate) use self::service_unit_registry::ServiceUnitAndTransport;
use self::service_unit_registry::ServiceUnitRegistry;
use crate::kv::death_watcher::DeathWatcher;
use crate::listeners::Listeners;
use crate::listeners::ServiceUnitChangeListener;
use crate::unit_type_utils::deprecate_unit_type_owner;

pub(crate) use unit_name_parameters_wrapper::InternalUnitNameParameters;

/// ServiceRegistry provides concurrent access to service units and unit types for both local and remote services.
#[derive(Default)]
pub(crate) struct ServiceRegistry {
    storage: Arc<Mutex<RegistryStorage>>,
}

#[allow(clippy::large_enum_variant)] // Largest one is the most common
#[derive(Clone)]
pub(crate) enum ServiceUnitEvent {
    RegisterServiceUnit(ServiceUnitAndTransport),
    UnregisterServiceUnit(ServiceUnitIdentifier),
    UnregisterByIdentity(ServiceFqinV2),
}

#[derive(Default)]
struct RegistryStorage {
    identity_registry: IdentityRegistry,
    service_unit_registry: ServiceUnitRegistry,
    token_service_unit_map: BTreeMap<SpIBinder /* token */, ServiceUnitDefinition>,
    listeners: Listeners,
}

impl RegistryStorage {
    fn remove_identity_impl(&mut self, identity: &ServiceIdentityV2) {
        debug!("Removing identity {identity:?}");
        let removed_service_units = self
            .service_unit_registry
            .remove_if(|_, _, sud| sud.get_service_unit_definition().identity == *identity);
        let _ = self.identity_registry.remove_identity(identity);

        for sud in &removed_service_units {
            self.listeners.on_service_unit_removed(sud);
        }
    }

    fn add_service_unit_internal(
        &mut self,
        service_unit_definition: ServiceUnitDefinition,
        transport_metadata: TransportMetadata,
    ) {
        let sid: Sid = service_unit_definition.identity.sid.into();
        let unit_name = service_unit_definition.serviceUnitName.clone();
        let sd = ServiceUnitAndTransport::new(service_unit_definition.clone(), transport_metadata);

        self.service_unit_registry.insert(sid, unit_name, sd);

        self.listeners.on_service_unit_added(&service_unit_definition);
    }
}

/// Unique runtime identifier for a service unit
#[derive(Debug, PartialEq, Clone)]
pub(crate) struct ServiceUnitIdentifier {
    pub sid: Sid,
    pub name: String,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub(crate) enum ServiceRegistryError {
    #[error("Identity not found")]
    IdentityNotFound,

    #[error("Token not found")]
    TokenNotFound,

    #[error("Identity with the same SID {sid:?} already exists with FQIN: {existing_fqin:?}")]
    SidAlreadyExists { sid: Sid, existing_fqin: ServiceFqinV2 },

    #[error("Identity with the same FQIN {fqin:?} already exists with SID: {existing_sid:?}")]
    FqinAlreadyExists { existing_sid: Sid, fqin: ServiceFqinV2 },

    #[error("Process identity already registered with FQIN: {existing_fqin:?}")]
    ProcessIdentityAlreadyExists { existing_fqin: ServiceFqinV2 },
}

impl ServiceRegistry {
    pub(crate) fn add_remote_service_unit(
        &self,
        mut unit_definition: ServiceUnitDefinition,
        transport_metadata: TransportMetadata,
    ) -> Result<(), ServiceRegistryError> {
        // TODO: Distinguish the case whether the unit located on the local ECU (LOCATION_LOCAL_HARDWARE)
        unit_definition.inferredMetadata =
            InferredMetadata { location: RelativeLocation::LOCATION_REMOTE };

        let mut storage = self.storage.lock().unwrap();

        storage.identity_registry.register_remote_identity(unit_definition.identity.clone())?;

        storage.add_service_unit_internal(unit_definition, transport_metadata);
        Ok(())
    }

    /// Returns list of VM names that have registered identities.
    /// This method is intended to use in dump() API
    pub(crate) fn get_known_vm_names(&self) -> Vec<String> {
        let storage = self.storage.lock().unwrap();
        storage.identity_registry.get_known_vm_names()
    }

    /// Returns list of service units for the given VM name.
    /// This method is intended to use in dump() API
    pub(crate) fn get_all_service_units_by_vm_name(
        &self,
        vm_name: &str,
    ) -> Vec<ServiceUnitAndTransport> {
        let storage = self.storage.lock().unwrap();
        storage
            .service_unit_registry
            .iter()
            .filter(|(_, _, sud)| {
                sud.get_service_unit_definition().identity.name.sdvVmName == vm_name
            })
            .map(|(_, _, sud)| sud.clone())
            .collect()
    }

    /// Atomically returns the current set of local service units and subscribes for future changes.
    /// called from mesh directly
    pub(crate) fn get_local_service_units_and_subscribe_for_changes(
        &self,
        vm_name: &str,
    ) -> (Vec<ServiceUnitAndTransport>, mpsc::UnboundedReceiver<ServiceUnitEvent>) {
        let mut storage: MutexGuard<RegistryStorage> = self.storage.lock().unwrap();
        let existing = storage
            .service_unit_registry
            .iter()
            .map(|(_, _, sud)| sud.clone())
            .filter(|sud| sud.get_service_unit_definition().identity.name.sdvVmName == vm_name)
            .collect();

        let (sender, receiver) = mpsc::unbounded_channel();

        storage.listeners.add_local_unit_listener(sender);

        (existing, receiver)
    }

    /// Removes all Services registered with the provided identity.
    /// This function might be called from The MeshManager.
    pub(crate) fn remove_by_identity_v2(&self, identity: &ServiceIdentityV2) {
        let mut storage = self.storage.lock().unwrap();

        storage.remove_identity_impl(identity);
    }

    pub(crate) fn remove_by_vm_name(&self, vm_name: &str) {
        let mut storage = self.storage.lock().unwrap();

        let removed_service_units = storage.service_unit_registry.remove_if(|_, _, value| {
            value.get_service_unit_definition().identity.name.sdvVmName == vm_name
        });

        // Remove all identities registered for the remote VM.
        let _ = storage.identity_registry.remove_all_by_vm_name(vm_name);

        for sud in &removed_service_units {
            storage.listeners.on_service_unit_removed(sud);
        }
    }

    pub(crate) fn list_service_units_by_unit_type(
        &self,
        unit_type: &UnitType,
    ) -> Vec<ServiceUnitDefinition> {
        let unit_type = deprecate_unit_type_owner(unit_type.clone());

        let storage = self.storage.lock().unwrap();
        storage
            .service_unit_registry
            .iter()
            .filter_map(|(_, _, sdi)| {
                let sud = sdi.get_service_unit_definition();
                if sud.unitType == unit_type {
                    Some(sud.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(crate) fn list_service_units_by_name(
        &self,
        query: &UnitNameParameters,
    ) -> Vec<ServiceUnitDefinition> {
        let internal_query = InternalUnitNameParameters(query.clone());

        let storage = self.storage.lock().unwrap();
        storage
            .service_unit_registry
            .iter()
            .filter_map(|(_, _, sdi)| {
                let sud = sdi.get_service_unit_definition();
                if internal_query.is_query_match(sud) {
                    Some(sud.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<ServiceUnitDefinition>>()
    }

    pub(crate) fn get_service_unit(
        &self,
        fqin: &ServiceFqinV2,
        service_unit_name: &str,
    ) -> Option<ServiceUnitDefinition> {
        let storage = self.storage.lock().unwrap();
        let unit = storage
            .service_unit_registry
            .iter()
            .find(|(_, _, sdi)| {
                let sud = sdi.get_service_unit_definition();
                sud.identity.name == *fqin && sud.serviceUnitName == service_unit_name
            })
            .map(|(_, _, sdi)| sdi.get_service_unit_definition().clone());
        unit // local variable helps with iter lifetimes
    }

    /// Removes the Service Unit registered for the `process_identifier` and the provided
    /// `service_unit_name`.
    ///
    /// This function returns `Some(ServiceUnitIdentifier)` if the Service Unit was removed,
    /// or `None` if the Service Unit is not registered.
    pub(crate) fn remove_local_service_unit(
        &self,
        service_unit_name: &str,
        process_identifier: &ProcessIdentifier,
    ) -> Option<ServiceUnitIdentifier> {
        let mut storage = self.storage.lock().unwrap();
        let existing_caller_identity =
            storage.identity_registry.get_identity_for_process(process_identifier).cloned()?;

        let unit_to_remove = ServiceUnitIdentifier {
            sid: existing_caller_identity.sid.into(),
            name: service_unit_name.to_string(),
        };

        if let Some(service_unit_definition) = storage.service_unit_registry.remove(&unit_to_remove)
        {
            storage.listeners.on_service_unit_removed(&service_unit_definition);
        } else {
            return None;
        }

        storage.listeners.notify_local_unit_event(ServiceUnitEvent::UnregisterServiceUnit(
            unit_to_remove.clone(),
        ));

        Some(unit_to_remove)
    }

    /// Removes the remote Service Unit with the provided `service_unit_id`.
    ///
    /// This function is intended to be called by the MeshManager when notified that a remote unit was removed.
    pub(crate) fn remove_remote_service_unit(&self, service_unit_id: &ServiceUnitIdentifier) {
        let mut storage = self.storage.lock().unwrap();
        if let Some(service_unit_definition) = storage.service_unit_registry.remove(service_unit_id)
        {
            storage.listeners.on_service_unit_removed(&service_unit_definition);
        }
    }

    fn match_service_fqin_without_vm_name(fqin1: &ServiceFqinV2, fqin2: &ServiceFqinV2) -> bool {
        fqin1.sdvPackageName == fqin2.sdvPackageName
            && fqin1.serviceBundleName == fqin2.serviceBundleName
            && fqin1.serviceInstanceName == fqin2.serviceInstanceName
    }

    fn populate_death_watcher(
        &self,
        death_watcher: &mut Option<Box<dyn DeathWatcher>>,
        identity: ServiceIdentityV2,
    ) {
        if let Some(watcher) = death_watcher {
            let storage = Arc::downgrade(&self.storage);

            let callback = Box::new(move || {
                let Some(strong_storage) = storage.upgrade() else {
                    warn!("Unable to get storage in death watcher callback");
                    return;
                };
                let mut storage = strong_storage.lock().unwrap();
                storage.remove_identity_impl(&identity);

                storage.listeners.notify_local_unit_event(ServiceUnitEvent::UnregisterByIdentity(
                    identity.name.clone(),
                ));
            });

            if watcher.link_to_death(callback).is_err() {
                warn!("Unable to link to death");
            }
        }
    }

    /// Attempts to registers local `identity` for `process_identifier` and returns Ok(None) if identity have been added.
    /// If the same identity have already been registered by the caller process, does nothing and return Ok(Some(existing_identity)).
    /// If the identity with the same FQIN is already registered by other process returns Err(FqinAlreadyExists).
    /// If the identity with the same SID is already registered by other process returns Err(SidAlreadyExists).
    /// If the identity with the different FQIN is already registered by the same process returns Err(ProcessIdentityAlreadyExists).
    pub(crate) fn register_identity_v2(
        &self,
        identity: ServiceIdentityV2,
        mut death_watcher: Option<Box<dyn DeathWatcher>>,
        process_identifier: &ProcessIdentifier,
    ) -> Result<Option<ServiceIdentityV2>, ServiceRegistryError> {
        info!(
            "registering identity {identity:?} for UID {}, PID {}",
            process_identifier.uid, process_identifier.pid
        );

        let mut storage = self.storage.lock().unwrap();

        if let Some(existing_caller_identity) =
            storage.identity_registry.get_identity_for_process(process_identifier)
        {
            if Self::match_service_fqin_without_vm_name(
                &existing_caller_identity.name,
                &identity.name,
            ) {
                info!(
                    "Found the same identity already registered for a caller: {existing_caller_identity:?}"
                );
                return Ok(Some(existing_caller_identity.clone()));
            }

            let err = ServiceRegistryError::ProcessIdentityAlreadyExists {
                existing_fqin: existing_caller_identity.name.clone(),
            };
            error!("Failed to register identity: {err}");
            return Err(err);
        }

        self.populate_death_watcher(&mut death_watcher, identity.clone());

        storage.identity_registry.register_local_identity(
            identity,
            death_watcher,
            process_identifier,
        )?;
        Ok(None)
    }

    pub(crate) fn find_identity_by_sid_v2(&self, id: i64) -> Option<ServiceIdentityV2> {
        let storage = self.storage.lock().unwrap();
        storage.identity_registry.get_identity(id as u64).cloned()
    }

    pub(crate) fn find_identity_by_fqin_v2(
        &self,
        fqin: &ServiceFqinV2,
    ) -> Option<ServiceIdentityV2> {
        let storage = self.storage.lock().unwrap();
        storage.identity_registry.get_identity_by_fqin(fqin).cloned()
    }

    /// Returns Service FQIN of the given process.
    pub(crate) fn get_process_fqin(
        &self,
        pid: &ProcessIdentifier,
    ) -> anyhow::Result<sdv_comms_id::ServiceFqin> {
        let identity = self
            .find_identity_by_process_identifier(pid)
            .ok_or_else(|| anyhow!("Identity not found for process {pid:?}"))?;
        let aidl_fqin = identity.name;

        let fqin = sdv_comms_id::ServiceFqin::try_from(aidl_fqin)
            .with_context(|| format!("Failed to create valid FQIN for process {pid:?}"))?;

        Ok(fqin)
    }

    pub(crate) fn find_identity_by_process_identifier(
        &self,
        process_identifier: &ProcessIdentifier,
    ) -> Option<ServiceIdentityV2> {
        let storage = self.storage.lock().unwrap();
        storage.identity_registry.get_identity_for_process(process_identifier).cloned()
    }

    /// Performs first step of service unit registration and returns a token for continuation.
    ///
    /// The first out of two steps of unit registrations partially populates metadata and returns
    /// a token required to finalize registration by populating transport metadata.
    /// See `finalize_service_unit_registration` for the second step.
    pub(crate) fn preregister_service_unit(
        &self,
        service_unit_name: &str,
        unit_type: &UnitType,
        _acls: &AccessControl,
        app_metadata: &ApplicationMetadata,
        process_identifier: &ProcessIdentifier,
    ) -> Option<RegistrationTokenV2> {
        let unit_type = deprecate_unit_type_owner(unit_type.clone());

        // TODO(b/384016427): Check if:
        // 1. The service unit name should not exist for the same ServiceBundle (should be a new ServiceUnitName).
        // 2. The UnitType should be a valid UnitType.
        // 3. The ACLs should exist.
        // 4. app_metadata should contain at least one of publisher_metadata or server_metadata.

        let mut storage = self.storage.lock().unwrap();
        let identity = storage.identity_registry.get_identity_for_process(process_identifier)?;

        let sud = ServiceUnitDefinition {
            identity: identity.clone(),
            serviceUnitName: service_unit_name.to_string(),
            unitType: unit_type,
            appMetadata: app_metadata.clone(),
            inferredMetadata: InferredMetadata {
                location: RelativeLocation::LOCATION_LOCAL_SDV_INSTANCE,
            },
        };

        let binder_token = Binder::new(()).as_binder();
        let token =
            RegistrationTokenV2 { identity: identity.clone(), token: Some(binder_token.clone()) };
        storage.token_service_unit_map.insert(binder_token, sud);
        Some(token)
    }

    pub(crate) fn get_service_unit_for_token(
        &self,
        token: &SpIBinder,
    ) -> Option<ServiceUnitDefinition> {
        // search locally
        let storage = self.storage.lock().unwrap();
        storage.token_service_unit_map.get(token).cloned()
    }

    /// Completes service unit registration by populating transport metadata.
    ///
    /// The second out of two steps of unit registrations. It takes a token
    /// returned by `preregister_service_unit` and populates transport metadata.
    /// The token ensures that the caller is the one who initiated the registration.
    pub(crate) fn finalize_service_unit_registration(
        &self,
        token: &SpIBinder,
        transport_metadata: TransportMetadata,
    ) -> Result<(), ServiceRegistryError> {
        let mut storage = self.storage.lock().unwrap();
        let (_, sud) = storage
            .token_service_unit_map
            .remove_entry(token)
            .ok_or(ServiceRegistryError::TokenNotFound)?;

        let sut: ServiceUnitAndTransport =
            ServiceUnitAndTransport::new(sud.clone(), transport_metadata.clone());
        storage.add_service_unit_internal(sud, transport_metadata);

        storage
            .listeners
            .notify_local_unit_event(ServiceUnitEvent::RegisterServiceUnit(sut.clone()));

        Ok(())
    }

    /// Writes the ServiceUnitDefinition and TransportMetadata to the provided arguments; and returns true.
    /// Returns false if the Service Unit with the given `sid` and `service_unit_name` does not exist.
    pub(crate) fn get_transport_metadata_for_service_unit(
        &self,
        sid: i64,
        service_unit_name: &str,
        service_unit_defn_out: &mut ServiceUnitDefinition,
        transport_metadata_out: &mut TransportMetadata,
    ) -> bool {
        let storage = self.storage.lock().unwrap();
        if let Some(sud) =
            storage.service_unit_registry.find_by_sid_and_name(sid.into(), service_unit_name)
        {
            service_unit_defn_out.clone_from(sud.get_service_unit_definition());
            transport_metadata_out.clone_from(sud.get_transport_metadata());
            return true;
        }
        false
    }

    pub(crate) fn register_service_unit_change_listener_by_type(
        &self,
        unit_type: &UnitType,
        listener: ServiceUnitChangeListener,
    ) -> binder::Result<()> {
        let unit_type = deprecate_unit_type_owner(unit_type.clone());

        let mut storage: MutexGuard<RegistryStorage> = self.storage.lock().unwrap();
        storage.listeners.register_listener_by_type(&unit_type, listener.clone());

        let service_defns = storage
            .service_unit_registry
            .iter()
            .filter_map(|(_, _, sdi)| {
                let sud = sdi.get_service_unit_definition();
                if sud.unitType == unit_type {
                    Some(sud.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<ServiceUnitDefinition>>();
        drop(storage);

        for service_defn in service_defns {
            if let Err(e) = listener.on_service_unit_registered(&service_defn) {
                warn!("ServiceUnitChangeListener::onServiceUnitRegistered() failed; {e:?}");
            }
        }

        Ok(())
    }

    pub(crate) fn unregister_service_unit_change_listener_by_type(
        &self,
        unit_type: &UnitType,
        listener: ServiceUnitChangeListener,
    ) -> binder::Result<()> {
        let unit_type = deprecate_unit_type_owner(unit_type.clone());

        let mut storage = self.storage.lock().unwrap();
        storage.listeners.unregister_listener_by_type(&unit_type, listener);

        Ok(())
    }

    pub(crate) fn register_service_unit_change_listener_by_channel(
        &self,
        unit_type: &UnitType,
        channel_name: &ChannelName,
        listener: ServiceUnitChangeListener,
    ) -> binder::Result<()> {
        let unit_type = deprecate_unit_type_owner(unit_type.clone());

        let service_defns = {
            let mut storage: MutexGuard<RegistryStorage> = self.storage.lock().unwrap();
            storage.listeners.register_listener_by_channel(
                &unit_type,
                channel_name,
                listener.clone(),
            );

            let service_defns = storage
                .service_unit_registry
                .iter()
                .filter_map(|(_, _, sdi)| {
                    let sud = sdi.get_service_unit_definition();
                    if sud.unitType == unit_type {
                        if let TypedMetadataAidl::PublisherUnit(metadata) =
                            &sud.appMetadata.typedMetadata
                        {
                            if metadata.channel_name != channel_name.as_ref() {
                                return None;
                            }
                            return Some(sud.clone());
                        }
                    }
                    None
                })
                .collect::<Vec<ServiceUnitDefinition>>();
            service_defns
        }; // make sure storage MutexGuard is kept as little as possible

        for service_defn in service_defns {
            if let Err(e) = listener.on_service_unit_registered(&service_defn) {
                warn!("ServiceUnitChangeListener::onServiceUnitRegistered() failed; {e:?}");
            }
        }

        Ok(())
    }

    pub(crate) fn unregister_service_unit_change_listener_by_channel(
        &self,
        unit_type: &UnitType,
        channel_name: &ChannelName,
        listener: ServiceUnitChangeListener,
    ) -> binder::Result<()> {
        let unit_type = deprecate_unit_type_owner(unit_type.clone());

        let mut storage = self.storage.lock().unwrap();
        storage.listeners.unregister_listener_by_channel(&unit_type, channel_name, listener);

        Ok(())
    }

    pub(crate) fn register_service_unit_change_listener_by_name(
        &self,
        unit_name_params: &UnitNameParameters,
        listener: ServiceUnitChangeListener,
    ) -> binder::Result<()> {
        let mut storage = self.storage.lock().unwrap();
        storage.listeners.register_listener_by_name(unit_name_params, listener.clone());
        let query = InternalUnitNameParameters(unit_name_params.clone());
        let service_defns = storage
            .service_unit_registry
            .iter()
            .filter_map(|(_, _, sdi)| {
                let sud = sdi.get_service_unit_definition();
                if query.is_query_match(sud) {
                    Some(sud.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<ServiceUnitDefinition>>();
        drop(storage);

        for service_defn in service_defns {
            if let Err(e) = listener.on_service_unit_registered(&service_defn) {
                warn!("ServiceUnitChangeListener::onServiceUnitRegistered() failed; {e:?}");
            }
        }

        Ok(())
    }

    pub(crate) fn unregister_service_unit_change_listener_by_name(
        &self,
        unit_name_params: &UnitNameParameters,
        listener: ServiceUnitChangeListener,
    ) -> binder::Result<()> {
        let mut storage = self.storage.lock().unwrap();
        storage.listeners.unregister_listener_by_name(unit_name_params, listener);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::EcPublicKey::EcPublicKey;
    use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceUnitChangeListener::BnServiceUnitChangeListener;
    use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceUnitChangeListener::IServiceUnitChangeListener;

    use crate::kv::death_watcher::{self, MockDeathWatcher};
    use crate::kv::service_unit_registry::ServiceUnitAndTransport;

    use super::test_support::{
        get_default_test_unit_type, register_service_unit_for_test,
        register_service_unit_for_test_channel,
    };
    use super::ServiceRegistry;

    use googletest::prelude::*;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};

    #[test]
    fn service_unit_definition_internal() {
        let service_unit_definition = ServiceUnitDefinition {
            serviceUnitName: String::from("service unit name"),
            identity: ServiceIdentityV2 {
                sid: 1234,
                publicKey: EcPublicKey {
                    keyValue: [
                        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
                        21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
                    ],
                },
                name: ServiceFqinV2 {
                    sdvVmName: String::from("vm name"),
                    sdvPackageName: String::from("package name"),
                    serviceBundleName: String::from("service bundle name"),
                    serviceInstanceName: String::from("instance name"),
                },
            },
            unitType: UnitType {
                sdvPackageName: String::from("package name"),
                serviceBundleName: String::from("bundle name"),
                typeName: String::from("type name"),
            },
            appMetadata: ApplicationMetadata {
                valueHolder: [1, 1, 1].to_vec(),
                ..Default::default()
            },
            ..Default::default()
        };

        let internal_service_unit_definition = ServiceUnitAndTransport::new(
            service_unit_definition.clone(),
            TransportMetadata { ..Default::default() },
        );

        assert_eq!(
            service_unit_definition.serviceUnitName,
            internal_service_unit_definition.get_service_unit_definition().serviceUnitName
        );

        assert_eq!(
            service_unit_definition.identity.sid,
            internal_service_unit_definition.get_service_unit_definition().identity.sid
        );
        assert_eq!(
            service_unit_definition.identity.publicKey.keyValue,
            internal_service_unit_definition
                .get_service_unit_definition()
                .identity
                .publicKey
                .keyValue
        );

        assert_eq!(
            service_unit_definition.identity.name.sdvVmName,
            internal_service_unit_definition.get_service_unit_definition().identity.name.sdvVmName
        );
        assert_eq!(
            service_unit_definition.identity.name.sdvPackageName,
            internal_service_unit_definition
                .get_service_unit_definition()
                .identity
                .name
                .sdvPackageName
        );
        assert_eq!(
            service_unit_definition.identity.name.serviceBundleName,
            internal_service_unit_definition
                .get_service_unit_definition()
                .identity
                .name
                .serviceBundleName
        );
        assert_eq!(
            service_unit_definition.identity.name.serviceInstanceName,
            internal_service_unit_definition
                .get_service_unit_definition()
                .identity
                .name
                .serviceInstanceName
        );

        assert_eq!(
            service_unit_definition.appMetadata,
            internal_service_unit_definition.get_service_unit_definition().appMetadata
        );
    }

    #[test]
    fn preregister_service_unit_with_identity() {
        let service_registry = ServiceRegistry::default();

        let unit_type = UnitType { ..Default::default() };
        let service_identity = ServiceIdentityV2 {
            name: ServiceFqinV2 {
                sdvVmName: "some_vm_name".to_string(),
                serviceBundleName: unit_type.serviceBundleName.clone(),
                sdvPackageName: unit_type.sdvPackageName.clone(),
                serviceInstanceName: "instance_1".to_string(),
            },
            ..Default::default()
        };
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };

        let _ = service_registry
            .register_identity_v2(service_identity.clone(), None, &caller_pid)
            .unwrap();

        let acl = AccessControl { ..Default::default() };
        let app_metadata = ApplicationMetadata { ..Default::default() };

        // Because the identity is known we should be able to register a service.
        let token = service_registry
            .preregister_service_unit(
                "service_unit_name",
                &unit_type,
                &acl,
                &app_metadata,
                &caller_pid,
            )
            .expect("Failed to preregister service unit");

        assert_eq!(&token.identity, &service_identity);

        let unit_definition = service_registry
            .get_service_unit_for_token(&token.token.expect("No binder handle in the token"))
            .expect("Unable to find unit by the token after registration");

        assert_eq!(&unit_definition.identity, &service_identity);
        assert_eq!(
            unit_definition.inferredMetadata.location,
            RelativeLocation::LOCATION_LOCAL_SDV_INSTANCE
        );
    }

    #[test]
    fn preregister_service_unit_no_identity() {
        let service_registry = ServiceRegistry::default();

        let unit_type = UnitType { ..Default::default() };
        let acl = AccessControl { ..Default::default() };
        let app_metadata = ApplicationMetadata { ..Default::default() };
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };

        // Check that without registration of the identity we can't register a service.
        assert!(service_registry
            .preregister_service_unit(
                "service_unit_name",
                &unit_type,
                &acl,
                &app_metadata,
                &caller_pid
            )
            .is_none());
    }

    #[test]
    fn add_remote_service_unit_adds_unit_of_remote_kind() {
        let service_registry = ServiceRegistry::default();

        let unit_type = UnitType {
            sdvPackageName: "package_name".to_string(),
            serviceBundleName: "".to_string(), // service bundle name as part of unit types is deprecated
            typeName: "type_name".to_string(),
        };

        let service_identity = ServiceIdentityV2 {
            name: ServiceFqinV2 {
                sdvVmName: "some_vm_name".to_string(),
                serviceBundleName: unit_type.serviceBundleName.clone(),
                sdvPackageName: unit_type.sdvPackageName.clone(),
                serviceInstanceName: "instance_1".to_string(),
            },
            ..Default::default()
        };

        let unit_definition = ServiceUnitDefinition {
            identity: service_identity,
            serviceUnitName: "service_unit_name".to_string(),
            unitType: unit_type.clone(),
            appMetadata: ApplicationMetadata { ..Default::default() },
            ..Default::default()
        };

        let transport_metadata = TransportMetadata { value_holder: "192.168.0.1:8080".into() };

        service_registry
            .add_remote_service_unit(unit_definition.clone(), transport_metadata.clone())
            .expect("Add remote service unit failed");

        // Check that the service is added.
        let service_unit_definitions = service_registry.list_service_units_by_unit_type(&unit_type);
        assert_eq!(service_unit_definitions.len(), 1);
        assert_eq!(service_unit_definitions.first().unwrap().unitType, unit_type);

        // Check that the inferred metadata is set correctly.
        assert!(matches!(
            service_unit_definitions.first().unwrap().inferredMetadata.location,
            RelativeLocation::LOCATION_REMOTE | RelativeLocation::LOCATION_LOCAL_HARDWARE
        ));
    }

    fn get_listener_test_default_unit_type() -> UnitType {
        UnitType {
            sdvPackageName: "package_name".to_string(),
            serviceBundleName: "ListenerServiceBundle".to_string(),
            typeName: "for_listener_type_name".to_string(),
        }
    }

    #[test]
    fn list_service_units_by_unit_type_unregistered() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_default_test_unit_type();

        // Currently there is no service units in the registry.
        let service_unit_definitions = service_registry.list_service_units_by_unit_type(&unit_type);
        assert!(service_unit_definitions.is_empty());
    }

    #[test]
    fn list_service_units_by_unit_type_registered() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_default_test_unit_type();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let _ = register_service_unit_for_test(
            &service_registry,
            "some_vm_name",
            &unit_type,
            &caller_pid,
            123,
        );

        let service_unit_definitions = service_registry.list_service_units_by_unit_type(&unit_type);
        assert_eq!(service_unit_definitions.len(), 1);
        assert_eq!(service_unit_definitions.first().unwrap().unitType, unit_type);
    }

    #[test]
    fn list_service_units_by_name_unregistered() {
        let service_registry = ServiceRegistry::default();

        let query = UnitNameParameters {
            sdvPackageName: "package_name".to_string(),
            serviceBundleName: "bundle_name".to_string(),
            sdvVmName: None,
            unitName: None,
        };

        // There are no registered service units. The result list must be empty.
        let service_unit_definitions = service_registry.list_service_units_by_name(&query);
        assert!(service_unit_definitions.is_empty());
    }

    #[test]
    fn list_service_units_by_name() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_default_test_unit_type();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let _ = register_service_unit_for_test(
            &service_registry,
            "some_vm_name",
            &unit_type,
            &caller_pid,
            123,
        );

        let query = UnitNameParameters {
            sdvPackageName: unit_type.sdvPackageName.clone(),
            serviceBundleName: unit_type.serviceBundleName.clone(),
            sdvVmName: None,
            unitName: None,
        };

        let service_unit_definitions = service_registry.list_service_units_by_name(&query);
        assert_eq!(service_unit_definitions.len(), 1);
        assert_eq!(service_unit_definitions.first().unwrap().unitType, unit_type);

        // Check the behavior of different queries.
        {
            let query_with_vm = UnitNameParameters {
                sdvPackageName: unit_type.sdvPackageName.clone(),
                serviceBundleName: unit_type.serviceBundleName.clone(),
                sdvVmName: Some("some_vm_name".to_string()),
                unitName: None,
            };
            assert_eq!(service_registry.list_service_units_by_name(&query_with_vm).len(), 1);
        }

        {
            let query_with_wrong_vm = UnitNameParameters {
                sdvPackageName: unit_type.sdvPackageName.clone(),
                serviceBundleName: unit_type.serviceBundleName.clone(),
                sdvVmName: Some("another_vm".to_string()),
                unitName: None,
            };
            assert_eq!(service_registry.list_service_units_by_name(&query_with_wrong_vm).len(), 0);
        }

        {
            let query_with_service_unit_name = UnitNameParameters {
                sdvPackageName: unit_type.sdvPackageName.clone(),
                serviceBundleName: unit_type.serviceBundleName.clone(),
                sdvVmName: None,
                unitName: Some("service_unit_name".to_string()),
            };
            assert_eq!(
                service_registry.list_service_units_by_name(&query_with_service_unit_name).len(),
                1
            );
        }

        {
            let query_with_wrong_service_unit_name = UnitNameParameters {
                sdvPackageName: unit_type.sdvPackageName.clone(),
                serviceBundleName: unit_type.serviceBundleName.clone(),
                sdvVmName: None,
                unitName: Some("wrong_service_unit_name".to_string()),
            };
            assert_eq!(
                service_registry
                    .list_service_units_by_name(&query_with_wrong_service_unit_name)
                    .len(),
                0
            );
        }
    }

    #[test]
    fn get_service_unit_unregistered() {
        let service_registry = ServiceRegistry::default();
        let fqin = ServiceFqinV2 {
            sdvVmName: "some_vm_name".to_string(),
            serviceBundleName: "serviceBundleName".to_string(),
            sdvPackageName: "sdvPackageName".to_string(),
            serviceInstanceName: "instance_1".to_string(),
        };

        let service_unit_definition = service_registry.get_service_unit(&fqin, "service_unit_name");
        assert!(service_unit_definition.is_none());
    }

    #[test]
    fn get_service_unit_registered() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_default_test_unit_type();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let fqin = ServiceFqinV2 {
            sdvVmName: "some_vm_name".to_string(),
            serviceBundleName: unit_type.serviceBundleName.clone(),
            sdvPackageName: unit_type.sdvPackageName.clone(),
            serviceInstanceName: "instance_1".to_string(),
        };

        let _ = register_service_unit_for_test(
            &service_registry,
            "some_vm_name",
            &unit_type,
            &caller_pid,
            123,
        );

        let service_unit_definition = service_registry.get_service_unit(&fqin, "service_unit_name");
        assert!(service_unit_definition.is_some());
        assert_eq!(service_unit_definition.unwrap().unitType, unit_type);
    }

    fn get_test_identity() -> ServiceIdentityV2 {
        ServiceIdentityV2 {
            name: ServiceFqinV2 {
                sdvVmName: "vm1 name".to_string(),
                sdvPackageName: "package.name".to_string(),
                serviceBundleName: "service bundle name".to_string(),
                serviceInstanceName: "instance 1".to_string(),
            },
            sid: 1234_i64,
            publicKey: EcPublicKey { keyValue: [45_u8; 32] },
        }
    }

    #[test]
    fn identity_with_registration() {
        let service_registry = ServiceRegistry::default();
        let service_identity = get_test_identity();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let _ = service_registry
            .register_identity_v2(service_identity.clone(), None, &caller_pid)
            .unwrap();

        let identity_by_sid = service_registry.find_identity_by_sid_v2(service_identity.sid);
        assert_eq!(identity_by_sid.unwrap(), service_identity);

        let identity_by_fqin = service_registry.find_identity_by_fqin_v2(&service_identity.name);
        assert_eq!(identity_by_fqin.unwrap(), service_identity);
    }

    #[test]
    fn register_identity_v2_duplicate_returns_the_old_one() {
        let service_registry = ServiceRegistry::default();
        let service_identity = get_test_identity();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };

        assert_that!(
            service_registry.register_identity_v2(service_identity.clone(), None, &caller_pid),
            ok(none())
        );

        // The second attempt to insert the same identity from the same PID succeed
        assert_that!(
            service_registry.register_identity_v2(service_identity.clone(), None, &caller_pid),
            ok(some(eq(&service_identity)))
        );

        // Still can find it afterwards
        assert_eq!(
            service_registry.find_identity_by_sid_v2(service_identity.sid).unwrap(),
            service_identity
        );
    }

    #[test]
    fn register_identity_v2_different_fqin_same_pid_returns_err() {
        let service_registry = ServiceRegistry::default();
        let service_identity_a = get_test_identity();
        let mut service_identity_b = get_test_identity();
        service_identity_b.name.serviceInstanceName = "instance 2".to_string();
        service_identity_b.sid = 4321_i64;
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };

        assert_that!(
            service_registry.register_identity_v2(service_identity_a.clone(), None, &caller_pid),
            ok(none())
        );

        // The second attempt to insert another identity from the same PID fails
        assert_that!(
            service_registry.register_identity_v2(service_identity_b.clone(), None, &caller_pid),
            err(matches_pattern!(ServiceRegistryError::ProcessIdentityAlreadyExists { .. }))
        );
    }

    #[test]
    fn register_identity_v2_death_watcher_removes_identity() {
        let service_registry = ServiceRegistry::default();
        let service_identity = get_test_identity();

        let mut death_watcher = MockDeathWatcher::new();

        let callback_holder: Arc<Mutex<Option<death_watcher::Callback>>> =
            Arc::new(Mutex::new(None));
        let callback_holder_clone = Arc::clone(&callback_holder);

        death_watcher.expect_link_to_death().return_once(move |cb| {
            let _ = callback_holder_clone.lock().unwrap().insert(cb);
            Ok(())
        });
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };

        // Register the identity with the mock death watcher.
        assert_that!(
            service_registry.register_identity_v2(
                service_identity.clone(),
                Some(Box::new(death_watcher)),
                &caller_pid,
            ),
            ok(none())
        );

        // Verify that the identity is registered.
        assert_eq!(
            service_registry.find_identity_by_sid_v2(service_identity.sid).unwrap(),
            service_identity
        );

        // Invoke the death watcher callback.
        let callback_holder = callback_holder.lock().unwrap();
        callback_holder.as_ref().expect("Callback was not set")();

        // Verify that the identity is removed.
        assert!(service_registry.find_identity_by_sid_v2(service_identity.sid).is_none());
    }

    #[test]
    fn identity_without_registration() {
        let service_registry = ServiceRegistry::default();
        let registered_identity = get_test_identity();
        let process_identifier = ProcessIdentifier { pid: 1234, uid: 5678 };
        let _ = service_registry
            .register_identity_v2(registered_identity, None, &process_identifier)
            .unwrap();

        // Let's try to find the not registered identity.
        // All the responses must fail.
        let unregistered_identity = ServiceIdentityV2 {
            name: ServiceFqinV2 {
                sdvPackageName: "another.package.name".to_string(),
                serviceBundleName: "another service bundle name".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(service_registry.find_identity_by_sid_v2(999).is_none());
        assert!(service_registry.find_identity_by_fqin_v2(&unregistered_identity.name).is_none());
    }

    #[test]
    fn identity_empty_registry() {
        let service_registry = ServiceRegistry::default();

        assert!(service_registry.find_identity_by_sid_v2(999).is_none());
        assert!(service_registry
            .find_identity_by_fqin_v2(&ServiceFqinV2 { ..Default::default() })
            .is_none());
    }

    #[test]
    fn remove_local_service_unit_success() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_default_test_unit_type();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let identity = register_service_unit_for_test(
            &service_registry,
            "some_vm_name",
            &unit_type,
            &caller_pid,
            123,
        );

        // Verify that the service unit exists before removal
        assert_eq!(service_registry.list_service_units_by_unit_type(&unit_type).len(), 1);

        assert_eq!(
            service_registry.remove_local_service_unit("service_unit_name", &caller_pid),
            Some(ServiceUnitIdentifier {
                sid: identity.sid.into(),
                name: "service_unit_name".to_string()
            })
        );

        // Verify that the service unit is removed
        assert!(service_registry.list_service_units_by_unit_type(&unit_type).is_empty());
    }

    #[test]
    fn remove_local_service_unit_not_found() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_default_test_unit_type();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };

        register_service_unit_for_test(
            &service_registry,
            "some_vm_name",
            &unit_type,
            &caller_pid,
            123,
        );

        // Attempt to remove a non-existent service unit
        assert!(service_registry
            .remove_local_service_unit("non_existent_service_unit", &caller_pid)
            .is_none());

        // Verify that the original service unit still exists
        assert_eq!(service_registry.list_service_units_by_unit_type(&unit_type).len(), 1);
    }

    #[test]
    fn remove_remote_service_unit_success() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_default_test_unit_type();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let identity = register_service_unit_for_test(
            &service_registry,
            "some_vm_name",
            &unit_type,
            &caller_pid,
            123,
        );

        // Verify that the service unit exists before removal
        assert_eq!(service_registry.list_service_units_by_unit_type(&unit_type).len(), 1);

        let service_unit_id = ServiceUnitIdentifier {
            sid: identity.sid.into(),
            name: "service_unit_name".to_string(),
        };
        service_registry.remove_remote_service_unit(&service_unit_id);

        // Verify that the service unit is removed
        assert!(service_registry.list_service_units_by_unit_type(&unit_type).is_empty());
    }

    #[test]
    fn remove_remote_service_unit_not_found() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_default_test_unit_type();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        register_service_unit_for_test(
            &service_registry,
            "some_vm_name",
            &unit_type,
            &caller_pid,
            123,
        );

        // Attempt to remove a non-existent service unit
        let non_existent_service_unit_id = ServiceUnitIdentifier {
            sid: 999.into(),
            name: "non_existent_service_unit".to_string(),
        };
        service_registry.remove_remote_service_unit(&non_existent_service_unit_id);

        // Verify that the original service unit still exists
        assert_eq!(service_registry.list_service_units_by_unit_type(&unit_type).len(), 1);
    }

    #[test]
    fn find_identity_by_process_identifier_found() {
        let service_registry = ServiceRegistry::default();
        let service_identity = get_test_identity();
        let process_identifier = ProcessIdentifier { pid: 1234, uid: 5678 };

        // Manually register the identity with the process identifier
        {
            let mut storage = service_registry.storage.lock().unwrap();
            storage
                .identity_registry
                .register_identity_internal(
                    service_identity.clone(),
                    Some(process_identifier.clone()),
                    None,
                )
                .unwrap();
        }

        let found_identity =
            service_registry.find_identity_by_process_identifier(&process_identifier);
        assert_eq!(found_identity, Some(service_identity));
    }

    #[test]
    fn find_identity_by_process_identifier_not_found() {
        let service_registry = ServiceRegistry::default();
        let process_identifier = ProcessIdentifier { pid: 1234, uid: 5678 };

        let found_identity =
            service_registry.find_identity_by_process_identifier(&process_identifier);
        assert_eq!(found_identity, None);
    }

    #[derive(Clone)]
    struct FakeServiceUnitChangeListener {
        num_on_service_unit_registered: Arc<Mutex<i32>>,
        num_on_service_unit_unregistered: Arc<Mutex<i32>>,
    }

    impl FakeServiceUnitChangeListener {
        fn new() -> Self {
            Self {
                num_on_service_unit_registered: Arc::new(Mutex::new(0)),
                num_on_service_unit_unregistered: Arc::new(Mutex::new(0)),
            }
        }

        fn wrap_with_authz(&self) -> ServiceUnitChangeListener {
            ServiceUnitChangeListener::new(
                // This filter function always authorizes access.
                Arc::new(|&_: &ServiceUnitDefinition| true),
                BnServiceUnitChangeListener::new_binder(
                    self.clone(),
                    binder::BinderFeatures::default(),
                ),
            )
        }

        fn get_num_on_service_unit_registered(&self) -> i32 {
            *self.num_on_service_unit_registered.lock().unwrap()
        }

        fn get_num_on_service_unit_unregistered(&self) -> i32 {
            *self.num_on_service_unit_unregistered.lock().unwrap()
        }
    }

    impl Interface for FakeServiceUnitChangeListener {}

    impl IServiceUnitChangeListener for FakeServiceUnitChangeListener {
        fn onServiceUnitRegistered(&self, _query: &ServiceUnitDefinition) -> binder::Result<()> {
            let mut num_calls = self.num_on_service_unit_registered.lock().unwrap();
            *num_calls += 1;
            Ok(())
        }

        fn onServiceUnitUnregistered(&self, _query: &ServiceUnitDefinition) -> binder::Result<()> {
            let mut num_calls = self.num_on_service_unit_unregistered.lock().unwrap();
            *num_calls += 1;
            Ok(())
        }

        fn onServiceUnitChanged(&self, _query: &ServiceUnitDefinition) -> binder::Result<()> {
            unimplemented!()
        }
    }

    #[test]
    fn register_unregister_listener_by_type_succeeds() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_listener_test_default_unit_type();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };

        let identity = register_service_unit_for_test(
            &service_registry,
            "some_vm_name",
            &unit_type,
            &caller_pid,
            123,
        );

        let listener = FakeServiceUnitChangeListener::new();
        let binder_status_listener = listener.wrap_with_authz();

        assert!(service_registry
            .register_service_unit_change_listener_by_type(
                &unit_type,
                binder_status_listener.clone()
            )
            .is_ok());

        assert_that!(listener.get_num_on_service_unit_registered(), eq(1));
        assert_that!(listener.get_num_on_service_unit_unregistered(), eq(0));

        let service_unit_id = ServiceUnitIdentifier {
            sid: identity.sid.into(),
            name: "service_unit_name".to_string(), // matches with register_service_unit_for_test
        };
        service_registry.remove_remote_service_unit(&service_unit_id);
        assert_that!(listener.get_num_on_service_unit_unregistered(), eq(1));

        assert!(service_registry
            .unregister_service_unit_change_listener_by_type(&unit_type, binder_status_listener)
            .is_ok());
    }

    #[test]
    fn register_unregister_listener_by_name_succeeds() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_listener_test_default_unit_type();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let unit_name_parameters = UnitNameParameters {
            sdvVmName: None,
            sdvPackageName: "package_name".to_string(),
            serviceBundleName: "ListenerServiceBundle".to_string(),
            unitName: None,
        };

        let listener = FakeServiceUnitChangeListener::new();
        let binder_status_listener = listener.wrap_with_authz();

        assert!(service_registry
            .register_service_unit_change_listener_by_name(
                &unit_name_parameters,
                binder_status_listener.clone()
            )
            .is_ok());

        let identity = register_service_unit_for_test(
            &service_registry,
            "some_vm_name",
            &unit_type,
            &caller_pid,
            123,
        );

        assert_that!(listener.get_num_on_service_unit_registered(), eq(1));
        assert_that!(listener.get_num_on_service_unit_unregistered(), eq(0));

        let service_unit_id = ServiceUnitIdentifier {
            sid: identity.sid.into(),
            name: "service_unit_name".to_string(), // matches with register_service_unit_for_test
        };
        service_registry.remove_remote_service_unit(&service_unit_id);

        assert_that!(listener.get_num_on_service_unit_unregistered(), eq(1));

        assert!(service_registry
            .unregister_service_unit_change_listener_by_name(
                &unit_name_parameters,
                binder_status_listener
            )
            .is_ok());
    }

    #[test]
    fn register_listener_by_name_notified_on_remove_identity() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_listener_test_default_unit_type();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let unit_name_parameters = UnitNameParameters {
            sdvVmName: None,
            sdvPackageName: "package_name".to_string(),
            serviceBundleName: "ListenerServiceBundle".to_string(),
            unitName: None,
        };

        let listener = FakeServiceUnitChangeListener::new();
        let binder_status_listener = listener.wrap_with_authz();

        assert!(service_registry
            .register_service_unit_change_listener_by_name(
                &unit_name_parameters,
                binder_status_listener.clone()
            )
            .is_ok());

        let identity = register_service_unit_for_test(
            &service_registry,
            "some_vm_name",
            &unit_type,
            &caller_pid,
            123,
        );

        assert_that!(listener.get_num_on_service_unit_unregistered(), eq(0));

        service_registry.remove_by_identity_v2(&identity);

        assert_that!(listener.get_num_on_service_unit_unregistered(), eq(1));

        assert!(service_registry
            .unregister_service_unit_change_listener_by_name(
                &unit_name_parameters,
                binder_status_listener
            )
            .is_ok());
    }

    #[test]
    fn register_listener_by_name_notified_on_remove_vm() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_listener_test_default_unit_type();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let unit_name_parameters = UnitNameParameters {
            sdvVmName: None,
            sdvPackageName: "package_name".to_string(),
            serviceBundleName: "ListenerServiceBundle".to_string(),
            unitName: None,
        };

        let listener = FakeServiceUnitChangeListener::new();
        let binder_status_listener = listener.wrap_with_authz();

        assert!(service_registry
            .register_service_unit_change_listener_by_name(
                &unit_name_parameters,
                binder_status_listener.clone()
            )
            .is_ok());

        let identity = register_service_unit_for_test(
            &service_registry,
            "some_vm_name",
            &unit_type,
            &caller_pid,
            123,
        );

        assert_that!(listener.get_num_on_service_unit_unregistered(), eq(0));

        service_registry.remove_by_vm_name(&identity.name.sdvVmName);

        assert_that!(listener.get_num_on_service_unit_unregistered(), eq(1));

        assert!(service_registry
            .unregister_service_unit_change_listener_by_name(
                &unit_name_parameters,
                binder_status_listener
            )
            .is_ok());
    }

    #[test]
    fn register_unregister_listener_by_channel_succeeds() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_listener_test_default_unit_type();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let channel_name = ChannelName::from_str("channel").unwrap();

        let identity = register_service_unit_for_test_channel(
            &service_registry,
            "some_vm_name",
            &unit_type,
            &caller_pid,
            123,
            Some(channel_name.to_string()),
        );

        let listener = FakeServiceUnitChangeListener::new();
        let binder_status_listener = listener.wrap_with_authz();

        // Register the listener.
        assert!(service_registry
            .register_service_unit_change_listener_by_channel(
                &unit_type,
                &channel_name,
                binder_status_listener.clone()
            )
            .is_ok());

        // Listener should be notified about the existing service unit.
        assert_that!(listener.get_num_on_service_unit_registered(), eq(1));
        assert_that!(listener.get_num_on_service_unit_unregistered(), eq(0));

        // Remove the service unit.
        let service_unit_id = ServiceUnitIdentifier {
            sid: identity.sid.into(),
            name: "service_unit_name".to_string(), // matches with register_service_unit_for_test
        };
        service_registry.remove_remote_service_unit(&service_unit_id);
        assert_that!(listener.get_num_on_service_unit_unregistered(), eq(1));

        // Unregister the listener.
        assert!(service_registry
            .unregister_service_unit_change_listener_by_channel(
                &unit_type,
                &channel_name,
                binder_status_listener
            )
            .is_ok());
    }

    #[test]
    fn listener_by_channel_not_notified_for_wrong_channel() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_listener_test_default_unit_type();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let channel_name = ChannelName::from_str("channel").unwrap();
        let wrong_channel_name = ChannelName::from_str("wrong-channel").unwrap();

        register_service_unit_for_test_channel(
            &service_registry,
            "some_vm_name",
            &unit_type,
            &caller_pid,
            123,
            Some(channel_name.to_string()),
        );

        let listener = FakeServiceUnitChangeListener::new();
        let binder_status_listener = listener.wrap_with_authz();

        // Register the listener for the correct channel name.
        assert!(service_registry
            .register_service_unit_change_listener_by_channel(
                &unit_type,
                &wrong_channel_name,
                binder_status_listener.clone()
            )
            .is_ok());

        // Listener should not be notified because the channel name doesn't match.
        assert_that!(listener.get_num_on_service_unit_registered(), eq(0));

        // Unregister the listener.
        assert!(service_registry
            .unregister_service_unit_change_listener_by_channel(
                &unit_type,
                &wrong_channel_name,
                binder_status_listener
            )
            .is_ok());
    }

    #[test]
    fn listener_by_channel_not_notified_for_no_publication_metadata() {
        let service_registry = ServiceRegistry::default();
        let unit_type = get_listener_test_default_unit_type();
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let channel_name = ChannelName::from_str("channel").unwrap();

        register_service_unit_for_test(
            &service_registry,
            "some_vm_name",
            &unit_type,
            &caller_pid,
            123,
        );

        let listener = FakeServiceUnitChangeListener::new();
        let binder_status_listener = listener.wrap_with_authz();

        // Register the listener for the correct channel name.
        assert!(service_registry
            .register_service_unit_change_listener_by_channel(
                &unit_type,
                &channel_name,
                binder_status_listener.clone()
            )
            .is_ok());

        // Listener should not be notified because the channel name doesn't match.
        assert_that!(listener.get_num_on_service_unit_registered(), eq(0));

        // Unregister the listener.
        assert!(service_registry
            .unregister_service_unit_change_listener_by_channel(
                &unit_type,
                &channel_name,
                binder_status_listener
            )
            .is_ok());
    }
}
