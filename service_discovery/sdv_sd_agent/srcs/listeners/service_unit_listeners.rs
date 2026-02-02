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
    metadata::ApplicationMetadata::TypedMetadata::TypedMetadata as TypedMetadataAidl,
    UnitType::UnitType,
};
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::{
    ServiceUnitDefinition::ServiceUnitDefinition, UnitNameParameters::UnitNameParameters,
};

use log::warn;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use crate::kv::{InternalUnitNameParameters, ServiceUnitEvent};
use crate::listeners::ServiceUnitChangeListener;
use crate::unit_type_utils::deprecate_unit_type_owner;
use sdv_comms_id::names::TopicName;
use tokio::sync::mpsc;

enum ListenerEventKind {
    Add,
    Remove,
}

#[derive(Default)]
pub(crate) struct Listeners {
    unit_type_listeners: HashMap<UnitType, Vec<ServiceUnitChangeListener>>,
    topic_listeners: HashMap<(UnitType, TopicName), Vec<ServiceUnitChangeListener>>,
    unit_name_listeners: HashMap<InternalUnitNameParameters, Vec<ServiceUnitChangeListener>>,
    local_unit_listeners: Vec<mpsc::UnboundedSender<ServiceUnitEvent>>,
}

impl Listeners {
    pub(crate) fn on_service_unit_added(&self, service_unit_definition: &ServiceUnitDefinition) {
        self.notify_by_type(service_unit_definition, ListenerEventKind::Add);
        self.notify_by_name(service_unit_definition, ListenerEventKind::Add);
        self.notify_by_topic(service_unit_definition, ListenerEventKind::Add);
    }

    pub(crate) fn on_service_unit_removed(&self, service_unit_definition: &ServiceUnitDefinition) {
        self.notify_by_type(service_unit_definition, ListenerEventKind::Remove);
        self.notify_by_name(service_unit_definition, ListenerEventKind::Remove);
        self.notify_by_topic(service_unit_definition, ListenerEventKind::Remove);
    }

