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

//! VM Authz Policy Model type.

use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::hash::Hash;
use std::str::FromStr;

use anyhow::{bail, Result};
use sdv_authz_policy_proto_rust::vm_authz_policy::{
    Client, Publisher, Server, Subscriber, VmAuthzPolicy,
};
use sdv_comms_id::names::{PackageName, ServiceUnitName, TopicName};
use sdv_comms_sd::discovery::UnitType;

/// VM Authz Policy represents Access Control Policy for a remote VM.
#[derive(Debug, Clone, PartialEq)]
pub struct VmAuthzPolicyModel {
    publish: Permissions<TopicName>,
    subscribe: Permissions<TopicName>,
    serve: Permissions<ServiceUnitName>,
    call: Permissions<ServiceUnitName>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Permissions<T: Hash + Eq> {
    unit_type_permissions: HashMap<UnitType, PermissionScope<T>>,
    package_permissions: HashSet<PackageName>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PermissionScope<T: Hash + Eq> {
    Specific(HashSet<T>),
    All,
}

impl<T: Hash + Eq> PermissionScope<T> {
    fn check(&self, value: &T) -> bool {
        match self {
            PermissionScope::All => true,
            PermissionScope::Specific(set) => set.contains(value),
        }
    }
}

#[allow(dead_code)]
impl VmAuthzPolicyModel {
    /// Returns whether there is permission to publish to the given topic.
    pub fn can_publish(&self, publication_type: &UnitType, topic: &TopicName) -> bool {
        self.publish.check(publication_type, topic)
    }

    /// Returns whether there is permission to subscribe to the given topic.
    pub fn can_subscribe(&self, publication_type: &UnitType, topic: &TopicName) -> bool {
        self.subscribe.check(publication_type, topic)
    }

    /// Returns whether there is permission to serve the given service unit.
    pub fn can_serve(&self, service: &UnitType, service_unit: &ServiceUnitName) -> bool {
        self.serve.check(service, service_unit)
    }

    /// Returns whether there is permission to call the given service unit.
    pub fn can_call(&self, service: &UnitType, service_unit: &ServiceUnitName) -> bool {
        self.call.check(service, service_unit)
    }

    fn has_package_permission(permissions: &HashSet<PackageName>, package: &PackageName) -> bool {
        if permissions.contains(package) {
            return true;
        }
        let mut package_str = package.name();
        while let Some((left, _)) = package_str.rsplit_once('.') {
            if let Ok(pkg_name) = PackageName::from_str(left) {
                if permissions.contains(&pkg_name) {
                    return true;
                }
            }
            package_str = left;
        }

        false
    }
}

impl<T: Hash + Eq> From<(HashMap<UnitType, PermissionScope<T>>, HashSet<PackageName>)>
    for Permissions<T>
{
    fn from(tuple: (HashMap<UnitType, PermissionScope<T>>, HashSet<PackageName>)) -> Self {
        Permissions { unit_type_permissions: tuple.0, package_permissions: tuple.1 }
    }
}

impl<T: Hash + Eq> Permissions<T> {
    fn check(&self, unit_type: &UnitType, value: &T) -> bool {
        self.unit_type_permissions.get(unit_type).is_some_and(|scope| scope.check(value))
            || VmAuthzPolicyModel::has_package_permission(
                &self.package_permissions,
                unit_type.package_name(),
            )
    }
}

impl TryFrom<VmAuthzPolicy> for VmAuthzPolicyModel {
    type Error = anyhow::Error;

    fn try_from(proto: VmAuthzPolicy) -> Result<Self, Self::Error> {
        let publish = Self::parse_permissions(proto.publisher)?;
        let subscribe = Self::parse_permissions(proto.subscriber)?;
        let serve = Self::parse_permissions(proto.server)?;
        let call = Self::parse_permissions(proto.client)?;

        Ok(VmAuthzPolicyModel { publish, subscribe, serve, call })
    }
}

/// Trait to be implemented by each .proto-derived permission struct.
trait DeclaredPermission {
    fn to_parsed_permission(self) -> ParsedPermission;
}

impl DeclaredPermission for Publisher {
    fn to_parsed_permission(self) -> ParsedPermission {
        ParsedPermission {
            package: self.package,
            unit_type: self.message,
            instance: self.topic,
            allow_all: self.allow_all_topics,
        }
    }
}

impl DeclaredPermission for Subscriber {
    fn to_parsed_permission(self) -> ParsedPermission {
        ParsedPermission {
            package: self.package,
            unit_type: self.message,
            instance: self.topic,
            allow_all: self.allow_all_topics,
        }
    }
}

impl DeclaredPermission for Server {
    fn to_parsed_permission(self) -> ParsedPermission {
        ParsedPermission {
            package: self.package,
            unit_type: self.service,
            instance: self.channel,
            allow_all: self.allow_all_channels,
        }
    }
}

impl DeclaredPermission for Client {
    fn to_parsed_permission(self) -> ParsedPermission {
        ParsedPermission {
            package: self.package,
            unit_type: self.service,
            instance: self.channel,
            allow_all: self.allow_all_channels,
        }
    }
}

/// Common representation of a .proto-derived permission struct.
///
/// Enables to convert .proto-derived permission structs to PermissionModel in a generic way.
struct ParsedPermission {
    package: Option<String>,
    unit_type: Option<String>,
    instance: Vec<String>,
    allow_all: Option<bool>,
}

/// Representation of parsed and validated permission declaration.
enum PermissionModel<T: Hash + Eq> {
    Package(PackageName),
    UnitType(UnitType, PermissionScope<T>),
}

/// Converts a ParsedPermission to a package-based PermissionModel.
///
/// Precondition: package is set and unit_type is not set.
fn to_package_permission_model<T: Hash + Eq + FromStr>(
    declared_permission: &ParsedPermission,
    package: &PackageName,
) -> anyhow::Result<PermissionModel<T>>
where
    <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
{
    if declared_permission.allow_all.is_some() {
        bail!("Permission should not set 'allow_all' when 'package' field is set");
    }

    if !declared_permission.instance.is_empty() {
        bail!("Permission should not have 'topic'/'channel' when 'package' field is set");
    }

    Ok(PermissionModel::Package(package.clone()))
}

/// Converts a ParsedPermission to a unit_type-based PermissionModel.
///
/// Precondition: unit_type is set and package is not set.
fn to_unit_type_permission_model<T: Hash + Eq + FromStr>(
    declared_permission: ParsedPermission,
    unit_type: UnitType,
) -> anyhow::Result<PermissionModel<T>>
where
    <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
{
    let allow_all = declared_permission.allow_all.unwrap_or(false);
    if allow_all && !declared_permission.instance.is_empty() {
        bail!("Permission cannot have both 'allow_all_topics'/'allow_all_channels' set to true and 'topic'/'channel' field set");
    }

    if !allow_all && declared_permission.instance.is_empty() {
        bail!("Permission must have either 'allow_all_topics'/'allow_all_channels' set to true or 'topic'/'channel' field set");
    }

    let scope = if allow_all {
        PermissionScope::All
    } else {
        let mut set = HashSet::new();
        for instance_str in declared_permission.instance {
            let instance = T::from_str(&instance_str)?;
            set.insert(instance);
        }
        PermissionScope::Specific(set)
    };

    Ok(PermissionModel::UnitType(unit_type, scope))
}

/// Converts a ParsedPermission to a PermissionModel.
fn to_permission_model<T: Hash + Eq + FromStr>(
    declared_permission: ParsedPermission,
) -> anyhow::Result<PermissionModel<T>>
where
    <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
{
    let package = declared_permission.package.as_deref().map(PackageName::from_str).transpose()?;
    let unit_type = declared_permission.unit_type.as_deref().map(UnitType::from_str).transpose()?;

    match (package, unit_type) {
        // Invalid case #1: both package and unit_type are set
        (Some(_), Some(_)) => {
            bail!("Permission cannot have both 'package' and 'unit_type' fields set")
        }
        // Invalid case #2: neither package nor unit_type are set
        (None, None) => {
            bail!("Permission must have either 'package' or 'message'/'service' field set")
        }
        // Valid case #1: package-based permission
        (Some(package), None) => to_package_permission_model(&declared_permission, &package),
        // Valid case #2: unit_type-based permission
        (None, Some(unit_type)) => to_unit_type_permission_model(declared_permission, unit_type),
    }
}

impl VmAuthzPolicyModel {
    fn parse_permissions<T>(permissions: Vec<impl DeclaredPermission>) -> Result<Permissions<T>>
    where
        T: FromStr + std::hash::Hash + std::cmp::Eq,
        <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
    {
        let mut unit_type_permissions = HashMap::new();
        let mut package_permissions = HashSet::new();
        for permission in permissions {
            match to_permission_model(permission.to_parsed_permission()) {
                Ok(PermissionModel::Package(package)) => {
                    package_permissions.insert(package);
                }
                Ok(PermissionModel::UnitType(unit_type, scope)) => {
                    unit_type_permissions.insert(unit_type, scope);
                }
                Err(e) => return Err(e),
            }
        }
        Ok(Permissions::from((unit_type_permissions, package_permissions)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protobuf::text_format;

    #[test]
    fn test_publisher_permissions() {
        let proto = text_format::parse_from_str::<VmAuthzPolicy>(
            r#"
            publisher {
                package: "test.package.allowed"
            }
            publisher {
                message: "test.package.TypeWithTwoTopics"
                topic: "first"
                topic: "second"
            }
            publisher {
                message: "test.package.TypeWithOneTopic"
                topic: "third"
            }
            publisher {
                message: "test.package.TypeWithAllowAllTopics"
                allow_all_topics: true
            }
        "#,
        )
        .unwrap();
        let policy = VmAuthzPolicyModel::try_from(proto).unwrap();

        let type_within_allowed_package =
            UnitType::from_str("test.package.allowed.TypeWithinAllowedPackage").unwrap();
        let type_with_two_topics = UnitType::from_str("test.package.TypeWithTwoTopics").unwrap();
        let type_with_one_topic = UnitType::from_str("test.package.TypeWithOneTopic").unwrap();
        let type_with_allow_all_topics =
            UnitType::from_str("test.package.TypeWithAllowAllTopics").unwrap();

        let first = TopicName::from_str("first").unwrap();
        let second = TopicName::from_str("second").unwrap();
        let third = TopicName::from_str("third").unwrap();

        // Package permissions for "test.package.allowed"
        assert!(policy.can_publish(&type_within_allowed_package, &first));

        // test.package.TypeWithTwoTopics
        assert!(policy.can_publish(&type_with_two_topics, &first));
        assert!(policy.can_publish(&type_with_two_topics, &second));
        assert!(!policy.can_publish(&type_with_two_topics, &third));

        // test.package.TypeWithOneTopic
        assert!(policy.can_publish(&type_with_one_topic, &third));
        assert!(!policy.can_publish(&type_with_one_topic, &first));

        // test.package.TypeWithAllowAllTopics
        assert!(policy.can_publish(&type_with_allow_all_topics, &first));
        assert!(policy.can_publish(&type_with_allow_all_topics, &second));
        assert!(policy.can_publish(&type_with_allow_all_topics, &third));
    }

    #[test]
    fn test_subscriber_permissions() {
        let proto = text_format::parse_from_str::<VmAuthzPolicy>(
            r#"
            subscriber {
                package: "test.package.allowed"
            }
            subscriber {
                message: "test.package.TypeWithTwoTopics"
                topic: "first"
                topic: "second"
            }
            subscriber {
                message: "test.package.TypeWithOneTopic"
                topic: "third"
            }
            subscriber {
                message: "test.package.TypeWithAllowAllTopics"
                allow_all_topics: true
            }
        "#,
        )
        .unwrap();
        let policy = VmAuthzPolicyModel::try_from(proto).unwrap();

        let type_within_allowed_package =
            UnitType::from_str("test.package.allowed.TypeWithinAllowedPackage").unwrap();
        let type_with_two_topics = UnitType::from_str("test.package.TypeWithTwoTopics").unwrap();
        let type_with_one_topic = UnitType::from_str("test.package.TypeWithOneTopic").unwrap();
        let type_with_allow_all_topics =
            UnitType::from_str("test.package.TypeWithAllowAllTopics").unwrap();

        let first = TopicName::from_str("first").unwrap();
        let second = TopicName::from_str("second").unwrap();
        let third = TopicName::from_str("third").unwrap();

        // Package permissions for "test.package.allowed"
        assert!(policy.can_subscribe(&type_within_allowed_package, &first));

        // test.package.TypeWithTwoTopics
        assert!(policy.can_subscribe(&type_with_two_topics, &first));
        assert!(policy.can_subscribe(&type_with_two_topics, &second));
        assert!(!policy.can_subscribe(&type_with_two_topics, &third));

        // test.package.TypeWithOneTopic
        assert!(policy.can_subscribe(&type_with_one_topic, &third));
        assert!(!policy.can_subscribe(&type_with_one_topic, &first));

        // test.package.TypeWithAllowAllTopics
        assert!(policy.can_subscribe(&type_with_allow_all_topics, &first));
        assert!(policy.can_subscribe(&type_with_allow_all_topics, &second));
        assert!(policy.can_subscribe(&type_with_allow_all_topics, &third));
    }

    #[test]
    fn test_server_permissions() {
        let proto = text_format::parse_from_str::<VmAuthzPolicy>(
            r#"
            server {
                package: "test.package.allowed"
            }
            server {
                service: "test.package.TypeWithTwoChannels"
                channel: "first"
                channel: "second"
            }
            server {
                service: "test.package.TypeWithOneChannel"
                channel: "third"
            }
            server {
                service: "test.package.TypeWithAllowAllChannels"
                allow_all_channels: true
            }
        "#,
        )
        .unwrap();
        let policy = VmAuthzPolicyModel::try_from(proto).unwrap();

        let type_within_allowed_package =
            UnitType::from_str("test.package.allowed.TypeWithinAllowedPackage").unwrap();
        let type_with_two_channels =
            UnitType::from_str("test.package.TypeWithTwoChannels").unwrap();
        let type_with_one_channel = UnitType::from_str("test.package.TypeWithOneChannel").unwrap();
        let type_with_allow_all_channels =
            UnitType::from_str("test.package.TypeWithAllowAllChannels").unwrap();

        let first = ServiceUnitName::from_str("first").unwrap();
        let second = ServiceUnitName::from_str("second").unwrap();
        let third = ServiceUnitName::from_str("third").unwrap();

        // Package permissions for "test.package.allowed"
        assert!(policy.can_serve(&type_within_allowed_package, &first));

        // test.package.TypeWithTwoChannels
        assert!(policy.can_serve(&type_with_two_channels, &first));
        assert!(policy.can_serve(&type_with_two_channels, &second));
        assert!(!policy.can_serve(&type_with_two_channels, &third));

        // test.package.TypeWithOneChannel
        assert!(policy.can_serve(&type_with_one_channel, &third));
        assert!(!policy.can_serve(&type_with_one_channel, &first));

        // test.package.TypeWithAllowAllChannels
        assert!(policy.can_serve(&type_with_allow_all_channels, &first));
        assert!(policy.can_serve(&type_with_allow_all_channels, &second));
        assert!(policy.can_serve(&type_with_allow_all_channels, &third));
    }

    #[test]
    fn test_client_permissions() {
        let proto = text_format::parse_from_str::<VmAuthzPolicy>(
            r#"
            client {
                package: "test.package.allowed"
            }
            client {
                service: "test.package.TypeWithTwoChannels"
                channel: "first"
                channel: "second"
            }
            client {
                service: "test.package.TypeWithOneChannel"
                channel: "third"
            }
            client {
                service: "test.package.TypeWithAllowAllChannels"
                allow_all_channels: true
            }
        "#,
        )
        .unwrap();
        let policy = VmAuthzPolicyModel::try_from(proto).unwrap();

        let type_within_allowed_package =
            UnitType::from_str("test.package.allowed.TypeWithinAllowedPackage").unwrap();
        let type_with_two_channels =
            UnitType::from_str("test.package.TypeWithTwoChannels").unwrap();
        let type_with_one_channel = UnitType::from_str("test.package.TypeWithOneChannel").unwrap();
        let type_with_allow_all_channels =
            UnitType::from_str("test.package.TypeWithAllowAllChannels").unwrap();

        let first = ServiceUnitName::from_str("first").unwrap();
        let second = ServiceUnitName::from_str("second").unwrap();
        let third = ServiceUnitName::from_str("third").unwrap();

        // Package permissions for "test.package.allowed"
        assert!(policy.can_call(&type_within_allowed_package, &first));

        // test.package.TypeWithTwoChannels
        assert!(policy.can_call(&type_with_two_channels, &first));
        assert!(policy.can_call(&type_with_two_channels, &second));
        assert!(!policy.can_call(&type_with_two_channels, &third));

        // test.package.TypeWithOneChannel
        assert!(policy.can_call(&type_with_one_channel, &third));
        assert!(!policy.can_call(&type_with_one_channel, &first));

        // test.package.TypeWithAllowAllChannels
        assert!(policy.can_call(&type_with_allow_all_channels, &first));
        assert!(policy.can_call(&type_with_allow_all_channels, &second));
        assert!(policy.can_call(&type_with_allow_all_channels, &third));
    }

    #[test]
    fn test_invalid_publisher_config() {
        // 1. Both package and message set
        let proto_both = r#"
            publisher {
                package: "pkg"
                message: "msg"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_both).unwrap()
        )
        .is_err());

        // 2. Neither package nor message set
        let proto_empty = r#"
            publisher {
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_empty).unwrap()
        )
        .is_err());

        // 3. Message set, but no topic and no allow_all
        let proto_no_topic = r#"
            publisher {
                message: "type"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_no_topic).unwrap()
        )
        .is_err());

