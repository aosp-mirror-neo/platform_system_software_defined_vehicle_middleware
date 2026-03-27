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

//! ACL Policy Model type.

use crate::parse_acl_policy_proto::parse_acl_policy_from_proto;

use std::collections::HashMap;
use std::collections::HashSet;

use sdv_authz_policy::Acl;
use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
use sdv_comms_id::names::TopicName;
use sdv_comms_id::names::{ServiceUnitName, UnitTypeName};
use sdv_comms_sd::discovery::UnitType;

use std::hash::Hash;

/// ACL Policy represents Access Control Policy for a Service Bundle.
///
/// ACL Policy contains required information to guard access to Unit Types
/// and Service Units implemented by that Service Bundle.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AclPolicyModel {
    service_units_acls: HashMap<ServiceUnitName, Vec<Acl>>,
    unit_types_acls: HashMap<UnitTypeName, Vec<Acl>>,

    pub(crate) publish_permissions: Permissions<TopicName>,
    pub(crate) subscribe_permissions: Permissions<TopicName>,
    pub(crate) serve_permissions: Permissions<ServiceUnitName>,
    pub(crate) call_permissions: Permissions<ServiceUnitName>,
    pub(crate) allow_read_all: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Permissions<T: Hash + Eq> {
    pub(crate) allowed_types: HashSet<String>,
    pub(crate) allowed_instances: HashMap<String, HashSet<T>>,
}

impl<T: Hash + Eq> Default for Permissions<T> {
    fn default() -> Self {
        Self { allowed_types: HashSet::new(), allowed_instances: HashMap::new() }
    }
}

impl<T: Hash + Eq> Permissions<T> {
    pub(crate) fn check(&self, unit_type: &UnitType, value: &T) -> bool {
        // Use UnitType::to_string() as a representation of UnitType to simplify handling of "subpackages".
        let unit_type_str = unit_type.to_string();

        self.allowed_types.contains(&unit_type_str)
            || self.allowed_instances.get(&unit_type_str).is_some_and(|set| set.contains(value))
    }

    pub(crate) fn add_permission(&mut self, model: PermissionModel<T>) {
        match model {
            PermissionModel::Type(unit_type) => {
                self.allowed_types.insert(unit_type.to_string());
            }
            PermissionModel::Instance(unit_type, instances) => {
                self.allowed_instances.entry(unit_type.to_string()).or_default().extend(instances);
            }
        }
    }
}

/// Validated model of declared permission.
pub(crate) enum PermissionModel<T: Hash + Eq> {
    Type(UnitType),
    Instance(UnitType, HashSet<T>),
}

impl AclPolicyModel {
    pub(crate) fn new(
        service_units_acls: HashMap<ServiceUnitName, Vec<Acl>>,
        unit_types_acls: HashMap<UnitTypeName, Vec<Acl>>,
        publish_permissions: Permissions<TopicName>,
        subscribe_permissions: Permissions<TopicName>,
        serve_permissions: Permissions<ServiceUnitName>,
        call_permissions: Permissions<ServiceUnitName>,
        allow_read_all: bool,
    ) -> AclPolicyModel {
        AclPolicyModel {
            service_units_acls,
            unit_types_acls,
            publish_permissions,
            subscribe_permissions,
            serve_permissions,
            call_permissions,
            allow_read_all,
        }
    }

    /// Returns whether AclPolicyModel contains ACLs for a given Service Unit Name.
    pub fn has_service_unit(&self, service_unit_name: &ServiceUnitName) -> bool {
        self.service_units_acls.contains_key(service_unit_name)
    }

    /// Returns whether AclPolicyModel contains ACLs for a given Unit Type Name.
    pub fn has_unit_type(&self, unit_type_name: &UnitTypeName) -> bool {
        self.unit_types_acls.contains_key(unit_type_name)
    }

    /// Returns parsed ACLs for the Service Unit.
    pub(crate) fn get_acls_by_service_unit(
        &self,
        service_unit_name: &ServiceUnitName,
    ) -> Option<&Vec<Acl>> {
        self.service_units_acls.get(service_unit_name)
    }

    /// Returns parsed ACLs for the Unit Type.
    pub(crate) fn get_acls_by_unit_type(&self, unit_type_name: &UnitTypeName) -> Option<&Vec<Acl>> {
        self.unit_types_acls.get(unit_type_name)
    }

    /// Returns whether there is permission to publishto the given topic.
    pub fn can_publish(&self, publication_type: &UnitType, topic: &TopicName) -> bool {
        self.publish_permissions.check(publication_type, topic)
    }

    /// Returns whether there is permission to subscribe to the given topic.
    pub fn can_subscribe(&self, publication_type: &UnitType, topic: &TopicName) -> bool {
        if self.allow_read_all {
            return true;
        }

        self.subscribe_permissions.check(publication_type, topic)
    }

