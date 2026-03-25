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

//! ACL Policy type parser from proto.

use crate::AclPolicyModel;

use std::collections::HashMap;
use std::collections::HashSet;
use std::str::FromStr;

use anyhow::bail;
use anyhow::{anyhow, Context};
use protobuf::MessageDyn;
use sdv_authz_policy::{Acl, AclName};
use sdv_authz_policy_proto_rust::authz_policy::{
    Acl as AclProto, AuthzPolicy as AclPolicyProto, UnitAclMapping as UnitAclMappingProto,
};

use sdv_authz_policy_proto_rust::authz_policy::{
    Client as CallPermissionProto, Publisher as PublishPermissionProto,
    Server as ServePermissionProto, Subscriber as SubscribePermissionProto,
};
use sdv_comms_id::names::TopicName;
use sdv_comms_id::names::{ServiceUnitName, UnitTypeName};
use sdv_comms_sd::discovery::UnitType;
use tracing::instrument;

/// Parses AclPolicyProto into an AclPolicyModel struct.
#[instrument(name = "authz_perf::service::parse_acl_policy_from_proto", skip_all, fields(size_bytes=acl_policy_proto.compute_size_dyn()))]
pub(crate) fn parse_acl_policy_from_proto(
    acl_policy_proto: AclPolicyProto,
) -> anyhow::Result<AclPolicyModel> {
    let acls_map = parse_acls(&acl_policy_proto.acls)?;

    let service_unit_acls_map = parse_service_units_acls_mapping(&acl_policy_proto, &acls_map)?;
    let unit_type_acls_map = parse_unit_types_acls_mapping(&acl_policy_proto, &acls_map)?;

    let publish_permissions = parse_publish_permissions(&acl_policy_proto.publisher)?;
    let subscribe_permissions = parse_subscribe_permissions(&acl_policy_proto.subscriber)?;
    let serve_permissions = parse_serve_permissions(&acl_policy_proto.server)?;
    let call_permissions = parse_call_permissions(&acl_policy_proto.client)?;

    Ok(AclPolicyModel::new(
        service_unit_acls_map,
        unit_type_acls_map,
        publish_permissions,
        subscribe_permissions,
        serve_permissions,
        call_permissions,
        acl_policy_proto.allow_read_all,
    ))
}

fn parse_publish_permissions(
    proto_perms: &[PublishPermissionProto],
) -> anyhow::Result<HashSet<(UnitType, Option<TopicName>)>> {
    let mut set = HashSet::new();
    for perm in proto_perms {
        let type_name = UnitType::from_str(&perm.message)
            .context("Failed to parse message in PublishPermission")?;

        for topic in &perm.topic {
            let topic =
                TopicName::from_str(topic).context("Failed to parse topic in PublishPermission")?;
            set.insert((type_name.clone(), Some(topic)));
        }

        if perm.allow_all_topics {
            if perm.topic.is_empty() {
                set.insert((type_name, None));
            } else {
                bail!("'allow_all_topics' == true, with non-empty 'topic' in PublishPermission");
            }
        } else if perm.topic.is_empty() {
            bail!(
                "'allow_all_topics' == false, with empty 'topic' in PublishPermission has no effect"
            );
        }
    }
    Ok(set)
}

fn parse_subscribe_permissions(
    proto_perms: &[SubscribePermissionProto],
) -> anyhow::Result<HashSet<(UnitType, Option<TopicName>)>> {
    let mut set = HashSet::new();
    for perm in proto_perms {
        let type_name = UnitType::from_str(&perm.message)
            .context("Failed to parse message in SubscribePermission")?;

        for topic in &perm.topic {
            let topic = TopicName::from_str(topic)
                .context("Failed to parse topic in SubscribePermission")?;
            set.insert((type_name.clone(), Some(topic)));
        }

        if perm.allow_all_topics {
            if perm.topic.is_empty() {
                set.insert((type_name, None));
            } else {
                bail!("'allow_all_topics' == true, with non-empty 'topic' in SubscribePermission");
            }
        } else if perm.topic.is_empty() {
            bail!("'allow_all_topics' == false, with empty 'topic' in SubscribePermission has no effect");
        }
    }
    Ok(set)
}

