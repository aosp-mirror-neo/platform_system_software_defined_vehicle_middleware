/*
 * Copyright (C) 2026 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Middleware Types for Discovery and Registration

use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::fmt;
use std::time::Duration;

/// Qualified Service Unit Name
#[derive(Debug, Clone, PartialEq)]
pub struct UnitName {
    /// package name
    package: String,
    /// bundle name
    bundle: String,
    /// vm name
    vm: String,
    /// service unit name
    name: String,
    /// instance name
    instance_name: String,
}

impl fmt::Display for UnitName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}", self.package, self.name)
    }
}

impl UnitName {
    /// returns a Builder for UnitName
    pub fn builder() -> UnitNameBuilder {
        UnitNameBuilder::default()
    }
    /// vm name of the service bundle offering this service unit
    pub fn get_vm_name(&self) -> &str {
        &self.vm
    }
    /// package name of the service bundle offering this service unit
    pub fn get_package_name(&self) -> &str {
        &self.package
    }
    /// bundle name of the service bundle offering this service unit
    pub fn get_bundle_name(&self) -> &str {
        &self.bundle
    }
    /// service unit name
    pub fn get_service_unit_name(&self) -> &str {
        &self.name
    }
    /// instance name of the service bundle offering this service unit
    pub fn get_instance_name(&self) -> &str {
        &self.instance_name
    }
}

#[derive(Clone, Default, Debug)]
pub struct UnitNameBuilder {
    vm_name: String,
    package_name: String,
    bundle_name: String,
    service_unit_name: String,
    instance_name: String,
}

impl UnitNameBuilder {
    /// set the vm name of the service bundle offering this service unit.
    pub fn vm_name(mut self, vm_name: impl Into<String>) -> Self {
        self.vm_name = vm_name.into();
        self
    }
    /// set the package name of the service bundle offering this service unit.
    pub fn package_name(mut self, package_name: impl Into<String>) -> Self {
        self.package_name = package_name.into();
        self
    }
    /// set the bundle name of the service bundle offering this service unit.
    pub fn bundle_name(mut self, bundle_name: impl Into<String>) -> Self {
        self.bundle_name = bundle_name.into();
        self
    }
    /// set the service unit name.
    pub fn service_unit_name(mut self, service_unit_name: impl Into<String>) -> Self {
        self.service_unit_name = service_unit_name.into();
        self
    }
    /// set the instance name of the service bundle offering this service unit
    pub fn instance_name(mut self, instance_name: impl Into<String>) -> Self {
        self.instance_name = instance_name.into();
        self
    }
    /// Validates UnitName before returning an instance of it.
    pub fn build(self) -> SdvResult<UnitName> {
        //TODO(b/379139605): Validate builder arguments
        Ok(UnitName {
            vm: self.vm_name,
            package: self.package_name,
            bundle: self.bundle_name,
            name: self.service_unit_name,
            instance_name: self.instance_name,
        })
    }
}

/// TypeName
#[derive(Debug, Clone, PartialEq)]
pub struct TypeName {
    /// package name
    package: String,
    /// bundle name
    bundle: String,
    /// type name
    name: String,
}

impl fmt::Display for TypeName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}", self.package, self.name)
    }
}

impl TypeName {
    /// returns a Builder for TypeName
    pub fn builder() -> TypeNameBuilder {
        TypeNameBuilder::default()
    }
    /// package name
    pub fn get_package_name(&self) -> &str {
        &self.package
    }
    /// bundle name
    pub fn get_bundle_name(&self) -> &str {
        &self.bundle
    }
    /// type name
    pub fn get_type_name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Default, Debug)]
pub struct TypeNameBuilder {
    package_name: String,
    bundle_name: String,
    type_name: String,
}

impl TypeNameBuilder {
    /// set the package name
    pub fn package_name(mut self, package_name: impl Into<String>) -> Self {
        self.package_name = package_name.into();
        self
    }
    /// set the bundle name
    pub fn bundle_name(mut self, bundle_name: impl Into<String>) -> Self {
        self.bundle_name = bundle_name.into();
        self
    }
    /// set the vm name of the service bundle offering this service unit.
    pub fn type_name(mut self, type_name: impl Into<String>) -> Self {
        self.type_name = type_name.into();
        self
    }
    /// Validates TypeName before returning an instance of it.
    pub fn build(self) -> SdvResult<TypeName> {
        //TODO(b/379139605): Validate builder arguments
        Ok(TypeName { package: self.package_name, bundle: self.bundle_name, name: self.type_name })
    }
}

/// UnitQuery
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum UnitQuery {
    /// query by UnitName
    Name(UnitName),
    /// query by TypeName
    Type(TypeName),
}

/// PublisherAttributes
#[derive(Debug, Clone)]
pub struct PublisherAttributes {
    /// publication unit name
    publication_name: String,
    /// TypeName
    type_name: TypeName,
    /// publisher metadata
    metadata: PublisherMetadata,
}

impl PublisherAttributes {
    /// returns a Builder for PublisherAttributes
    pub fn builder() -> PublisherAttributesBuilder {
        PublisherAttributesBuilder::default()
    }
    /// publication unit name
    pub fn get_publication_name(&self) -> &str {
        &self.publication_name
    }
    /// TypeName
    pub fn get_type_name(&self) -> &TypeName {
        &self.type_name
    }
    /// publisher metadata
    pub fn get_metadata(&self) -> &PublisherMetadata {
        &self.metadata
    }
}

#[derive(Clone, Default, Debug)]
pub struct PublisherAttributesBuilder {
    publication_name: String,
    type_name: Option<TypeName>,
    metadata: PublisherMetadata,
}

impl PublisherAttributesBuilder {
    /// set the publication name
    pub fn publication_name(mut self, publication_name: impl Into<String>) -> Self {
        self.publication_name = publication_name.into();
        self
    }
    /// set the type name
    pub fn type_name(mut self, type_name: TypeName) -> Self {
        self.type_name = Some(type_name);
        self
    }
    /// set the publisher metadata
    pub fn metadata(mut self, metadata: PublisherMetadata) -> Self {
        self.metadata = metadata;
        self
    }
    /// Validates PublisherAttributes before returning an instance of it.
    pub fn build(self) -> SdvResult<PublisherAttributes> {
        //TODO(b/379139605): Validate builder arguments
        let Some(type_name) = self.type_name else {
            return Err(SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                "type_name must be set",
            ));
        };
        Ok(PublisherAttributes {
            publication_name: self.publication_name,
            type_name,
            metadata: self.metadata,
        })
    }
}
/// PublisherMetadata
#[derive(Debug, Default, Clone)]
pub struct PublisherMetadata {
    /// size of the mssage, in bytes
    message_size: u32,
    /// size of the message queue on publisher side
    message_count: u32, // runtime, when creating a publisher
    /// true if multiple publishers can exist for this
    multipub: bool,
    /// message metadata for reflection: fq-message name and FileDescriptorProtos
    message_metadata: sdv_proto_reflection_rs::message_metadata::MessageMetadata,
    /// true if message metadata shall be registered
    message_metadata_required: bool,
}

impl PublisherMetadata {
    /// returns a Builder for PublisherMetadata
    pub fn builder() -> PublisherMetadataBuilder {
        PublisherMetadataBuilder::default()
    }
    /// size of the message, in bytes
    pub fn get_message_size(&self) -> u32 {
        self.message_size
    }
    /// size of the message queue on publisher side
    pub fn get_message_count(&self) -> u32 {
        self.message_count
    }
    /// true if multiple publishers can exist for this
    pub fn is_multipub(&self) -> bool {
        self.multipub
    }
    /// message metadata for reflection
    pub fn get_message_metadata(
        &self,
    ) -> &sdv_proto_reflection_rs::message_metadata::MessageMetadata {
        &self.message_metadata
    }
    /// true if message metadata shall be registered
    pub fn is_message_metadata_required(&self) -> bool {
        self.message_metadata_required
    }
}

#[derive(Clone, Default, Debug)]
pub struct PublisherMetadataBuilder {
    message_size: u32,
    message_count: u32,
    is_multipub: bool,
    message_metadata: sdv_proto_reflection_rs::message_metadata::MessageMetadata,
    message_metadata_required: bool,
}

impl PublisherMetadataBuilder {
    /// set size of the message, in bytes
    pub fn message_size(mut self, message_size: u32) -> Self {
        self.message_size = message_size;
        self
    }
    /// set size of the message queue on publisher side
    pub fn message_count(mut self, message_count: u32) -> Self {
        self.message_count = message_count;
        self
    }
    /// set true if multiple publishers can exist for this
    #[allow(clippy::wrong_self_convention)]
    pub fn is_multipub(mut self, is_multipub: bool) -> Self {
        self.is_multipub = is_multipub;
        self
    }
    /// set message metadata for reflection
    pub fn message_metadata(
        mut self,
        message_metadata: sdv_proto_reflection_rs::message_metadata::MessageMetadata,
    ) -> Self {
        self.message_metadata = message_metadata;
        self
    }
    /// set true if message metadata shall be registered
    pub fn message_metadata_required(mut self, message_metadata_required: bool) -> Self {
        self.message_metadata_required = message_metadata_required;
        self
    }

    /// Validates PublisherMetadata before returning an instance of it.
    pub fn build(self) -> SdvResult<PublisherMetadata> {
        //TODO(b/379139605): Validate builder arguments
        Ok(PublisherMetadata {
            message_size: self.message_size,
            message_count: self.message_count,
            multipub: self.is_multipub,
            message_metadata: self.message_metadata,
            message_metadata_required: self.message_metadata_required,
        })
    }
}

/// SubscribeOptions
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SubscribeOptions {
    /// makes the latest published message available to the subscriber upon subscription
    read_on_subscribe: bool,
    /// minimum callback interval (observer-only)
    min_interval: Duration,
    /// size of message queue (history-only)
    message_count: u32,
    /// optional VM name to look for, applies to observer, subscriber, history, instant reader
    vm_name: String,
    /// optional service bundle instance name to look for, applies to observer, subscriber, history, instant reader
    instance_name: String,
}

impl SubscribeOptions {
    /// returns a Builder for SubscribeOptions
    pub fn builder() -> SubscribeOptionsBuilder {
        SubscribeOptionsBuilder::default()
    }
    /// makes the latest published message available to the subscriber upon subscription
    pub fn read_on_subscribe(&self) -> bool {
        self.read_on_subscribe
    }
    /// minimum callback interval (observer-only)
    pub fn get_min_interval(&self) -> Duration {
        self.min_interval
    }
    /// size of message queue (history-only)
    pub fn get_message_count(&self) -> u32 {
        self.message_count
    }
    /// name of the virtual machine
    pub fn get_vm_name(&self) -> &str {
        &self.vm_name
    }
    /// name of the specific service bundle instance
    pub fn get_instance_name(&self) -> &str {
        &self.instance_name
    }
}

#[derive(Clone, Default, Debug)]
pub struct SubscribeOptionsBuilder {
    read_on_subscribe: bool,
    min_interval: Duration,
    message_count: u32,
    vm_name: String,
    instance_name: String,
}

impl SubscribeOptionsBuilder {
    /// set true to make the latest published message available to the subscriber upon subscription
    pub fn read_on_subscribe(mut self, read_on_subscribe: bool) -> Self {
        self.read_on_subscribe = read_on_subscribe;
        self
    }
    /// set minimum callback interval (observer-only)
    pub fn min_interval(mut self, min_interval: Duration) -> Self {
        self.min_interval = min_interval;
        self
    }
    /// set size of message queue (history-only)
    pub fn message_count(mut self, message_count: u32) -> Self {
        self.message_count = message_count;
        self
    }
    /// set vm name
    pub fn vm_name(mut self, vm_name: impl Into<String>) -> Self {
        self.vm_name = vm_name.into();
        self
    }
    /// set instance name
    pub fn instance_name(mut self, instance_name: impl Into<String>) -> Self {
        self.instance_name = instance_name.into();
        self
    }
    /// Validates SubscribeOptions before returning an instance of it.
    pub fn build(self) -> SdvResult<SubscribeOptions> {
        //TODO(b/379139605): Validate builder arguments
        Ok(SubscribeOptions {
            read_on_subscribe: self.read_on_subscribe,
            min_interval: self.min_interval,
            message_count: self.message_count,
            vm_name: self.vm_name,
            instance_name: self.instance_name,
        })
    }
}

/// ClientAttributes
#[derive(Debug, Clone)]
pub struct ClientAttributes {
    /// name of server to call
    server_name: UnitName,
    /// client options
    options: ClientOptions,
}

impl ClientAttributes {
    /// returns a Builder for ClientAttributes
    pub fn builder() -> ClientAttributesBuilder {
        ClientAttributesBuilder::default()
    }
    /// name of server to call
    pub fn get_server_name(&self) -> &UnitName {
        &self.server_name
    }
    /// client options
    pub fn get_client_options(&self) -> &ClientOptions {
        &self.options
    }
}

#[derive(Clone, Default, Debug)]
pub struct ClientAttributesBuilder {
    server_name: Option<UnitName>,
    client_options: ClientOptions,
}

impl ClientAttributesBuilder {
    /// set the UnitName of server to call
    pub fn server_name(mut self, server_name: UnitName) -> Self {
        self.server_name = Some(server_name);
        self
    }
    /// set client options
    pub fn client_options(mut self, client_options: ClientOptions) -> Self {
        self.client_options = client_options;
        self
    }
    /// Validates ClientAttributes before returning an instance of it.
    pub fn build(self) -> SdvResult<ClientAttributes> {
        //TODO(b/379139605): Validate builder arguments
        let Some(server_name) = self.server_name else {
            return Err(SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                "server_name must be set",
            ));
        };
        Ok(ClientAttributes { server_name, options: self.client_options })
    }
}

/// ClientOptions
#[derive(Debug, Default, Clone)]
pub struct ClientOptions {
    /// timeout in seconds
    timeout_in_sec: u32,
    /// maximum accepted response receiving size, in bytes
    max_response_size: Option<usize>,
}

impl ClientOptions {
    /// returns a Builder for ClientOptions
    pub fn builder() -> ClientOptionsBuilder {
        ClientOptionsBuilder::default()
    }
    /// timeout in seconds
    pub fn get_timeout_in_sec(&self) -> u32 {
        self.timeout_in_sec
    }
    /// maximum accepted response receiving size, in bytes
    pub fn get_max_response_size(&self) -> Option<usize> {
        self.max_response_size
    }
}

#[derive(Clone, Default, Debug)]
pub struct ClientOptionsBuilder {
    timeout_in_sec: u32,
    /// maximum accepted response receiving size, in bytes
    max_response_size: Option<usize>,
}

impl ClientOptionsBuilder {
    /// set the timeout in seconds
    pub fn timeout_in_sec(mut self, timeout_in_sec: u32) -> Self {
        self.timeout_in_sec = timeout_in_sec;
        self
    }

    /// set the maximum accepted response receiving size, in bytes
    pub fn max_response_size(mut self, max_response_size: Option<usize>) -> Self {
        self.max_response_size = max_response_size;
        self
    }

    /// Validates ClientOptions before returning an instance of it.
    pub fn build(self) -> SdvResult<ClientOptions> {
        //TODO(b/379139605): Validate builder arguments
        Ok(ClientOptions {
            timeout_in_sec: self.timeout_in_sec,
            max_response_size: self.max_response_size,
        })
    }
}

/// ServerAttributes
#[derive(Debug, Clone)]
pub struct ServerAttributes {
    /// server service_unit_name
    server_name: String,
    /// TypeName
    type_name: TypeName,
    /// server options
    options: ServerOptions,
}

impl ServerAttributes {
    /// returns a Builder for ServerAttributes
    pub fn builder() -> ServerAttributesBuilder {
        ServerAttributesBuilder::default()
    }
    /// server service_unit_name
    pub fn get_server_name(&self) -> &str {
        &self.server_name
    }
    /// TypeName of the service interface
    pub fn get_type_name(&self) -> &TypeName {
        &self.type_name
    }
    /// server options
    pub fn get_server_options(&self) -> &ServerOptions {
        &self.options
    }
}

#[derive(Clone, Default, Debug)]
pub struct ServerAttributesBuilder {
    server_name: String,
    type_name: Option<TypeName>,
    server_options: ServerOptions,
}

impl ServerAttributesBuilder {
    /// set the server service_unit_name
    pub fn server_name(mut self, server_name: impl Into<String>) -> Self {
        self.server_name = server_name.into();
        self
    }
    /// set the TypeName of the service interface
    pub fn type_name(mut self, type_name: TypeName) -> Self {
        self.type_name = Some(type_name);
        self
    }

    /// set server options
    pub fn server_options(mut self, server_options: ServerOptions) -> Self {
        self.server_options = server_options;
        self
    }
    /// Validates ServerAttributes before returning an instance of it.
    pub fn build(self) -> SdvResult<ServerAttributes> {
        //TODO(b/379139605): Validate builder arguments
        let Some(type_name) = self.type_name else {
            return Err(SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                "type_name must be set",
            ));
        };
        Ok(ServerAttributes {
            server_name: self.server_name,
            type_name,
            options: self.server_options,
        })
    }
}

/// ServerOptions
#[derive(Debug, Default, Clone)]
pub struct ServerOptions {
    /// timeout in seconds
    timeout_in_sec: u32,
    /// maximum accepted request size, in bytes
    max_request_size: Option<usize>,
}

impl ServerOptions {
    /// returns a Builder for ServerOptions
    pub fn builder() -> ServerOptionsBuilder {
        ServerOptionsBuilder::default()
    }
    /// timeout in seconds
    pub fn get_timeout_in_sec(&self) -> u32 {
        self.timeout_in_sec
    }
    /// maximum accepted request size, in bytes
    pub fn get_max_request_size(&self) -> Option<usize> {
        self.max_request_size
    }
}

/// ServerOptions
#[derive(Debug, Default, Clone)]
pub struct ServerOptionsBuilder {
    /// timeout in seconds
    timeout_in_sec: u32,
    /// maximum accepted request size, in bytes
    max_request_size: Option<usize>,
}

impl ServerOptionsBuilder {
    /// set the timeout in sec
    pub fn timeout_in_sec(mut self, timeout_in_sec: u32) -> Self {
        self.timeout_in_sec = timeout_in_sec;
        self
    }

    /// set the maximum accepted request size, in bytes
    pub fn max_request_size(mut self, max_request_size: Option<usize>) -> Self {
        self.max_request_size = max_request_size;
        self
    }

    /// Validates ServerOptions before returning an instance of it.
    pub fn build(self) -> SdvResult<ServerOptions> {
        //TODO(b/379139605): Validate builder arguments
        Ok(ServerOptions {
            timeout_in_sec: self.timeout_in_sec,
            max_request_size: self.max_request_size,
        })
    }
}

/// Represents the availability status of a resource or system.
#[derive(Clone, Eq, Hash, PartialEq, Debug)]
pub enum Availability {
    /// Indicates the resource is currently available for use.
    Available,
    /// Indicates the resource is currently unavailable.
    Unavailable,
}

#[cfg(test)]
mod test {
    use super::*;
    use sdv_proto_reflection_rs::message_metadata;
    #[test]
    fn test_unit_name_builder() {
        let vm_name = "random_vm";
        let package_name = "rand.pgk.name";
        let bundle_name = "BundleName";
        let service_unit_name = "some-service-unit";
        let instance_name = "test-instance";

        let result = UnitName::builder()
            .vm_name(vm_name)
            .package_name(package_name)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .instance_name(instance_name)
            .build();
        assert!(result.is_ok());
        let unit_name = result.unwrap();
        assert_eq!(unit_name.get_vm_name(), vm_name);
        assert_eq!(unit_name.get_package_name(), package_name);
        assert_eq!(unit_name.get_bundle_name(), bundle_name);
        assert_eq!(unit_name.get_service_unit_name(), service_unit_name);
        assert_eq!(unit_name.get_instance_name(), instance_name);

        // Explicitly test the Display trait implementation
        let expected_display = format!("{}.{}", package_name, service_unit_name);
        assert_eq!(format!("{}", unit_name), expected_display);
    }

    #[test]
    fn test_type_name_builder() {
        let package_name = "rand.pgk.name";
        let bundle_name = "BundleName";
        let type_name_name = "RandTypeName";

        let result = TypeName::builder()
            .package_name(package_name)
            .bundle_name(bundle_name)
            .type_name(type_name_name)
            .build();
        assert!(result.is_ok());
        let type_name = result.unwrap();
        assert_eq!(type_name.get_package_name(), package_name);
        assert_eq!(type_name.get_bundle_name(), bundle_name);
        assert_eq!(type_name.get_type_name(), type_name_name);

        // Explicitly test the Display trait implementation
        let expected_display = format!("{}.{}", package_name, type_name_name);
        assert_eq!(format!("{}", type_name), expected_display);
    }

    #[test]
    fn test_publisher_attributes_builder_ok() {
        let publication_name = "some-service-unit";
        let package_name = "rand.pgk.name";
        let type_name = TypeName::builder().package_name(package_name).build().unwrap();
        let metadata = PublisherMetadata::builder().is_multipub(true).build().unwrap();

        let result = PublisherAttributes::builder()
            .publication_name(publication_name)
            .type_name(type_name)
            .metadata(metadata)
            .build();
        assert!(result.is_ok());
        let publisher_attributes = result.unwrap();
        assert_eq!(publisher_attributes.get_publication_name(), publication_name);
        assert_eq!(publisher_attributes.get_type_name().get_package_name(), package_name);
        assert!(publisher_attributes.get_metadata().is_multipub());
    }

    #[test]
    fn test_publisher_attributes_builder_error() {
        let publication_name = "some-service-unit";
        let metadata = PublisherMetadata::builder().is_multipub(true).build().unwrap();

        let result = PublisherAttributes::builder()
            .publication_name(publication_name)
            .metadata(metadata)
            .build();

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_publisher_metadata_builder() {
        let message_size: u32 = 321;
        let message_count: u32 = 923;
        let message_metadata = message_metadata::MessageMetadata::default();

        let result = PublisherMetadata::builder()
            .message_size(message_size)
            .message_count(message_count)
            .is_multipub(true)
            .message_metadata(message_metadata)
            .message_metadata_required(true)
            .build();
        assert!(result.is_ok());
        let publisher_metadata = result.unwrap();
        assert_eq!(publisher_metadata.get_message_size(), message_size);
        assert_eq!(publisher_metadata.get_message_count(), message_count);
        assert!(publisher_metadata.is_multipub());
        assert_eq!(
            publisher_metadata.get_message_metadata(),
            &sdv_proto_reflection_rs::message_metadata::MessageMetadata::default()
        );
        assert!(publisher_metadata.is_message_metadata_required());
    }

    #[test]
    fn test_subscribe_options_builder() {
        let min_interval = Duration::from_secs(121);
        let message_count: u32 = 292;
        let vm_name = "testname";
        let test_instance = "test-instance";

        let result = SubscribeOptions::builder()
            .read_on_subscribe(true)
            .min_interval(min_interval)
            .message_count(message_count)
            .vm_name(vm_name)
            .instance_name(test_instance)
            .build();
        assert!(result.is_ok());
        let subscribe_options = result.unwrap();
        assert!(subscribe_options.read_on_subscribe());
        assert_eq!(subscribe_options.get_min_interval(), min_interval);
        assert_eq!(subscribe_options.get_message_count(), message_count);
        assert_eq!(subscribe_options.get_vm_name(), vm_name);
        assert_eq!(subscribe_options.get_instance_name(), test_instance);
    }

    #[test]
    fn test_client_attributes_builder_ok() {
        let service_unit_name = "some-service-unit";
        let timeout_in_sec: u32 = 948;
        let server_name = UnitName::builder().service_unit_name(service_unit_name).build().unwrap();
        let client_options =
            ClientOptions::builder().timeout_in_sec(timeout_in_sec).build().unwrap();

        let result = ClientAttributes::builder()
            .server_name(server_name)
            .client_options(client_options)
            .build();
        assert!(result.is_ok());
        let client_attributes = result.unwrap();
        assert_eq!(client_attributes.get_server_name().get_service_unit_name(), service_unit_name);
        assert_eq!(client_attributes.get_client_options().get_timeout_in_sec(), timeout_in_sec);
    }

    #[test]
    fn test_client_attributes_builder_error() {
        let timeout_in_sec: u32 = 948;
        let client_options =
            ClientOptions::builder().timeout_in_sec(timeout_in_sec).build().unwrap();

        let result = ClientAttributes::builder().client_options(client_options).build();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_client_options_builder() {
        let timeout_in_sec: u32 = 948;

        let result = ClientOptions::builder()
            .timeout_in_sec(timeout_in_sec)
            .max_response_size(Some(93901))
            .build();
        assert!(result.is_ok());
        let client_options = result.unwrap();
        assert_eq!(client_options.get_timeout_in_sec(), timeout_in_sec);
        assert_eq!(client_options.get_max_response_size(), Some(93901));
    }

    #[test]
    fn test_server_attributes_builder_ok() {
        let server_name = "some-service-unit";
        let package_name = "rand.pgk.name";
        let type_name = TypeName::builder().package_name(package_name).build().unwrap();
        let timeout_in_sec: u32 = 948;
        let server_options =
            ServerOptions::builder().timeout_in_sec(timeout_in_sec).build().unwrap();

        let result = ServerAttributes::builder()
            .server_name(server_name)
            .type_name(type_name)
            .server_options(server_options)
            .build();
        assert!(result.is_ok());
        let server_attributes = result.unwrap();
        assert_eq!(server_attributes.get_server_name(), server_name);
        assert_eq!(server_attributes.get_type_name().get_package_name(), package_name);
        assert_eq!(server_attributes.get_server_options().get_timeout_in_sec(), timeout_in_sec);
    }

    #[test]
    fn test_server_attributes_builder_error() {
        let server_name = "some-service-unit";
        let timeout_in_sec: u32 = 948;
        let server_options =
            ServerOptions::builder().timeout_in_sec(timeout_in_sec).build().unwrap();

        let result = ServerAttributes::builder()
            .server_name(server_name)
            .server_options(server_options)
            .build();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_server_options_builder() {
        let result =
            ServerOptions::builder().timeout_in_sec(487236).max_request_size(Some(93947)).build();
        assert!(result.is_ok());
        let server_options = result.unwrap();
        assert_eq!(server_options.get_timeout_in_sec(), 487236);
        assert_eq!(server_options.get_max_request_size(), Some(93947));
    }
}
