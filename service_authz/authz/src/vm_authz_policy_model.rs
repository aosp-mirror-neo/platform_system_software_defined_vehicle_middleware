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

use anyhow::Result;
use sdv_authz_policy_proto_rust::vm_authz_policy::{
    Client as ClientProto, Publisher as PublisherProto, Server as ServerProto,
    Subscriber as SubscriberProto, VmAuthzPolicy as VmAuthzPolicyProto,
};
use sdv_comms_id::names::{ServiceUnitName, TopicName};
use sdv_comms_sd::discovery::UnitType;

/// VM Authz Policy represents Access Control Policy for a remote VM.
#[derive(Debug, Clone, PartialEq)]
pub struct VmAuthzPolicyModel {
    publish: Permissions<TopicName>,
    subscribe: Permissions<TopicName>,
    serve: Permissions<ServiceUnitName>,
    call: Permissions<ServiceUnitName>,
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

    fn parse_permissions<T, P>(
        allow_permissions: Vec<P>,
        deny_permissions: Vec<P>,
    ) -> Result<Permissions<T>>
    where
        T: FromStr + std::hash::Hash + std::cmp::Eq,
        P: Into<PermissionDeclaration>,
        <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
    {
        let mut permissions = Permissions::<T>::default();

        for permission in allow_permissions {
            let declaration: PermissionDeclaration = permission.into();
            permissions.add_allow_permission(declaration.validate()?);
        }

        for permission in deny_permissions {
            let declaration: PermissionDeclaration = permission.into();
            permissions.add_deny_permission(declaration.validate()?);
        }

        Ok(permissions)
    }
}

impl TryFrom<VmAuthzPolicyProto> for VmAuthzPolicyModel {
    type Error = anyhow::Error;

