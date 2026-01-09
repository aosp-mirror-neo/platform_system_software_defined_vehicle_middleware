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
use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinV2;
use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::EcPublicKey::EcPublicKey;

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
    metadata::{
        ApplicationMetadata::ApplicationMetadata,
        ApplicationMetadata::TypedMetadata::TypedMetadata::ServerUnit,
        ServerUnitMetadata::ServerUnitMetadata, TransportMetadata::TransportMetadata,
    },
    AccessControl::AccessControl,
    UnitType::UnitType,
};
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::ServiceUnitDefinition;

use binder::{binder_impl::Binder, Interface};

use test_agents::{
    get_identity_agent, get_registration_agent, get_service_discovery_agent,
    get_transport_support_agent_service,
};

#[test]
fn add_service_flow_test_v2() {
    let identity_registry_v2 = get_identity_agent();

    let publickey = EcPublicKey { keyValue: *b"01234567890123456789012345678912" };

    let fqin = ServiceFqinV2 {
        sdvVmName: "".to_string(),
        sdvPackageName: "com.android.sdv.service_discovery.test".to_string(),
        serviceBundleName: "AddServiceFlowTestV2".to_string(),
        serviceInstanceName: "default".to_string(),
    };

    // 1.) Create Identity in app
    let link_to_death = Binder::new(()).as_binder();
    let service_identity =
        identity_registry_v2.createIdentity(&publickey, &fqin, &link_to_death).unwrap();

    // 2.) Register service unit
    let registration_agent = get_registration_agent();
    let unit_type = UnitType {
        sdvPackageName: fqin.sdvPackageName.clone(),
        serviceBundleName: fqin.serviceBundleName.clone(),
        typeName: "Default".to_string(),
    };
    let acl: AccessControl = AccessControl { ..Default::default() };
    let app_metadata = ApplicationMetadata {
        typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
        valueHolder: "test-value:1".into(),
    };
    let result = registration_agent.registerServiceUnit(
        "service-unit-name",
        &unit_type,
        &acl,
        &app_metadata,
    );
    let token = result.unwrap();

    let service_discovery_agent = get_service_discovery_agent();
    let sd_list = service_discovery_agent.listServiceUnitsByType(&unit_type).unwrap();

    // until agent added metadata, the service shouldn't appear in listService*()
    assert!(sd_list.is_empty());

    // 3.) app pass token to agent via their own aidl interface (not covered here)

    // 4.) Transport agent can get more info from service definition from the token
    let transport_support_agent = get_transport_support_agent_service();
    let result = transport_support_agent.getServiceUnitForToken(&token);
    assert!(result.is_ok());
    let discovered_service_unit_definition = result.unwrap();
    assert!(service_identity == discovered_service_unit_definition.identity);

    // 5.) Transport agent add the metadata needed for transport, this information is not visible
    //   to app, only for agent
    let transport_metadata = TransportMetadata { value_holder: "192.168.0.1:8080".into() };
    let result = transport_support_agent.setAgentMetadata(&token, &transport_metadata);
    assert!(result.is_ok());

    // 6.) services should have been sync across the mesh now
    let result = service_discovery_agent.listServiceUnitsByType(&unit_type);
    assert!(result.is_ok());
    let sd_list = result.unwrap();

    assert!(!sd_list.is_empty());

    let result = service_discovery_agent
        .getServiceUnit(&sd_list[0].identity.name, &sd_list[0].serviceUnitName);
    assert!(result.is_ok());

    // 7.) agent can also retrieve the agent metadata (suspose to be used bu remote side)
    let mut fetched_sud = ServiceUnitDefinition::default();
    let mut fetched_tm = TransportMetadata::default();
    let result = transport_support_agent.getAgentMetadata(
        service_identity.sid,
        "service-unit-name",
        &mut fetched_sud,
        &mut fetched_tm,
    );
    assert!(result.is_ok());
    assert!(fetched_tm.value_holder == transport_metadata.value_holder);
    assert_eq!(discovered_service_unit_definition, fetched_sud);
}