fn parse_serve_permissions(
    proto_perms: &[ServePermissionProto],
) -> Result<HashSet<(UnitType, Option<ServiceUnitName>)>, anyhow::Error> {
    let mut set = HashSet::new();
    for perm in proto_perms {
        let type_name = UnitType::from_str(&perm.service)
            .context("Failed to parse service in ServePermission")?;

        for channel in &perm.channel {
            let channel = ServiceUnitName::from_str(channel)
                .context("Failed to parse channel in ServePermission")?;
            set.insert((type_name.clone(), Some(channel)));
        }

        if perm.allow_all_channels {
            if perm.channel.is_empty() {
                set.insert((type_name, None));
            } else {
                bail!("'allow_all_channels' == true, with non-empty 'channel' in ServePermission");
            }
        } else if perm.channel.is_empty() {
            bail!("'allow_all_channels' == false, with empty 'channel' in ServePermission has no effect");
        }
    }
    Ok(set)
}

fn parse_call_permissions(
    proto_perms: &[CallPermissionProto],
) -> anyhow::Result<HashSet<(UnitType, Option<ServiceUnitName>)>> {
    let mut set = HashSet::new();
    for perm in proto_perms {
        let type_name = UnitType::from_str(&perm.service)
            .context("Failed to parse service in CallPermission")?;

        for channel in &perm.channel {
            let channel = ServiceUnitName::from_str(channel)
                .context("Failed to parse channel in CallPermission")?;
            set.insert((type_name.clone(), Some(channel)));
        }

        if perm.allow_all_channels {
            if perm.channel.is_empty() {
                set.insert((type_name, None));
            } else {
                bail!("'allow_all_channels' == true, with non-empty 'channel' in CallPermission");
            }
        } else if perm.channel.is_empty() {
            bail!("'allow_all_channels' == false, with empty 'channel' in CallPermission has no effect");
        }
    }
    Ok(set)
}

/// Parses a list of Service Unit entries from AclPolicyProto and returns a map
/// of Service Unit Name to ACL list.
fn parse_service_units_acls_mapping(
    acl_policy_proto: &AclPolicyProto,
    acls_map: &HashMap<AclName, Acl>,
) -> Result<HashMap<ServiceUnitName, Vec<Acl>>, anyhow::Error> {
    let service_units_map = parse_units_acl_mapping(&acl_policy_proto.service_units)
        .with_context(|| "Failed to parse Service Units ACL mapping")?;
    let mut service_unit_acls_map: HashMap<ServiceUnitName, Vec<Acl>> = HashMap::new();
    for (service_unit_name, acl_names) in service_units_map {
        let service_unit_name = ServiceUnitName::from_str(&service_unit_name)
            .with_context(|| "Failed to validate service unit name")?;

        let mut acls = Vec::new();
        for acl_name in acl_names {
            match acls_map.get(&acl_name) {
                Some(static_acl) => {
                    acls.push(static_acl.clone());
                }
                None => {
                    return Err(anyhow!(
                        "ACL {:?} required for {:?} is not found",
                        acl_name,
                        service_unit_name
                    ));
                }
            }
        }
        service_unit_acls_map.insert(service_unit_name, acls);
    }

    Ok(service_unit_acls_map)
}

/// Parses a list of Unit Types entries from AclPolicyProto and returns a map
/// of Unit Type Name to ACL list.
fn parse_unit_types_acls_mapping(
    acl_policy_proto: &AclPolicyProto,
    acls_map: &HashMap<AclName, Acl>,
) -> Result<HashMap<UnitTypeName, Vec<Acl>>, anyhow::Error> {
    let unit_types_map = parse_units_acl_mapping(&acl_policy_proto.unit_types)
        .with_context(|| "Failed to parse Unit Types ACL mapping")?;
    let mut unit_type_acls_map = HashMap::new();
    for (unit_type_name, acl_names) in unit_types_map {
        let unit_type_name = UnitTypeName::from_str(&unit_type_name)
            .with_context(|| "Failed to validate unit type name")?;

        let mut acls = Vec::new();
        for acl_name in acl_names {
            match acls_map.get(&acl_name) {
                Some(static_acl) => {
                    acls.push(static_acl.clone());
                }
                None => {
                    return Err(anyhow!(
                        "ACL {:?} required for {:?} is not found",
                        acl_name,
                        unit_type_name
                    ));
                }
            }
        }
        unit_type_acls_map.insert(unit_type_name, acls);
    }

    Ok(unit_type_acls_map)
}

