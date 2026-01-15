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
#[cfg(feature = "permissions")]
use std::collections::HashSet;

use sdv_authz_policy::Acl;
use sdv_authz_policy_proto_rust::acl_policy::AclPolicy as AclPolicyProto;
#[cfg(feature = "permissions")]
use sdv_comms_id::names::ChannelName;
use sdv_comms_id::names::{ServiceUnitName, UnitTypeName};

/// ACL Policy represents Access Control Policy for a Service Bundle.
///
/// ACL Policy contains required information to guard access to Unit Types
/// and Service Units implemented by that Service Bundle.
#[derive(Debug, Clone)]
pub struct AclPolicyModel {
    service_units_acls: HashMap<ServiceUnitName, Vec<Acl>>,
    unit_types_acls: HashMap<UnitTypeName, Vec<Acl>>,

    #[cfg(feature = "permissions")]
    pub(crate) publish_permissions: HashSet<(UnitTypeName, Option<ChannelName>)>,
    #[cfg(feature = "permissions")]
    pub(crate) subscribe_permissions: HashSet<(UnitTypeName, Option<ChannelName>)>,
    #[cfg(feature = "permissions")]
    pub(crate) serve_permissions: HashSet<(UnitTypeName, Option<ServiceUnitName>)>,
    #[cfg(feature = "permissions")]
    pub(crate) call_permissions: HashSet<(UnitTypeName, Option<ServiceUnitName>)>,
}

impl AclPolicyModel {
    pub(crate) fn new(
        service_units_acls: HashMap<ServiceUnitName, Vec<Acl>>,
        unit_types_acls: HashMap<UnitTypeName, Vec<Acl>>,
    ) -> AclPolicyModel {
        AclPolicyModel {
            service_units_acls,
            unit_types_acls,
            #[cfg(feature = "permissions")]
            publish_permissions: HashSet::new(),
            #[cfg(feature = "permissions")]
            subscribe_permissions: HashSet::new(),
            #[cfg(feature = "permissions")]
            serve_permissions: HashSet::new(),
            #[cfg(feature = "permissions")]
            call_permissions: HashSet::new(),
        }
    }

    #[cfg(feature = "permissions")]
    pub(crate) fn new_with_permissions(
        service_units_acls: HashMap<ServiceUnitName, Vec<Acl>>,
        unit_types_acls: HashMap<UnitTypeName, Vec<Acl>>,
        publish_permissions: HashSet<(UnitTypeName, Option<ChannelName>)>,
        subscribe_permissions: HashSet<(UnitTypeName, Option<ChannelName>)>,
        serve_permissions: HashSet<(UnitTypeName, Option<ServiceUnitName>)>,
        call_permissions: HashSet<(UnitTypeName, Option<ServiceUnitName>)>,
    ) -> AclPolicyModel {
        AclPolicyModel {
            service_units_acls,
            unit_types_acls,
            publish_permissions,
            subscribe_permissions,
            serve_permissions,
            call_permissions,
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

    #[cfg(feature = "permissions")]
    pub fn can_publish(&self, publication_type: &UnitTypeName, channel: &ChannelName) -> bool {
        self.publish_permissions.contains(&(publication_type.clone(), Some(channel.clone())))
            || self.publish_permissions.contains(&(publication_type.clone(), None))
    }

    #[cfg(feature = "permissions")]
    pub fn can_subscribe(&self, publication_type: &UnitTypeName, channel: &ChannelName) -> bool {
        self.subscribe_permissions.contains(&(publication_type.clone(), Some(channel.clone())))
            || self.subscribe_permissions.contains(&(publication_type.clone(), None))
    }

    #[cfg(feature = "permissions")]
    pub fn can_serve(&self, server_type: &UnitTypeName, service_unit: &ServiceUnitName) -> bool {
        self.serve_permissions.contains(&(server_type.clone(), Some(service_unit.clone())))
            || self.serve_permissions.contains(&(server_type.clone(), None))
    }

    #[cfg(feature = "permissions")]
    pub fn can_call(&self, server_type: &UnitTypeName, service_unit: &ServiceUnitName) -> bool {
        self.call_permissions.contains(&(server_type.clone(), Some(service_unit.clone())))
            || self.call_permissions.contains(&(server_type.clone(), None))
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

    #[cfg(feature = "permissions")]
    #[test]
    fn test_permissions_logic() {
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            publish {
                publication_type: "TirePressureTopic"
                channel_name: "front_left"
            }
            publish {
                publication_type: "BrakeTopic"
            }
            subscribe {
                publication_type: "TemperatureTopic"
                channel_name: "cabin"
            }
            serve {
                server_type: "HvacServer"
                service_unit_name: "hvac-controller"
            }
            serve {
                server_type: "MediaServer"
            }
            call {
                server_type: "NavServer"
                service_unit_name: "nav-system"
            }
        "#,
        )
        .unwrap();

        let acl_policy = AclPolicyModel::try_from(proto).unwrap();

        let tire_topic = UnitTypeName::from_str("TirePressureTopic").unwrap();
        let brake_topic = UnitTypeName::from_str("BrakeTopic").unwrap();
        let temp_topic = UnitTypeName::from_str("TemperatureTopic").unwrap();
        let hvac_unit = ServiceUnitName::from_str("hvac-controller").unwrap();
        let hvac_server = UnitTypeName::from_str("HvacServer").unwrap();
        let media_server = UnitTypeName::from_str("MediaServer").unwrap();
        let any_media_unit = ServiceUnitName::from_str("any-media-unit").unwrap();
        let nav_unit = ServiceUnitName::from_str("nav-system").unwrap();
        let nav_server = UnitTypeName::from_str("NavServer").unwrap();
        let front_left = ChannelName::from_str("front_left").unwrap();
        let cabin = ChannelName::from_str("cabin").unwrap();
        let rear_right = ChannelName::from_str("rear_right").unwrap();
        let any_brake_channel = ChannelName::from_str("any-brake-channel").unwrap();

        // 1. Test Positive Logic
        assert!(acl_policy.can_publish(&tire_topic, &front_left));
        assert!(acl_policy.can_subscribe(&temp_topic, &cabin));
        assert!(acl_policy.can_serve(&hvac_server, &hvac_unit));
        assert!(acl_policy.can_call(&nav_server, &nav_unit));

        // 2. Test Blanket Permissions
        assert!(acl_policy.can_publish(&brake_topic, &any_brake_channel));
        assert!(acl_policy.can_serve(&media_server, &any_media_unit));

        // 3. Test Negative Logic (Denial)
        assert!(!acl_policy.can_publish(&tire_topic, &rear_right));
        assert!(!acl_policy.can_subscribe(&hvac_server, &cabin));
        assert!(!acl_policy.can_serve(&hvac_server, &nav_unit));
        assert!(!acl_policy.can_call(&nav_server, &hvac_unit));
    }

    #[cfg(feature = "permissions")]
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
            publish {
                publication_type: "MyTopic"
                channel_name: "ch1"
            }
            serve {
                server_type: "MyServer"
                service_unit_name: "my-service-unit"
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
            &UnitTypeName::from_str("MyTopic").unwrap(),
            &ChannelName::from_str("ch1").unwrap()
        ));
        assert!(acl_policy.can_serve(
            &UnitTypeName::from_str("MyServer").unwrap(),
            &ServiceUnitName::from_str("my-service-unit").unwrap()
        ));
    }
}