    /// Returns whether there is permission to serve the given service unit.
    pub fn can_serve(&self, service: &UnitType, service_unit: &ServiceUnitName) -> bool {
        self.serve_permissions.check(service, service_unit)
    }

    /// Returns whether there is permission to call the given service unit.
    pub fn can_call(&self, service: &UnitType, service_unit: &ServiceUnitName) -> bool {
        if self.allow_read_all {
            return true;
        }

        self.call_permissions.check(service, service_unit)
    }
}

impl TryFrom<AclPolicyProto> for AclPolicyModel {
    type Error = anyhow::Error;

    fn try_from(acl_policy_proto: AclPolicyProto) -> Result<Self, Self::Error> {
        parse_acl_policy_from_proto(acl_policy_proto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use protobuf::text_format;

    #[test]
    fn test_has_service_unit_true() {
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            acls {
                name: "TirePressureSubs"
                include: "vm-cdc:*"
                include: "vm-telemetry:*"
                never_include: "vm-ivi:*"
            }
            service_units {
                name: "tire-pressure-publisher"
                acl_names: "TirePressureSubs"
            }
        "#,
        )
        .unwrap();

        let acl_policy = AclPolicyModel::try_from(proto).unwrap();

        assert!(acl_policy
            .has_service_unit(&ServiceUnitName::from_str("tire-pressure-publisher").unwrap()));
    }

    #[test]
    fn test_has_service_unit_false() {
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            acls {
                name: "TirePressureSubs"
                include: "vm-cdc:*"
            }
            service_units {
                name: "tire-pressure-publisher"
                acl_names: "TirePressureSubs"
            }
        "#,
        )
        .unwrap();

        let acl_policy = AclPolicyModel::try_from(proto).unwrap();

        assert!(!acl_policy
            .has_service_unit(&ServiceUnitName::from_str("temperature-publisher").unwrap()));
    }

    #[test]
    fn test_has_unit_type_true() {
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            acls {
                name: "TirePressurePubs"
                include: "vm-cdc:*"
                include: "vm-telemetry:*"
                never_include: "vm-ivi:*"
            }
            unit_types {
                name: "TirePressureTopic"
                acl_names: "TirePressurePubs"
            }
        "#,
        )
        .unwrap();

        let acl_policy = AclPolicyModel::try_from(proto).unwrap();