/// Parses a list of AclProto into a map of AclName to Acl.
fn parse_acls(acls: &[AclProto]) -> Result<HashMap<AclName, Acl>, anyhow::Error> {
    let mut acls_map = HashMap::new();
    for acl in acls {
        let acl_name = AclName(acl.name.clone());
        if acls_map.contains_key(&acl_name) {
            return Err(anyhow!("ACL policy contains ACLs with the same name {:?}", acl_name));
        }

        let acl = Acl::new(&acl_name, &acl.include, &acl.never_include)
            .with_context(|| format!("Failed to parse ACL {acl_name:?}"))?;

        acls_map.insert(acl_name, acl);
    }
    Ok(acls_map)
}

/// Parses a list of UnitAclMappingProto into a map of either:
/// - Unit Type Name to AclName list or
/// - Service Unit Name to AclName list.
fn parse_units_acl_mapping(
    unit_name_acl_mapping: &[UnitAclMappingProto],
) -> Result<HashMap<String, Vec<AclName>>, anyhow::Error> {
    let mut unit_acls_map = HashMap::new();
    for unit_name_acl_entry in unit_name_acl_mapping {
        let unit_name = unit_name_acl_entry.name.clone();
        if unit_acls_map.contains_key(&unit_name) {
            return Err(anyhow!(
                "ACL policy contains mapping entries with the same name {:?}",
                unit_name
            ));
        }

        let acl_names = unit_name_acl_entry
            .acl_names
            .iter()
            .map(|acl_name| AclName(acl_name.clone()))
            .collect();

        unit_acls_map.insert(unit_name, acl_names);
    }

    Ok(unit_acls_map)
}

#[cfg(test)]
mod tests {
    use super::*;

    use protobuf::text_format;

    #[test]
    fn test_from_textproto_success() {
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            acls {
                name: "TirePressureAcl1"
                include: "vm-cdc:*"
                include: "vm-telemetry:*"
                never_include: "vm-ivi:*"
            }
            acls {
                name: "TirePressureAcl2"
                include: "com.google.android.sdv.datatunnel.*"
            }
            service_units {
                name: "tire-pressure-publisher-1"
                acl_names: "TirePressureAcl2"
            }
            service_units {
                name: "tire-pressure-publisher-2"
                acl_names: "TirePressureAcl1"
                acl_names: "TirePressureAcl2"
            }
            unit_types {
                name: "TirePressureTopic1"
                acl_names: "TirePressureAcl1"
            }
            unit_types {
                name: "TirePressureTopic2"
                acl_names: "TirePressureAcl1"
                acl_names: "TirePressureAcl2"
            }
        "#,
        )
        .unwrap();

        let acl_policy = parse_acl_policy_from_proto(proto).unwrap();

        assert!(acl_policy
            .has_service_unit(&ServiceUnitName::from_str("tire-pressure-publisher-1").unwrap()));
        assert!(acl_policy
            .has_service_unit(&ServiceUnitName::from_str("tire-pressure-publisher-2").unwrap()));