    fn notify_listeners(
        event: &ListenerEventKind,
        service_unit_definition: &ServiceUnitDefinition,
        listeners: &Vec<ServiceUnitChangeListener>,
    ) {
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

    fn notify_by_type(
        &self,
        service_unit_definition: &ServiceUnitDefinition,
        event: ListenerEventKind,
    ) {
        let unit_type: UnitType =
            deprecate_unit_type_owner(service_unit_definition.unitType.clone());

        // Notify general unit type listeners
        if let Some(listeners) = self.unit_type_listeners.get(&unit_type) {
            Self::notify_listeners(&event, service_unit_definition, listeners);
        }
    }

    fn notify_by_topic(
        &self,
        service_unit_definition: &ServiceUnitDefinition,
        event: ListenerEventKind,
    ) {
        let unit_type: UnitType =
            deprecate_unit_type_owner(service_unit_definition.unitType.clone());
        if let TypedMetadataAidl::PublisherUnit(metadata) =
            &service_unit_definition.appMetadata.typedMetadata
        {
            let topic_name = match TopicName::from_str(&metadata.topic_name) {
                Ok(name) => name,
                Err(err) => {
                    warn!("Invalid topic name: {} due to: {}", metadata.topic_name, err);
                    return;
                }
            };

            let key = (unit_type.clone(), topic_name);
            if let Some(listeners) = self.topic_listeners.get(&key) {
                Self::notify_listeners(&event, service_unit_definition, listeners);
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
            Self::notify_listeners(&event, service_unit_definition, listeners);
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

    pub(crate) fn register_listener_by_topic(
        &mut self,
        unit_type: &UnitType,
        topic_name: &TopicName,
        listener: ServiceUnitChangeListener,
    ) {
        let unit_type = deprecate_unit_type_owner(unit_type.clone());
        self.topic_listeners.entry((unit_type, topic_name.clone())).or_default().push(listener);
    }

    pub(crate) fn unregister_listener_by_topic(
        &mut self,
        unit_type: &UnitType,
        topic_name: &TopicName,
        listener: ServiceUnitChangeListener,
    ) {
        let unit_type: UnitType = deprecate_unit_type_owner(unit_type.clone());
        self.topic_listeners
            .entry((unit_type, topic_name.clone()))
            .or_default()
            .retain(|x| *x != listener);
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

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::kv::ServiceUnitAndTransport;
    use crate::listeners::service_unit_listeners::ServiceUnitEvent;
    use google_sdv_identity::aidl::google::sdv::identity::{
        ServiceFqin::ServiceFqin as AidlServiceFqin,
        ServiceIdentity::ServiceIdentity as AidlServiceIdentity,
    };
    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
        metadata::{
            ApplicationMetadata::ApplicationMetadata,
            ApplicationMetadata::TypedMetadata::TypedMetadata as TypedMetadataAidl,
            PublisherUnitMetadata::PublisherUnitMetadata,
        },
        UnitType::UnitType,
    };
    use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::{
        IServiceUnitChangeListener::{BnServiceUnitChangeListener, IServiceUnitChangeListener},
        ServiceUnitDefinition::ServiceUnitDefinition,
        UnitNameParameters::UnitNameParameters,
    };
    use sdv_comms_id::names::TopicName;
    #[allow(unused_imports)]
    use std::{
        collections::HashSet,
        str::FromStr,
        sync::{Arc, Mutex},
    };
    use tokio::sync::mpsc;

    #[derive(Clone)]
    struct MockServiceUnitChangeListener {
        registered: Arc<Mutex<Vec<ServiceUnitDefinition>>>,
        unregistered: Arc<Mutex<Vec<ServiceUnitDefinition>>>,
    }

    impl MockServiceUnitChangeListener {
        fn new() -> Self {
            Self {
                registered: Arc::new(Mutex::new(Vec::new())),
                unregistered: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn get_registered(&self) -> Vec<ServiceUnitDefinition> {
            self.registered.lock().unwrap().clone()
        }

        fn get_unregistered(&self) -> Vec<ServiceUnitDefinition> {
            self.unregistered.lock().unwrap().clone()
        }
    }

    impl binder::Interface for MockServiceUnitChangeListener {}

    impl IServiceUnitChangeListener for MockServiceUnitChangeListener {
        fn onServiceUnitRegistered(&self, sud: &ServiceUnitDefinition) -> binder::Result<()> {
            self.registered.lock().unwrap().push(sud.clone());
            Ok(())
        }

        fn onServiceUnitUnregistered(&self, sud: &ServiceUnitDefinition) -> binder::Result<()> {
            self.unregistered.lock().unwrap().push(sud.clone());
            Ok(())
        }

        fn onServiceUnitChanged(&self, _sud: &ServiceUnitDefinition) -> binder::Result<()> {
            unimplemented!()
        }
    }

    fn create_listener() -> (ServiceUnitChangeListener, Arc<MockServiceUnitChangeListener>) {
        let mock = MockServiceUnitChangeListener::new();
        let binder_listener = BnServiceUnitChangeListener::new_binder(
            mock.clone(),
            binder::BinderFeatures::default(),
        );
        let listener = ServiceUnitChangeListener::new(
            Arc::new(|_: &ServiceUnitDefinition| true),
            binder_listener,
        );
        (listener, mock.into())
    }

    fn create_sud(unit_name: &str, unit_type: &UnitType) -> ServiceUnitDefinition {
        ServiceUnitDefinition {
            identity: AidlServiceIdentity {
                name: AidlServiceFqin {
                    sdvVmName: "test_vm".to_string(),
                    sdvPackageName: unit_type.sdvPackageName.clone(),
                    serviceBundleName: unit_type.serviceBundleName.clone(),
                    serviceInstanceName: "instance".to_string(),
                },
                ..Default::default()
            },
            serviceUnitName: unit_name.to_string(),
            unitType: unit_type.clone(),
            ..Default::default()
        }
    }

    fn create_publisher_sud(
        unit_name: &str,
        unit_type: &UnitType,
        topic_name: &str,
    ) -> ServiceUnitDefinition {
        let mut sud = create_sud(unit_name, unit_type);
        sud.appMetadata = ApplicationMetadata {
            typedMetadata: TypedMetadataAidl::PublisherUnit(PublisherUnitMetadata {
                topic_name: topic_name.to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        sud
    }

    #[test]
    fn test_register_unregister_listener_by_type() {
        let mut listeners = Listeners::default();
        let (listener, mock) = create_listener();
        let unit_type = UnitType {
            sdvPackageName: "com.example".to_string(),
            serviceBundleName: "TestBundle".to_string(),
            typeName: "TestType".to_string(),
        };

        listeners.register_listener_by_type(&unit_type, listener.clone());

        let sud = create_sud("unit1", &unit_type);
        listeners.on_service_unit_added(&sud);
        assert_eq!(mock.get_registered(), vec![sud.clone()]);
        assert!(mock.get_unregistered().is_empty());

        listeners.on_service_unit_removed(&sud);
        assert_eq!(mock.get_registered(), vec![sud.clone()]);
        assert_eq!(mock.get_unregistered(), vec![sud.clone()]);

        listeners.unregister_listener_by_type(&unit_type, listener);
        let sud2 = create_sud("unit2", &unit_type);
        listeners.on_service_unit_added(&sud2);
        // No new registered event after unregistering.
        assert_eq!(mock.get_registered(), vec![sud.clone()]);
    }

    #[test]
    fn test_register_unregister_listener_by_topic() {
        let mut listeners = Listeners::default();
        let (listener, mock) = create_listener();
        let unit_type = UnitType {
            sdvPackageName: "com.example".to_string(),
            serviceBundleName: "TestBundle".to_string(),
            typeName: "TestType".to_string(),
        };
        let topic_name = TopicName::from_str("test_topic").unwrap();

        listeners.register_listener_by_topic(&unit_type, &topic_name, listener.clone());

        let sud = create_publisher_sud("unit1", &unit_type, "test_topic");
        listeners.on_service_unit_added(&sud);
        println!("registered");
        assert_eq!(mock.get_registered(), vec![sud.clone()]);

        let sud_wrong_topic = create_publisher_sud("unit2", &unit_type, "wrong-topic");
        listeners.on_service_unit_added(&sud_wrong_topic);
        assert_eq!(mock.get_registered(), vec![sud.clone()]); // Not notified for wrong topic.

        let sud_server = create_sud("unit3", &unit_type); // Not a publisher.
        listeners.on_service_unit_added(&sud_server);
        assert_eq!(mock.get_registered(), vec![sud.clone()]); // Not notified for non-publisher.

        listeners.unregister_listener_by_topic(&unit_type, &topic_name, listener);
        listeners.on_service_unit_added(&sud);
        assert_eq!(mock.get_registered(), vec![sud.clone()]); // Not notified after unregister.
    }

    #[test]
    fn test_register_unregister_listener_by_name() {
        let mut listeners = Listeners::default();
        let (listener, mock) = create_listener();
        let unit_type = UnitType {
            sdvPackageName: "com.example".to_string(),
            serviceBundleName: "TestBundle".to_string(),
            typeName: "TestType".to_string(),
        };
        let params = UnitNameParameters {
            sdvVmName: None,
            sdvPackageName: "com.example".to_string(),
            serviceBundleName: "TestBundle".to_string(),
            unitName: Some("unit1".to_string()),
        };

        listeners.register_listener_by_name(&params, listener.clone());

        let sud1 = create_sud("unit1", &unit_type);
        listeners.on_service_unit_added(&sud1);
        assert_eq!(mock.get_registered(), vec![sud1.clone()]);

        let sud2 = create_sud("unit2", &unit_type);
        listeners.on_service_unit_added(&sud2);
        assert_eq!(mock.get_registered(), vec![sud1.clone()]); // Not notified for unit2.

        listeners.unregister_listener_by_name(&params, listener);
        listeners.on_service_unit_added(&sud1);
        assert_eq!(mock.get_registered(), vec![sud1.clone()]); // Not notified after unregister.
    }

    #[test]
    fn test_notify_local_unit_event() {
        let mut listeners = Listeners::default();
        let (tx1, mut rx1) = mpsc::unbounded_channel();
        let (tx2, mut rx2) = mpsc::unbounded_channel();

        listeners.add_local_unit_listener(tx1);
        listeners.add_local_unit_listener(tx2);

        let sud = create_sud("unit1", &UnitType::default());
        let event = ServiceUnitEvent::RegisterServiceUnit(ServiceUnitAndTransport::new(
            sud.clone(),
            Default::default(),
        ));

        listeners.notify_local_unit_event(event.clone());

        assert!(
            matches!(rx1.try_recv(), Ok(e) if matches!(&e, ServiceUnitEvent::RegisterServiceUnit(sut) if sut.get_service_unit_definition() == &sud))
        );
        assert!(
            matches!(rx2.try_recv(), Ok(e) if matches!(&e, ServiceUnitEvent::RegisterServiceUnit(sut) if sut.get_service_unit_definition() == &sud))
        );

        drop(rx1); // Close one receiver.
        listeners.notify_local_unit_event(event);
        assert_eq!(listeners.local_unit_listeners.len(), 1);
    }
}