        assert!(acl_policy.has_unit_type(&UnitTypeName::from_str("TirePressureTopic").unwrap()));
    }

    #[test]
    fn test_has_unit_type_false() {
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            acls {
                name: "TirePressurePubs"
                include: "vm-cdc:*"
            }
            unit_types {
                name: "TirePressureTopic"
                acl_names: "TirePressurePubs"
            }
        "#,
        )
        .unwrap();

        let acl_policy = AclPolicyModel::try_from(proto).unwrap();

        assert!(!acl_policy.has_unit_type(&UnitTypeName::from_str("TemperatureTopic").unwrap()));
    }

    #[test]
    fn test_with_subpackage() {
        let acl_policy_proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            publisher {
                message: "test.package.subpackage.Message"
                topic: "topic"
            }
            subscriber {
                message: "test.package.subpackage.Message"
                topic: "topic"
            }
            server {
                service: "test.package.subpackage.Service"
                channel: "channel"
            }
            client {
                service: "test.package.subpackage.Service"
                channel: "channel"
            }
        "#,
        )
        .expect("Failed to parse policy");
        let policy = AclPolicyModel::try_from(acl_policy_proto).unwrap();

        let non_normalized_message = UnitType::builder()
            .package_name("test.package")
            .unit_type_name("subpackage.Message")
            .build()
            .unwrap();
        let non_normalized_service = UnitType::builder()
            .package_name("test.package")
            .unit_type_name("subpackage.Service")
            .build()
            .unwrap();

        let topic = TopicName::from_str("topic").unwrap();
        let channel = ServiceUnitName::from_str("channel").unwrap();

        assert!(policy.can_publish(&non_normalized_message, &topic));
        assert!(policy.can_subscribe(&non_normalized_message, &topic));
        assert!(policy.can_serve(&non_normalized_service, &channel));
        assert!(policy.can_call(&non_normalized_service, &channel));
    }

    #[test]
    fn test_permissions_logic() {
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            publisher {
                message: "test.package.TirePressureTopic"
                topic: "front-left"
            }
            publisher {
                message: "test.package.BrakeTopic"
                allow_all_topics: true
            }
            subscriber {
                message: "test.package.TemperatureTopic"
                topic: "cabin"
            }
            server {
                service: "test.package.HvacServer"
                channel: "hvac-controller"
            }
            server {
                service: "test.package.MediaServer"
                allow_all_channels: true
            }
            client {
                service: "test.package.NavServer"
                channel: "nav-system"
            }
        "#,
        )
        .unwrap();

        let acl_policy = AclPolicyModel::try_from(proto).unwrap();

        let tire_topic = UnitType::from_str("test.package.TirePressureTopic").unwrap();
        let brake_topic = UnitType::from_str("test.package.BrakeTopic").unwrap();
        let temp_topic = UnitType::from_str("test.package.TemperatureTopic").unwrap();
        let hvac_unit = ServiceUnitName::from_str("hvac-controller").unwrap();
        let hvac_server = UnitType::from_str("test.package.HvacServer").unwrap();
        let media_server = UnitType::from_str("test.package.MediaServer").unwrap();
        let any_media_unit = ServiceUnitName::from_str("any-media-unit").unwrap();
        let nav_unit = ServiceUnitName::from_str("nav-system").unwrap();
        let nav_server = UnitType::from_str("test.package.NavServer").unwrap();
        let front_left = TopicName::from_str("front-left").unwrap();
        let cabin = TopicName::from_str("cabin").unwrap();
        let rear_right = TopicName::from_str("rear-right").unwrap();
        let any_brake_topic = TopicName::from_str("any-brake-topic").unwrap();

        // 1. Test Positive Logic
        assert!(acl_policy.can_publish(&tire_topic, &front_left));
        assert!(acl_policy.can_subscribe(&temp_topic, &cabin));
        assert!(acl_policy.can_serve(&hvac_server, &hvac_unit));
        assert!(acl_policy.can_call(&nav_server, &nav_unit));

        // 2. Test Blanket Permissions
        assert!(acl_policy.can_publish(&brake_topic, &any_brake_topic));
        assert!(acl_policy.can_serve(&media_server, &any_media_unit));

        // 3. Test Negative Logic (Denial)
        assert!(!acl_policy.can_publish(&tire_topic, &rear_right));
        assert!(!acl_policy.can_subscribe(&tire_topic, &cabin));
        assert!(!acl_policy.can_serve(&hvac_server, &nav_unit));
        assert!(!acl_policy.can_call(&nav_server, &hvac_unit));
    }

    #[test]
    fn test_blanket_read_permission_logic() {
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            publisher {
                message: "test.package.TirePressureTopic"
                topic: "front-left"
            }
            server {
                service: "test.package.MediaServer"
                allow_all_channels: true
            }

            allow_read_all: true
        "#,
        )
        .unwrap();

        let acl_policy = AclPolicyModel::try_from(proto).unwrap();

        let tire_topic = UnitType::from_str("test.package.TirePressureTopic").unwrap();
        let media_server = UnitType::from_str("test.package.MediaServer").unwrap();
        let front_left = TopicName::from_str("front-left").unwrap();
        let any_service_name = ServiceUnitName::from_str("any-service-name").unwrap();
        let any_topic_name = TopicName::from_str("any-topic-name").unwrap();

        // 1. Test Positive Logic
        assert!(acl_policy.can_publish(&tire_topic, &front_left));
        assert!(acl_policy.can_serve(&media_server, &any_service_name));

        // 2. Test Blanket Permission
        assert!(acl_policy.can_subscribe(&tire_topic, &any_topic_name));
        assert!(acl_policy.can_call(&media_server, &any_service_name));

        // 3. Test Negative Logic (Denial)
        assert!(!acl_policy.can_serve(&tire_topic, &any_service_name));
        assert!(!acl_policy.can_publish(&tire_topic, &any_topic_name));
    }

    #[test]
    fn test_mixed_acls_and_permissions() {
        // This test ensures that the presence of permission fields does not interfere
        // with ACL parsing, and vice versa.
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            # 1. ACL Definitions
            acls {
                name: "MyAcl"
                include: "vm-cdc:*"
            }
            service_units {
                name: "my-service-unit"
                acl_names: "MyAcl"
            }

            # 2. Permission Definitions
            publisher {
                message: "test.package.MyTopic"
                topic: "ch1"
            }
            server {
                service: "test.package.MyServer"
                channel: "my-service-unit"
            }
        "#,
        )
        .unwrap();

        let acl_policy = AclPolicyModel::try_from(proto).unwrap();

        // Check ACL logic.
        assert!(acl_policy.has_service_unit(&ServiceUnitName::from_str("my-service-unit").unwrap()));

        let acl_vec = acl_policy
            .get_acls_by_service_unit(&ServiceUnitName::from_str("my-service-unit").unwrap())
            .unwrap();
        assert_eq!(acl_vec.len(), 1);

        // Check Permissions logic.
        assert!(acl_policy.can_publish(
            &UnitType::from_str("test.package.MyTopic").unwrap(),
            &TopicName::from_str("ch1").unwrap()
        ));
        assert!(acl_policy.can_serve(
            &UnitType::from_str("test.package.MyServer").unwrap(),
            &ServiceUnitName::from_str("my-service-unit").unwrap()
        ));
    }
}