        assert!(acl_policy.has_unit_type(&UnitTypeName::from_str("TirePressureTopic1").unwrap()));
        assert!(acl_policy.has_unit_type(&UnitTypeName::from_str("TirePressureTopic2").unwrap()));
    }

    #[test]
    fn test_parse_permissions_population() {
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            publisher {
                message: "test.package.TopicA"
                topic: "ch1"
            }
            publisher {
                message: "test.package.TopicA"
                allow_all_topics: true
            }
            subscriber {
                message: "test.package.TopicB"
                topic: "ch1"
            }
            subscriber {
                message: "test.package.TopicC"
                allow_all_topics: true
            }
            client {
                service: "test.package.TopicB"
                channel: "sun1"
            }
            client {
                service: "test.package.TopicC"
                allow_all_channels: true
            }
            server {
                service: "test.package.TopicB"
                channel: "sun1"
            }
            server {
                service: "test.package.TopicC"
                allow_all_channels: true
            }
        "#,
        )
        .unwrap();

        let acl_policy = parse_acl_policy_from_proto(proto).unwrap();

        let topic_a = UnitType::from_str("test.package.TopicA").unwrap();
        let topic_b = UnitType::from_str("test.package.TopicB").unwrap();
        let topic_c = UnitType::from_str("test.package.TopicC").unwrap();
        let ch1 = TopicName::from_str("ch1").unwrap();
        let sun1 = ServiceUnitName::from_str("sun1").unwrap();

        assert_eq!(acl_policy.publish_permissions.len(), 2);
        assert!(acl_policy.publish_permissions.contains(&(topic_a.clone(), Some(ch1.clone()))));
        assert!(acl_policy.publish_permissions.contains(&(topic_a, None)));
        assert_eq!(acl_policy.subscribe_permissions.len(), 2);
        assert!(acl_policy.subscribe_permissions.contains(&(topic_b.clone(), Some(ch1))));
        assert!(acl_policy.subscribe_permissions.contains(&(topic_c.clone(), None)));
        assert_eq!(acl_policy.serve_permissions.len(), 2);
        assert!(acl_policy.serve_permissions.contains(&(topic_b.clone(), Some(sun1.clone()))));
        assert!(acl_policy.serve_permissions.contains(&(topic_c.clone(), None)));
        assert_eq!(acl_policy.call_permissions.len(), 2);
        assert!(acl_policy.call_permissions.contains(&(topic_b, Some(sun1))));
        assert!(acl_policy.call_permissions.contains(&(topic_c, None)));
        assert!(!acl_policy.allow_read_all);
    }

    #[test]
    fn test_from_textproto_failure_invalid_acl_statement() {
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            acls {
                name: "TirePressureSubs"
                include: "vm-cdc:*"
                include: "vm-telemetry:*"
                never_include: "*"
            }
            service_units {
                name: "tire-pressure-publisher"
                acl_names: "TirePressureSubs"
            }
        "#,
        )
        .unwrap();

        let err = parse_acl_policy_from_proto(proto).unwrap_err();
        assert_eq!(err.to_string(), r#"Failed to parse ACL AclName("TirePressureSubs")"#);
    }

    #[test]
    fn test_from_textproto_failure_duplicate_acl() {
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            acls {
                name: "TirePressureSubs"
                include: "vm-cdc:*"
                include: "vm-telemetry:*"
                never_include: "vm-ivi:*"
            }
            acls {
                name: "TirePressureSubs"
                include: "com.google.android.sdv.datatunnel.*"
            }
            service_units {
                name: "TirePressurePublisher"
                acl_names: "TirePressureSubs"
            }
        "#,
        )
        .unwrap();

        let err = parse_acl_policy_from_proto(proto).unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"ACL policy contains ACLs with the same name AclName("TirePressureSubs")"#
        );
    }

    #[test]
    fn test_from_textproto_failure_duplicate_service_unit() {
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            acls {
                name: "TirePressureSubs"
                include: "vm-cdc:*"
                include: "vm-telemetry:*"
                never_include: "vm-ivi:*"
            }
            service_units {
                name: "TirePressurePublisher"
                acl_names: "TirePressureSubs"
            }
            service_units {
                name: "TirePressurePublisher"
                acl_names: "TirePressureSubs"
            }
        "#,
        )
        .unwrap();

        let err = parse_acl_policy_from_proto(proto).unwrap_err();
        assert_eq!(err.to_string(), r#"Failed to parse Service Units ACL mapping"#);
        assert_eq!(
            err.root_cause().to_string(),
            r#"ACL policy contains mapping entries with the same name "TirePressurePublisher""#
        );
    }

    #[test]
    fn test_from_textproto_failure_duplicate_unit_type() {
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
            unit_types {
                name: "TirePressureTopic"
                acl_names: "TirePressurePubs"
            }
        "#,
        )
        .unwrap();

        let err = parse_acl_policy_from_proto(proto).unwrap_err();
        assert_eq!(err.to_string(), r#"Failed to parse Unit Types ACL mapping"#);
        assert_eq!(
            err.root_cause().to_string(),
            r#"ACL policy contains mapping entries with the same name "TirePressureTopic""#
        );
    }

    #[test]
    fn test_from_textproto_failure_service_unit_acl_not_found() {
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            service_units {
                name: "tire-pressure-publisher"
                acl_names: "NonExistentTirePressureSubs"
            }
        "#,
        )
        .unwrap();

        let err = parse_acl_policy_from_proto(proto).unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"ACL AclName("NonExistentTirePressureSubs") required for ServiceUnitName("tire-pressure-publisher") is not found"#
        );
    }

    #[test]
    fn test_from_textproto_failure_unit_type_acl_not_found() {
        let proto: AclPolicyProto = text_format::parse_from_str(
            r#"
            unit_types {
                name: "TirePressureTopic"
                acl_names: "NonExistentTirePressurePubs"
            }
        "#,
        )
        .unwrap();

        let err = parse_acl_policy_from_proto(proto).unwrap_err();
        assert_eq!(
            err.to_string(),
            r#"ACL AclName("NonExistentTirePressurePubs") required for UnitTypeName("TirePressureTopic") is not found"#
        );
    }
}
