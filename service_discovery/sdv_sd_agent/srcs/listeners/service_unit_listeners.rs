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
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
    UnitType::UnitType,
    // comment to allow single element in {}
};
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::{
    ServiceUnitDefinition::ServiceUnitDefinition, UnitNameParameters::UnitNameParameters,
};
use log::warn;
use std::collections::HashMap;
use std::sync::Arc;

use crate::kv::{InternalUnitNameParameters, ServiceUnitEvent};
use crate::listeners::ServiceUnitChangeListener;
use crate::unit_type_utils::deprecate_unit_type_owner;
use tokio::sync::mpsc;

enum ListenerEventKind {
    Add,
    Remove,
}

#[derive(Default)]
pub(crate) struct Listeners {
    unit_type_listeners: HashMap<UnitType, Vec<ServiceUnitChangeListener>>,
    unit_name_listeners: HashMap<InternalUnitNameParameters, Vec<ServiceUnitChangeListener>>,
    local_unit_listeners: Vec<mpsc::UnboundedSender<ServiceUnitEvent>>,
}

impl Listeners {
    pub(crate) fn on_service_unit_added(&self, service_unit_definition: &ServiceUnitDefinition) {
        self.notify_by_type(service_unit_definition, ListenerEventKind::Add);
        self.notify_by_name(service_unit_definition, ListenerEventKind::Add);
    }

    pub(crate) fn on_service_unit_removed(&self, service_unit_definition: &ServiceUnitDefinition) {
        self.notify_by_type(service_unit_definition, ListenerEventKind::Remove);
        self.notify_by_name(service_unit_definition, ListenerEventKind::Remove);
    }

    fn notify_by_type(
        &self,
        service_unit_definition: &ServiceUnitDefinition,
        event: ListenerEventKind,
    ) {
        if let Some(listeners) = self.unit_type_listeners.get(&service_unit_definition.unitType) {
            for listener in listeners {
                let listener = Arc::new(listener.clone());
                match event {
                    ListenerEventKind::Add => {
                        if let Err(status) =
                            listener.on_service_unit_registered(service_unit_definition)
                        {
                            warn!("Listener fails at on_service_unit_registered {status}");
                        }
                    }
                    ListenerEventKind::Remove => {
                        if let Err(status) =
                            listener.on_service_unit_unregistered(service_unit_definition)
                        {
                            warn!("Listener fails at on_service_unit_unregistered {status}");
                        }
                    }
                }
            }
        }
    }

    fn notify_by_name(
        &self,
        service_unit_definition: &ServiceUnitDefinition,
        event: ListenerEventKind,
    ) {
        for (_, listeners) in self
            .unit_name_listeners
            .iter()
            .filter(|(query, _)| query.is_query_match(service_unit_definition))
        {
            for listener in listeners {
                let listener = Arc::new(listener.clone());
                match event {
                    ListenerEventKind::Add => {
                        if let Err(status) =
                            listener.on_service_unit_registered(service_unit_definition)
                        {
                            warn!("Listener fails at on_service_unit_registered {status}");
                        }
                    }
                    ListenerEventKind::Remove => {
                        if let Err(status) =
                            listener.on_service_unit_unregistered(service_unit_definition)
                        {
                            warn!("Listener fails at on_service_unit_unregistered {status}");
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn notify_local_unit_event(&mut self, service_unit_event: ServiceUnitEvent) {
        for listener in &self.local_unit_listeners {
            let _ = listener.send(service_unit_event.clone());
        }
        self.local_unit_listeners.retain(move |listener| !listener.is_closed());
    }

    pub(crate) fn add_local_unit_listener(
        &mut self,
        sender: mpsc::UnboundedSender<ServiceUnitEvent>,
    ) {
        self.local_unit_listeners.push(sender);
    }

    pub(crate) fn register_listener_by_type(
        &mut self,
        unit_type: &UnitType,
        listener: ServiceUnitChangeListener,
    ) {
        let unit_type = deprecate_unit_type_owner(unit_type.clone());
        self.unit_type_listeners.entry(unit_type).or_default().push(listener);
    }

    pub(crate) fn unregister_listener_by_type(
        &mut self,
        unit_type: &UnitType,
        listener: ServiceUnitChangeListener,
    ) {
        let unit_type: UnitType = deprecate_unit_type_owner(unit_type.clone());

        self.unit_type_listeners.entry(unit_type).or_default().retain(|x| *x != listener);
    }

    pub(crate) fn register_listener_by_name(
        &mut self,
        unit_name_params: &UnitNameParameters,
        listener: ServiceUnitChangeListener,
    ) {
        let unit_name_params = InternalUnitNameParameters(unit_name_params.clone());
        self.unit_name_listeners.entry(unit_name_params.clone()).or_default().push(listener);
    }

    pub(crate) fn unregister_listener_by_name(
        &mut self,
        unit_name_params: &UnitNameParameters,
        listener: ServiceUnitChangeListener,
    ) {
        let unit_name_params = InternalUnitNameParameters(unit_name_params.clone());
        self.unit_name_listeners
            .entry(unit_name_params.clone())
            .or_default()
            .retain(|x| *x != listener);
    }
}
