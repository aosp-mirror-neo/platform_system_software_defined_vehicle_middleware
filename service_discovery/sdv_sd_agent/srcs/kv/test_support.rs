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

use crate::kv::ServiceRegistry;
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
    metadata::{
        ApplicationMetadata::{
            r#ApplicationMetadata, r#TypedMetadata::TypedMetadata as TypedMetadataAidl,
        },
        PublisherUnitMetadata::PublisherUnitMetadata,
        TransportMetadata::TransportMetadata,
    },
    AccessControl::AccessControl,
    UnitType::UnitType,
};

use google_sdv_identity::aidl::google::sdv::identity::{
    ProcessIdentifier::ProcessIdentifier, ServiceFqin::ServiceFqin as ServiceFqinV2,
    ServiceIdentity::ServiceIdentity as ServiceIdentityV2,
};

pub(crate) fn register_service_unit_for_test(
    service_registry: &ServiceRegistry,
    vm_name: &str,
    unit_type: &UnitType,
    process_identifier: &ProcessIdentifier,
    sid: i64,
) -> ServiceIdentityV2 {
    register_service_unit_for_test_topic(
        service_registry,
        vm_name,
        unit_type,
        process_identifier,
        sid,
        None,
    )
}

pub(crate) fn register_service_unit_for_test_topic(
    service_registry: &ServiceRegistry,
    vm_name: &str,
    unit_type: &UnitType,
    process_identifier: &ProcessIdentifier,
    sid: i64,
    topic_name: Option<String>,
) -> ServiceIdentityV2 {
    let service_identity = ServiceIdentityV2 {
        sid,
        name: ServiceFqinV2 {
            sdvVmName: vm_name.to_string(),
            serviceBundleName: unit_type.serviceBundleName.clone(),
            sdvPackageName: unit_type.sdvPackageName.clone(),
            serviceInstanceName: "instance_1".to_string(),
        },
        ..Default::default()
    };

    let _ = service_registry
        .register_identity_v2(service_identity.clone(), None, process_identifier)
        .unwrap();

    let acl = AccessControl { ..Default::default() };
    let mut app_metadata = ApplicationMetadata {
        typedMetadata: TypedMetadataAidl::PublisherUnit(PublisherUnitMetadata {
            ..Default::default()
        }),
        ..Default::default()
    };
    if let Some(topic_name) = topic_name {
        app_metadata.typedMetadata = TypedMetadataAidl::PublisherUnit(PublisherUnitMetadata {
            topic_name,
            ..Default::default()
        });
    }

    // Because the identity is known we should be able to register a service.
    let token = service_registry
        .preregister_service_unit(
            "service_unit_name",
            unit_type,
            &acl,
            &app_metadata,
            process_identifier,
        )
        .unwrap();

    // Check that the service will not be exposed before transport metadata is set.
    let unit_types = service_registry.list_service_units_by_unit_type(unit_type);
    assert!(unit_types.is_empty());

    let transport_metadata = TransportMetadata { value_holder: "192.168.0.1:8080".into() };

    service_registry
        .finalize_service_unit_registration(&token.token.unwrap(), transport_metadata)
        .unwrap();

    service_identity
}

pub(crate) fn get_default_test_unit_type() -> UnitType {
    UnitType {
        sdvPackageName: "package_name".to_string(),
        serviceBundleName: "".to_string(),
        typeName: "type_name".to_string(),
    }
}