        // 4. Message set, allow_all=true AND topic set
        // Note: For package, topics are ignored, but for message, they are mutually exclusive.
        let proto_conflict = r#"
            publisher {
                message: "type"
                topic: "t"
                allow_all_topics: true
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_conflict).unwrap()
        )
        .is_err());

        // 5. Package set AND allow_all_topics=true
        let proto_package_allow_all = r#"
            publisher {
                package: "pkg"
                allow_all_topics: true
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_allow_all).unwrap()
        )
        .is_err());

        // 6. Package set AND topic set
        let proto_package_topic = r#"
            publisher {
                package: "pkg"
                topic: "t"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_topic).unwrap()
        )
        .is_err());

        // 7. Package set to a single dot
        let proto_package_dot = r#"
            publisher {
                package: "."
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_dot).unwrap()
        )
        .is_err());

        // 8. Package set to an asterisk
        let proto_package_ast = r#"
            publisher {
                package: "*"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_ast).unwrap()
        )
        .is_err());

        // 9. Package set to an empty string
        let proto_package_empty = r#"
            publisher {
                package: ""
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_empty).unwrap()
        )
        .is_err());
    }

    #[test]
    fn test_invalid_subscriber_config() {
        // 1. Both package and message set
        let proto_both = r#"
            subscriber {
                package: "pkg"
                message: "msg"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_both).unwrap()
        )
        .is_err());

        // 2. Neither package nor message set
        let proto_empty = r#"
            subscriber {
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_empty).unwrap()
        )
        .is_err());

        // 3. Message set, but no topic and no allow_all
        let proto_no_topic = r#"
            subscriber {
                message: "type"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_no_topic).unwrap()
        )
        .is_err());

        // 4. Message set, allow_all=true AND topic set
        let proto_conflict = r#"
            subscriber {
                message: "type"
                topic: "t"
                allow_all_topics: true
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_conflict).unwrap()
        )
        .is_err());

        // 5. Package set AND allow_all_topics=true
        let proto_package_allow_all = r#"
            subscriber {
                package: "pkg"
                allow_all_topics: true
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_allow_all).unwrap()
        )
        .is_err());

        // 6. Package set AND topic set
        let proto_package_topic = r#"
            subscriber {
                package: "pkg"
                topic: "t"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_topic).unwrap()
        )
        .is_err());

        // 7. Package set to a single dot
        let proto_package_dot = r#"
            subscriber {
                package: "."
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_dot).unwrap()
        )
        .is_err());

        // 8. Package set to an asterisk
        let proto_package_ast = r#"
            subscriber {
                package: "*"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_ast).unwrap()
        )
        .is_err());

        // 9. Package set to an empty string
        let proto_package_empty = r#"
            subscriber {
                package: ""
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_empty).unwrap()
        )
        .is_err());
    }

    #[test]
    fn test_invalid_server_config() {
        // 1. Both package and service set
        let proto_both = r#"
            server {
                package: "pkg"
                service: "svc"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_both).unwrap()
        )
        .is_err());

        // 2. Neither package nor service set
        let proto_empty = r#"
            server {
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_empty).unwrap()
        )
        .is_err());

        // 3. Service set, but no channel and no allow_all
        let proto_no_channel = r#"
            server {
                service: "svc"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_no_channel).unwrap()
        )
        .is_err());

        // 4. Service set, allow_all=true AND channel set
        let proto_conflict = r#"
            server {
                service: "svc"
                channel: "ch"
                allow_all_channels: true
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_conflict).unwrap()
        )
        .is_err());

        // 5. Package set AND allow_all_channels=true
        let proto_package_allow_all = r#"
            server {
                package: "pkg"
                allow_all_channels: true
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_allow_all).unwrap()
        )
        .is_err());

        // 6. Package set AND channel set
        let proto_package_channel = r#"
            server {
                package: "pkg"
                channel: "ch"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_channel).unwrap()
        )
        .is_err());

        // 7. Package set to a single dot
        let proto_package_dot = r#"
            server {
                package: "."
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_dot).unwrap()
        )
        .is_err());

        // 8. Package set to an asterisk
        let proto_package_ast = r#"
            server {
                package: "*"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_ast).unwrap()
        )
        .is_err());

        // 9. Package set to an empty string
        let proto_package_empty = r#"
            server {
                package: ""
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_empty).unwrap()
        )
        .is_err());
    }

    #[test]
    fn test_invalid_client_config() {
        // 1. Both package and service set
        let proto_both = r#"
            client {
                package: "pkg"
                service: "svc"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_both).unwrap()
        )
        .is_err());

        // 2. Neither package nor service set
        let proto_empty = r#"
            client {
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_empty).unwrap()
        )
        .is_err());

        // 3. Service set, but no channel and no allow_all
        let proto_no_channel = r#"
            client {
                service: "svc"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_no_channel).unwrap()
        )
        .is_err());

        // 4. Service set, allow_all=true AND channel set
        let proto_conflict = r#"
            client {
                service: "svc"
                channel: "ch"
                allow_all_channels: true
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_conflict).unwrap()
        )
        .is_err());

        // 5. Package set AND allow_all_channels=true
        let proto_package_allow_all = r#"
            client {
                package: "pkg"
                allow_all_channels: true
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_allow_all).unwrap()
        )
        .is_err());

        // 6. Package set AND channel set
        let proto_package_channel = r#"
            client {
                package: "pkg"
                channel: "ch"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_channel).unwrap()
        )
        .is_err());

        // 7. Package set to a single dot
        let proto_package_dot = r#"
            client {
                package: "."
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_dot).unwrap()
        )
        .is_err());

        // 8. Package set to an asterisk
        let proto_package_ast = r#"
            client {
                package: "*"
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_ast).unwrap()
        )
        .is_err());

        // 9. Package set to an empty string
        let proto_package_empty = r#"
            client {
                package: ""
            }
        "#;
        assert!(VmAuthzPolicyModel::try_from(
            text_format::parse_from_str::<VmAuthzPolicy>(proto_package_empty).unwrap()
        )
        .is_err());
    }

    #[test]
    fn test_package_match_logic() {
        let proto: VmAuthzPolicy = text_format::parse_from_str(
            r#"
            publisher { package: "com.allowed" }
            publisher { package: "com.deep.nested" }
            subscriber { package: "com.allowed" }
            subscriber { package: "com.deep.nested" }
            server { package: "com.allowed" }
            server { package: "com.deep.nested" }
            client { package: "com.allowed" }
            client { package: "com.deep.nested" }
        "#,
        )
        .unwrap();

        let policy = VmAuthzPolicyModel::try_from(proto).unwrap();

        let t = TopicName::from_str("t").unwrap();
        let s = ServiceUnitName::from_str("s").unwrap();

        // 1. Exact match
        let unit = UnitType::from_str("com.allowed.Message").unwrap();
        assert!(policy.can_publish(&unit, &t));
        assert!(policy.can_subscribe(&unit, &t));
        assert!(policy.can_serve(&unit, &s));
        assert!(policy.can_call(&unit, &s));

        // 2. Child match (prefix with dot)
        let unit = UnitType::from_str("com.allowed.child.Message").unwrap();
        assert!(policy.can_publish(&unit, &t));
        assert!(policy.can_subscribe(&unit, &t));
        assert!(policy.can_serve(&unit, &s));
        assert!(policy.can_call(&unit, &s));

        // 3. Deep child match
        let unit = UnitType::from_str("com.allowed.child.grandchild.Message").unwrap();
        assert!(policy.can_publish(&unit, &t));
        assert!(policy.can_subscribe(&unit, &t));
        assert!(policy.can_serve(&unit, &s));
        assert!(policy.can_call(&unit, &s));

        // 4. Sibling mismatch
        let unit = UnitType::from_str("com.disallowed.Message").unwrap();
        assert!(!policy.can_publish(&unit, &t));
        assert!(!policy.can_subscribe(&unit, &t));
        assert!(!policy.can_serve(&unit, &s));
        assert!(!policy.can_call(&unit, &s));

        // 5. Parent mismatch (when only child is allowed)
        let unit = UnitType::from_str("com.deep.Message").unwrap();
        assert!(!policy.can_publish(&unit, &t));
        assert!(!policy.can_subscribe(&unit, &t));
        assert!(!policy.can_serve(&unit, &s));
        assert!(!policy.can_call(&unit, &s));

        // 6. Partial prefix mismatch (no dot)
        let unit = UnitType::from_str("com.allowedish.Message").unwrap();
        assert!(!policy.can_publish(&unit, &t));
        assert!(!policy.can_subscribe(&unit, &t));
        assert!(!policy.can_serve(&unit, &s));
        assert!(!policy.can_call(&unit, &s));
    }
}
