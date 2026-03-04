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

use binder::{binder_impl::Binder, Interface};
use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinV2;
use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::EcPublicKey::EcPublicKey;
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
    metadata::{
        ApplicationMetadata::ApplicationMetadata,
        ApplicationMetadata::TypedMetadata::TypedMetadata::PublisherUnit,
        PublisherUnitMetadata::PublisherUnitMetadata, TransportMetadata::TransportMetadata,
    },
    AccessControl::AccessControl,
    UnitType::UnitType,
};
use test_agents::{
    get_identity_agent, get_registration_agent, get_service_discovery_agent,
    get_transport_support_agent_service,
};

#[test]
fn test_ensure_publisher_has_topic() {
    let identity_registry_v2 = get_identity_agent();

    let publickey = EcPublicKey { keyValue: *b"01234567890123456789012345678912" };

    let fqin = ServiceFqinV2 {
        sdvVmName: "".to_string(),
        sdvPackageName: "com.android.sdv.service_discovery.test".to_string(),
        serviceBundleName: "EnsurePublisherHasTopicTest".to_string(),
        serviceInstanceName: "default".to_string(),
    };

    // 1.) Create Identity in app
    let link_to_death = Binder::new(()).as_binder();
    let _service_identity =
        identity_registry_v2.createIdentity(&publickey, &fqin, &link_to_death).unwrap();

    // 2.) Register service unit
    let registration_agent = get_registration_agent();
    let unit_type = UnitType {
        sdvPackageName: fqin.sdvPackageName.clone(),
        serviceBundleName: fqin.serviceBundleName.clone(),
        typeName: "Default".to_string(),
    };
    let acl: AccessControl = AccessControl { ..Default::default() };
    // Register a publisher with an empty topic name.
    let app_metadata = ApplicationMetadata {
        typedMetadata: PublisherUnit(PublisherUnitMetadata {
            topic_name: "".to_string(),
            ..Default::default()
        }),
        valueHolder: "test-value:1".into(),
    };
    let service_unit_name = "test-publisher";
    let result =
        registration_agent.registerServiceUnit(service_unit_name, &unit_type, &acl, &app_metadata);
    let token = result.unwrap();

    // 3.) Transport agent sets metadata.
    let transport_support_agent = get_transport_support_agent_service();
    let transport_metadata = TransportMetadata { value_holder: "some_transport_data".into() };
    transport_support_agent.setAgentMetadata(&token, &transport_metadata).unwrap();

    // 4.) Fetch the service unit and verify the topic name.
    let service_discovery_agent = get_service_discovery_agent();
    let sd_list = service_discovery_agent.listServiceUnitsByType(&unit_type).unwrap();

    assert_eq!(sd_list.len(), 1, "Expected one service unit to be registered");
    let sud = &sd_list[0];
    assert_eq!(sud.serviceUnitName, service_unit_name);

    match &sud.appMetadata.typedMetadata {
        PublisherUnit(metadata) => {
            assert_eq!(
                metadata.topic_name, service_unit_name,
                "Topic name should be defaulted to service unit name"
            );
        }
        _ => panic!("Expected PublisherUnit metadata"),
    }
}