    fn try_from(proto: VmAuthzPolicyProto) -> Result<Self, Self::Error> {
        let publish = Self::parse_permissions(proto.allow_publisher, proto.deny_publisher)?;
        let subscribe = Self::parse_permissions(proto.allow_subscriber, proto.deny_subscriber)?;
        let serve = Self::parse_permissions(proto.allow_server, proto.deny_server)?;
        let call = Self::parse_permissions(proto.allow_client, proto.deny_client)?;

        Ok(VmAuthzPolicyModel { publish, subscribe, serve, call })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Permissions<T: Hash + Eq> {
    allow_all: bool,
    allowed_types: HashSet<String>,
    allowed_instances: HashMap<String, HashSet<T>>,
    deny_all: bool,
    denied_types: HashSet<String>,
    denied_instances: HashMap<String, HashSet<T>>,
}

impl<T: Hash + Eq> Default for Permissions<T> {
    fn default() -> Self {
        Self {
            allow_all: false,
            allowed_types: HashSet::new(),
            allowed_instances: HashMap::new(),
            deny_all: false,
            denied_types: HashSet::new(),
            denied_instances: HashMap::new(),
        }
    }
}

impl<T: Hash + Eq> Permissions<T> {
    fn check(&self, unit_type: &UnitType, value: &T) -> bool {
        let unit_type_str = unit_type.to_string();

        // 1. Specific Deny: (Message, Topic)
        if self.denied_instances.get(&unit_type_str).is_some_and(|set| set.contains(value)) {
            return false;
        }

        // 2. Specific Allow
        if self.allowed_instances.get(&unit_type_str).is_some_and(|set| set.contains(value)) {
            return true;
        }

        // 3. Message-wide Deny
        if self.denied_types.contains(&unit_type_str) {
            return false;
        }

        // 4. Message-wide Allow
        if self.allowed_types.contains(&unit_type_str) {
            return true;
        }

        // 5. Blanket Deny
        if self.deny_all {
            return false;
        }

        // 6. Blanket Allow
        if self.allow_all {
            return true;
        }

        false
    }

    fn add_allow_permission(&mut self, model: PermissionModel<T>) {
        match model {
            PermissionModel::All => self.allow_all = true,
            PermissionModel::Type(unit_type) => {
                self.allowed_types.insert(unit_type.to_string());
            }
            PermissionModel::Instance(unit_type, instances) => {
                self.allowed_instances.entry(unit_type.to_string()).or_default().extend(instances);
            }
        }
    }

    fn add_deny_permission(&mut self, model: PermissionModel<T>) {
        match model {
            PermissionModel::All => self.deny_all = true,
            PermissionModel::Type(unit_type) => {
                self.denied_types.insert(unit_type.to_string());
            }
            PermissionModel::Instance(unit_type, instances) => {
                self.denied_instances.entry(unit_type.to_string()).or_default().extend(instances);
            }
        }
    }
}

/// Validated model of declared permission.
enum PermissionModel<T: Hash + Eq> {
    All,
    Type(UnitType),
    Instance(UnitType, HashSet<T>),
}

/// Common representation of a .proto-derived permission struct to enable generic parsing.
struct PermissionDeclaration {
    unit_type: String,
    instances: HashSet<String>,
}

impl From<PublisherProto> for PermissionDeclaration {
    fn from(publisher: PublisherProto) -> Self {
        PermissionDeclaration {
            unit_type: publisher.message,
            instances: publisher.topic.into_iter().collect(),
        }
    }
}

impl From<SubscriberProto> for PermissionDeclaration {
    fn from(subscriber: SubscriberProto) -> Self {
        PermissionDeclaration {
            unit_type: subscriber.message,
            instances: subscriber.topic.into_iter().collect(),
        }
    }
}

impl From<ServerProto> for PermissionDeclaration {
    fn from(server: ServerProto) -> Self {
        PermissionDeclaration {
            unit_type: server.service,
            instances: server.channel.into_iter().collect(),
        }
    }
}

impl From<ClientProto> for PermissionDeclaration {
    fn from(client: ClientProto) -> Self {
        PermissionDeclaration {
            unit_type: client.service,
            instances: client.channel.into_iter().collect(),
        }
    }
}

impl PermissionDeclaration {
    /// Validates and extracts PermissionModels from a PermissionDeclaration.
    fn validate<T: FromStr + Hash + Eq>(self) -> Result<PermissionModel<T>>
    where
        <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
    {
        if self.unit_type == "*" {
            self.validate_blanket_permission()
        } else {
            let unit_type = UnitType::from_str(&self.unit_type)?;

            if self.instances.contains("*") {
                self.validate_type_permission(unit_type)
            } else {
                self.validate_instance_permission(unit_type)
            }
        }
    }

    fn validate_blanket_permission<T: Hash + Eq>(&self) -> Result<PermissionModel<T>> {
        if self.instances.len() == 1 && self.instances.contains("*") {
            Ok(PermissionModel::All)
        } else {
            Err(anyhow::anyhow!(
                "Invalid permission declaration, when 'message'/'service' is '*', 'topic'/'channel' can only contain '*', instead contained {:?}",
                self.instances
            ))
        }
    }

    fn validate_type_permission<T: FromStr + Hash + Eq>(
        self,
        unit_type: UnitType,
    ) -> Result<PermissionModel<T>>
    where
        <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
    {
        if self.instances.len() > 1 {
            return Err(anyhow::anyhow!(
                "Invalid permission declaration, when 'topic'/'channel' contains '*', 'topic'/'channel' shouldn't contain other values, instead contained {:?}",
                self.instances
            ));
        }

        Ok(PermissionModel::Type(unit_type))
    }

    fn validate_instance_permission<T: FromStr + Hash + Eq>(
        self,
        unit_type: UnitType,
    ) -> Result<PermissionModel<T>>
    where
        <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
    {
        let mut allowed_instances = HashSet::new();
        for instance in self.instances {
            allowed_instances.insert(T::from_str(&instance)?);
        }
        Ok(PermissionModel::Instance(unit_type, allowed_instances))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protobuf::text_format;
    use sdv_authz_policy_proto_rust::vm_authz_policy::VmAuthzPolicy as VmAuthzPolicyProto;
    use sdv_comms_id::names::{ServiceUnitName, TopicName};
    use sdv_comms_sd::discovery::UnitType;
    use std::str::FromStr;

    fn parse_policy(text_proto: &str) -> Result<VmAuthzPolicyModel> {
        let policy_proto: VmAuthzPolicyProto = text_format::parse_from_str(text_proto).unwrap();
        VmAuthzPolicyModel::try_from(policy_proto)
    }

    // Test Constants
    fn message() -> UnitType {
        UnitType::from_str("test.package.Message").unwrap()
    }

    fn other_message() -> UnitType {
        UnitType::from_str("test.package.OtherMessage").unwrap()
    }

    fn topic() -> TopicName {
        TopicName::from_str("topic").unwrap()
    }

    fn other_topic() -> TopicName {
        TopicName::from_str("other-topic").unwrap()
    }

    fn service() -> UnitType {
        UnitType::from_str("test.package.Service").unwrap()
    }

    fn other_service() -> UnitType {
        UnitType::from_str("test.package.OtherService").unwrap()
    }

    fn channel() -> ServiceUnitName {
        ServiceUnitName::from_str("channel").unwrap()
    }

    fn other_channel() -> ServiceUnitName {
        ServiceUnitName::from_str("other-channel").unwrap()
    }

    const ALLOW_ALL_POLICY: &str = r#"
        allow_publisher {
            message: "*"
            topic: "*"
        }
        allow_subscriber {
            message: "*"
            topic: "*"
        }
        allow_server {
            service: "*"
            channel: "*"
        }
        allow_client {
            service: "*"
            channel: "*"
        }
    "#;

    const DENY_ALL_POLICY: &str = r#"
        deny_publisher {
            message: "*"
            topic: "*"
        }
        deny_subscriber {
            message: "*"
            topic: "*"
        }
        deny_server {
            service: "*"
            channel: "*"
        }
        deny_client {
            service: "*"
            channel: "*"
        }
    "#;

    const WILDCARD_INSTANCE_ALLOW_POLICY: &str = r#"
        allow_publisher {
            message: "test.package.Message"
            topic: "*"
        }
        allow_subscriber {
            message: "test.package.Message"
            topic: "*"
        }
        allow_server {
            service: "test.package.Service"
            channel: "*"
        }
        allow_client {
            service: "test.package.Service"
            channel: "*"
        }
    "#;

    const WILDCARD_INSTANCE_DENY_POLICY: &str = r#"
        deny_publisher {
            message: "test.package.Message"
            topic: "*"
        }
        deny_subscriber {
            message: "test.package.Message"
            topic: "*"
        }
        deny_server {
            service: "test.package.Service"
            channel: "*"
        }
        deny_client {
            service: "test.package.Service"
            channel: "*"
        }
    "#;

    const GRANULAR_ALLOW_POLICY: &str = r#"
        allow_publisher {
            message: "test.package.Message"
            topic: "topic"
        }
        allow_subscriber {
            message: "test.package.Message"
            topic: "topic"
        }
        allow_server {
            service: "test.package.Service"
            channel: "channel"
        }
        allow_client {
            service: "test.package.Service"
            channel: "channel"
        }
    "#;

    const GRANULAR_DENY_POLICY: &str = r#"
        deny_publisher {
            message: "test.package.Message"
            topic: "topic"
        }
        deny_subscriber {
            message: "test.package.Message"
            topic: "topic"
        }
        deny_server {
            service: "test.package.Service"
            channel: "channel"
        }
        deny_client {
            service: "test.package.Service"
            channel: "channel"
        }
    "#;

    #[test]
    fn test_with_subpackage() {
        // GIVEN an empty policy
        let policy = parse_policy(
            r#"
            allow_publisher {
                message: "test.package.subpackage.Message"
                topic: "topic"
            }
            allow_subscriber {
                message: "test.package.subpackage.Message"
                topic: "topic"
            }
            allow_server {
                service: "test.package.subpackage.Service"
                channel: "channel"
            }
            allow_client {
                service: "test.package.subpackage.Service"
                channel: "channel"
            }
        "#,
        )
        .expect("Failed to parse policy");

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

        assert!(policy.can_publish(&non_normalized_message, &topic()));
        assert!(policy.can_subscribe(&non_normalized_message, &topic()));
        assert!(policy.can_serve(&non_normalized_service, &channel()));
        assert!(policy.can_call(&non_normalized_service, &channel()));
    }

    #[test]
    fn test_deny_by_default() {
        let policy = parse_policy(
            r#"
        "#,
        )
        .expect("Failed to parse policy");

        // THEN everything is denied by default
        assert!(!policy.can_publish(&message(), &topic()));
        assert!(!policy.can_subscribe(&message(), &topic()));
        assert!(!policy.can_serve(&service(), &channel()));
        assert!(!policy.can_call(&service(), &channel()));
    }

    #[test]
    fn test_granular_allow() {
        // GIVEN a policy with a granular allow
        let policy = parse_policy(GRANULAR_ALLOW_POLICY).expect("Failed to parse policy");

        // THEN the policy allows the specified combination of message/topic and service/channel
        assert!(policy.can_publish(&message(), &topic()));
        assert!(policy.can_subscribe(&message(), &topic()));
        assert!(policy.can_serve(&service(), &channel()));
        assert!(policy.can_call(&service(), &channel()));

        // THEN the policy denies all other combinations
        assert!(!policy.can_publish(&message(), &other_topic()));
        assert!(!policy.can_subscribe(&message(), &other_topic()));
        assert!(!policy.can_serve(&service(), &other_channel()));
        assert!(!policy.can_call(&service(), &other_channel()));

        assert!(!policy.can_publish(&other_message(), &topic()));
        assert!(!policy.can_subscribe(&other_message(), &topic()));
        assert!(!policy.can_serve(&other_service(), &channel()));
        assert!(!policy.can_call(&other_service(), &channel()));
    }

    #[test]
    fn test_wildcard_instance_allow() {
        // GIVEN a policy with a wildcard instance allow
        let policy = parse_policy(WILDCARD_INSTANCE_ALLOW_POLICY).expect("Failed to parse policy");

        // THEN the policy allows any message/topic with specified service/channel
        assert!(policy.can_publish(&message(), &topic()));
        assert!(policy.can_subscribe(&message(), &topic()));
        assert!(policy.can_serve(&service(), &channel()));
        assert!(policy.can_call(&service(), &channel()));

        assert!(policy.can_publish(&message(), &other_topic()));
        assert!(policy.can_subscribe(&message(), &other_topic()));
        assert!(policy.can_serve(&service(), &other_channel()));
        assert!(policy.can_call(&service(), &other_channel()));
    }

    #[test]
    fn test_wildcard_instance_and_type_allow() {
        // GIVEN a policy with a wildcard instance and type allow
        let policy = parse_policy(ALLOW_ALL_POLICY).expect("Failed to parse policy");

        // THEN the policy allows any message/topic with any service/channel
        assert!(policy.can_publish(&message(), &topic()));
        assert!(policy.can_subscribe(&message(), &topic()));
        assert!(policy.can_serve(&service(), &channel()));
        assert!(policy.can_call(&service(), &channel()));

        assert!(policy.can_publish(&other_message(), &other_topic()));
        assert!(policy.can_subscribe(&other_message(), &other_topic()));
        assert!(policy.can_serve(&other_service(), &other_channel()));
        assert!(policy.can_call(&other_service(), &other_channel()));
    }

    #[test]
    fn test_granular_deny() {
        // GIVEN a policy with a granular deny
        // AND allow rules with wildcards
        // AND granular allow rules
        let policy_text = format!(
            r#"
            # allow all by default

            {ALLOW_ALL_POLICY}

            # Deny specific combinations

            {GRANULAR_DENY_POLICY}
        "#
        );
        let policy = parse_policy(&policy_text).expect("Failed to parse policy");

        // THEN the policy denies the specified combination of message/topic and service/channel
        assert!(!policy.can_publish(&message(), &topic()));
        assert!(!policy.can_subscribe(&message(), &topic()));
        assert!(!policy.can_serve(&service(), &channel()));
        assert!(!policy.can_call(&service(), &channel()));

        // THEN the policy allows other combinations
        assert!(policy.can_publish(&message(), &other_topic()));
        assert!(policy.can_subscribe(&message(), &other_topic()));
        assert!(policy.can_serve(&service(), &other_channel()));
        assert!(policy.can_call(&service(), &other_channel()));

        assert!(policy.can_publish(&other_message(), &topic()));
        assert!(policy.can_subscribe(&other_message(), &topic()));
        assert!(policy.can_serve(&other_service(), &channel()));
        assert!(policy.can_call(&other_service(), &channel()));
    }

    #[test]
    fn test_wildcard_instance_deny() {
        // GIVEN a policy with a wildcard instance deny
        let policy_text = format!(
            r#"
            # allow all by default

            {ALLOW_ALL_POLICY}

            # deny any message/topic for specific service/channel

            {WILDCARD_INSTANCE_DENY_POLICY}
        "#
        );
        let policy = parse_policy(&policy_text).expect("Failed to parse policy");

        // THEN the policy denies any message/topic for specific service/channel
        assert!(!policy.can_publish(&message(), &topic()));
        assert!(!policy.can_subscribe(&message(), &topic()));
        assert!(!policy.can_serve(&service(), &channel()));
        assert!(!policy.can_call(&service(), &channel()));

        assert!(!policy.can_publish(&message(), &other_topic()));
        assert!(!policy.can_subscribe(&message(), &other_topic()));
        assert!(!policy.can_serve(&service(), &other_channel()));
        assert!(!policy.can_call(&service(), &other_channel()));

        // THEN the policy allows other combinations
        assert!(policy.can_publish(&other_message(), &topic()));
        assert!(policy.can_subscribe(&other_message(), &topic()));
        assert!(policy.can_serve(&other_service(), &channel()));
        assert!(policy.can_call(&other_service(), &channel()));
    }

    #[test]
    fn test_wildcard_type_and_instance_deny() {
        // GIVEN a policy with a wildcard type and instance deny
        let policy_text = format!(
            r#"
            # allow all by default

            {ALLOW_ALL_POLICY}

            # deny any message/topic for specific service/channel

            {DENY_ALL_POLICY}
        "#
        );
        let policy = parse_policy(&policy_text).expect("Failed to parse policy");

        // THEN the policy denies any message/topic for any service/channel.
        // Even overriding allow all rules.
        assert!(!policy.can_publish(&message(), &topic()));
        assert!(!policy.can_subscribe(&message(), &topic()));
        assert!(!policy.can_serve(&service(), &channel()));
        assert!(!policy.can_call(&service(), &channel()));
    }

    #[test]
    fn test_precedence_granular_deny() {
        // GIVEN a policy with a allow all
        // AND a wildcard instance allow
        // AND a granular allow
        // AND a granular deny
        let policy_text = format!(
            r#"
            {ALLOW_ALL_POLICY}

            {WILDCARD_INSTANCE_ALLOW_POLICY}

            {GRANULAR_ALLOW_POLICY}

            {GRANULAR_DENY_POLICY}
        "#
        );
        let policy = parse_policy(&policy_text).expect("Failed to parse policy");

        // THEN the policy denies the specified combination of message/topic and service/channel
        assert!(!policy.can_publish(&message(), &topic()));
        assert!(!policy.can_subscribe(&message(), &topic()));
        assert!(!policy.can_serve(&service(), &channel()));
        assert!(!policy.can_call(&service(), &channel()));
    }

    #[test]
    fn test_precedence_granular_allow() {
        // GIVEN a policy with a deny all
        // AND a wildcard instance deny
        // AND a granular allow
        let policy_text = format!(
            r#"
            {DENY_ALL_POLICY}

            {WILDCARD_INSTANCE_DENY_POLICY}

            {GRANULAR_ALLOW_POLICY}
        "#
        );
        let policy = parse_policy(&policy_text).expect("Failed to parse policy");

        // THEN the policy allows the specified combination of message/topic and service/channel
        assert!(policy.can_publish(&message(), &topic()));
        assert!(policy.can_subscribe(&message(), &topic()));
        assert!(policy.can_serve(&service(), &channel()));
        assert!(policy.can_call(&service(), &channel()));
    }

    #[test]
    fn test_precedence_wildcard_instance_deny() {
        // GIVEN a policy with a allow all
        // AND a wildcard instance allow
        // AND a granular allow
        // AND a wildcard instance deny
        let policy_text = format!(
            r#"
            {ALLOW_ALL_POLICY}

            {WILDCARD_INSTANCE_ALLOW_POLICY}

            {GRANULAR_ALLOW_POLICY}

            {WILDCARD_INSTANCE_DENY_POLICY}
        "#
        );
        let policy = parse_policy(&policy_text).expect("Failed to parse policy");

        // THEN deny takes precedence over allow
        assert!(!policy.can_publish(&message(), &other_topic()));
        assert!(!policy.can_subscribe(&message(), &other_topic()));
        assert!(!policy.can_serve(&service(), &other_channel()));
        assert!(!policy.can_call(&service(), &other_channel()));

        // EXCEPT for the granular allow
        assert!(policy.can_publish(&message(), &topic()));
        assert!(policy.can_subscribe(&message(), &topic()));
        assert!(policy.can_serve(&service(), &channel()));
        assert!(policy.can_call(&service(), &channel()));
    }

    #[test]
    fn test_precedence_wildcard_instance_and_type_deny() {
        // GIVEN a policy with a allow all
        // AND a wildcard instance allow
        // AND a deny all
        let policy_text = format!(
            r#"
            {ALLOW_ALL_POLICY}

            {WILDCARD_INSTANCE_ALLOW_POLICY}

            {DENY_ALL_POLICY}
        "#
        );
        let policy = parse_policy(&policy_text).expect("Failed to parse policy");

        // THEN deny all takes precedence over allow all
        assert!(!policy.can_publish(&other_message(), &other_topic()));
        assert!(!policy.can_subscribe(&other_message(), &other_topic()));
        assert!(!policy.can_serve(&other_service(), &other_channel()));
        assert!(!policy.can_call(&other_service(), &other_channel()));

        // But not over more granular allow rules
        assert!(policy.can_publish(&message(), &topic()));
        assert!(policy.can_subscribe(&message(), &topic()));
        assert!(policy.can_serve(&service(), &channel()));
        assert!(policy.can_call(&service(), &channel()));
    }

    #[test]
    fn test_invalid_policy_no_message_or_service() {
        let policy = parse_policy(
            r#"
            allow_publisher {
                topic: "*"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_publisher {
                topic: "*"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_subscriber {
                topic: "*"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_subscriber {
                topic: "*"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_server {
                channel: "*"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_server {
                channel: "*"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_client {
                channel: "*"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_client {
                channel: "*"
            }
        "#,
        );
        assert!(policy.is_err());
    }

    #[test]
    fn test_invalid_policy_no_topic_or_channel() {
        let policy = parse_policy(
            r#"
            allow_publisher {
                message: "*"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_publisher {
                message: "*"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_subscriber {
                message: "*"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_subscriber {
                message: "*"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_server {
                service: "*"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_server {
                service: "*"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_client {
                service: "*"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_client {
                service: "*"
            }
        "#,
        );
        assert!(policy.is_err());
    }

    #[test]
    fn test_invalid_policy_wildcard_type_with_concrete_instance() {
        let policy = parse_policy(
            r#"
            allow_publisher {
                message: "*"
                topic: "topic"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_publisher {
                message: "*"
                topic: "topic"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_subscriber {
                message: "*"
                topic: "topic"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_subscriber {
                message: "*"
                topic: "topic"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_server {
                service: "*"
                channel: "channel"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_server {
                service: "*"
                channel: "channel"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_client {
                service: "*"
                channel: "channel"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_client {
                service: "*"
                channel: "channel"
            }
        "#,
        );
        assert!(policy.is_err());
    }

    #[test]
    fn test_invalid_policy_wildcard_instance_with_concrete_instance() {
        let policy = parse_policy(
            r#"
            allow_publisher {
                message: "test.package.Message"
                topic: "*"
                topic: "topic"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_publisher {
                message: "test.package.Message"
                topic: "*"
                topic: "topic"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_subscriber {
                message: "test.package.Message"
                topic: "*"
                topic: "topic"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_subscriber {
                message: "test.package.Message"
                topic: "*"
                topic: "topic"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_server {
                service: "test.package.Service"
                channel: "*"
                channel: "channel"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_server {
                service: "test.package.Service"
                channel: "*"
                channel: "channel"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_client {
                service: "test.package.Service"
                channel: "*"
                channel: "channel"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_client {
                service: "test.package.Service"
                channel: "*"
                channel: "channel"
            }
        "#,
        );
        assert!(policy.is_err());
    }

    #[test]
    fn test_invalid_policy_invalid_type_name() {
        let policy = parse_policy(
            r#"
            allow_publisher {
                message: "message"
                topic: "topic"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_publisher {
                message: "message"
                topic: "topic"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_subscriber {
                message: "message"
                topic: "topic"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_subscriber {
                message: "message"
                topic: "topic"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_server {
                service: "service"
                channel: "channel"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_server {
                service: "service"
                channel: "channel"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_client {
                service: "service"
                channel: "channel"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_client {
                service: "service"
                channel: "channel"
            }
        "#,
        );
        assert!(policy.is_err());
    }

    #[test]
    fn test_invalid_policy_invalid_instance_name() {
        let policy = parse_policy(
            r#"
            allow_publisher {
                message: "test.package.Message"
                topic: "topic_with_underscore"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_publisher {
                message: "test.package.Message"
                topic: "topic_with_underscore"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_subscriber {
                message: "test.package.Message"
                topic: "topic_with_underscore"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_subscriber {
                message: "test.package.Message"
                topic: "topic_with_underscore"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_server {
                service: "test.package.Service"
                channel: "channel_with_underscore"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_server {
                service: "test.package.Service"
                channel: "channel_with_underscore"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            allow_client {
                service: "test.package.Service"
                channel: "channel_with_underscore"
            }
        "#,
        );
        assert!(policy.is_err());

        let policy = parse_policy(
            r#"
            deny_client {
                service: "test.package.Service"
                channel: "channel_with_underscore"
            }
        "#,
        );
        assert!(policy.is_err());
    }
}
