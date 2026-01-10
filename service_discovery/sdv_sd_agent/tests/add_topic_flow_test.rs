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
// TODO remove later
#![allow(unused)]

use google_sdv_sd_aidl::aidl::google::sdv::sd::IAgentManager::{BpAgentManager, IAgentManager};
use google_sdv_sd_aidl::aidl::google::sdv::sd::IIdentityManager::{
    BpIdentityManager, IIdentityManager,
};
use google_sdv_sd_aidl::aidl::google::sdv::sd::IServiceManager::{
    BpServiceManager, IServiceManager,
};
use google_sdv_sd_aidl::aidl::google::sdv::sd::ServiceDefinition::TypedMetadata::TypedMetadata as ServiceTypedMetadata;
use google_sdv_sd_aidl::aidl::google::sdv::sd::TopicDefinition::TypedMetadata::TypedMetadata as TopicTypedMetadata;
use google_sdv_sd_common_aidl::aidl::google::sdv::sd_common::GenericMetadata::GenericMetadata;
use google_sdv_sd_common_aidl::aidl::google::sdv::sd_common::ServiceFqin::ServiceFqin;
use google_sdv_sd_common_aidl::aidl::google::sdv::sd_common::ServiceIdentity::PublicKey::PublicKey;

fn get_service_manager() -> binder::Strong<dyn IServiceManager> {
    let descriptor =
        <BpServiceManager as IServiceManager>::get_descriptor().to_owned() + "/default";
    binder::get_interface(&descriptor).unwrap()
}

fn get_identity_service() -> binder::Strong<dyn IIdentityManager> {
    let descriptor =
        <BpIdentityManager as IIdentityManager>::get_descriptor().to_owned() + "/default";
    binder::get_interface(&descriptor).unwrap()
}

fn get_agent_service() -> binder::Strong<dyn IAgentManager> {
    let descriptor = <BpAgentManager as IAgentManager>::get_descriptor().to_owned() + "/default";
    binder::get_interface(&descriptor).unwrap()
}

#[test]
fn add_topic_flow_test() {
    let identity = get_identity_service();

    let publickey = PublicKey { value: *b"01234567890123456789012345678912" };

    let fqin = ServiceFqin {
        vm_name: "".to_string(),
        package_name: "com.android.sdv.service_discovery.test".to_string(),
        service_name: "AddTopicFlowTest".to_string(),
        instance_name: "default".to_string(),
    };

    // 1.) Create Identity in app
    let result = identity.createIdentity(&publickey, &fqin);
    assert!(result.is_ok());
    let service_identity = result.unwrap();

    let service = get_service_manager();
    let metadata = TopicTypedMetadata { version: 1 };
    let app_metadata = GenericMetadata { value_holder: "test-value:1".as_bytes().to_vec() };

    // 2.) Register service in app
    let result = service.registerTopic("topic1", &metadata, &app_metadata);
    assert!(result.is_ok());
    let token = result.unwrap();

    // verify listTopicsByName API returns no topics
    let result = service.listTopicsByName(
        "topic1",
        None,
        Some(fqin.package_name.as_str()),
        Some(fqin.service_name.as_str()),
        Some(fqin.instance_name.as_str()),
    );
    assert!(result.is_ok());
    let td_list = result.unwrap();

    // until agent added metadata, the topic shouldn't appear in list_topic
    assert!(td_list.is_empty());

    // verify listTopicsByService API returns no topics
    let result = service.listTopicsByService(
        None,
        fqin.package_name.as_str(),
        fqin.service_name.as_str(),
        Some(fqin.instance_name.as_str()),
    );
    assert!(result.is_ok());
    let td_list = result.unwrap();

    // until agent added metadata, the topic shouldn't appear in topic list
    assert!(td_list.is_empty());

    // 3.) app pass token to agent via their own aidl interface (not covered here)

    // 4.) Transport agent can get more info from topic definition from the token
    let agent_service = get_agent_service();
    let result = agent_service.getTopicForToken(&token);
    assert!(result.is_ok());
    let sd = result.unwrap();
    assert!(service_identity == sd.identity);

    // 5.) Transport agent add the metadata needed for transport, this information is not visible
    //   to app, only for agent
    let agent_metadata = GenericMetadata { value_holder: "192.168.0.1:8080".as_bytes().to_vec() };
    let result = agent_service.setAgentMetadata(&token, &agent_metadata);
    assert!(result.is_ok());

    // 6.) services should have been sync across the mesh now
    let result = service.listTopicsByName(
        "topic1",
        None,
        Some(fqin.package_name.as_str()),
        Some(fqin.service_name.as_str()),
        Some(fqin.instance_name.as_str()),
    );
    assert!(result.is_ok());
    let td_list = result.unwrap();

    // the topic should not be empty
    assert!(!td_list.is_empty());

    // just get first topic available
    let result = service.getTopic(&td_list[0].identity.fqin, "topic1");
    assert!(result.is_ok());

    // alternatively, clients can query all topics by service
    let result = service.listTopicsByService(
        None,
        fqin.package_name.as_str(),
        fqin.service_name.as_str(),
        Some(fqin.instance_name.as_str()),
    );
    assert!(result.is_ok());
    let td_list = result.unwrap();

    // the topic should not be empty
    assert!(!td_list.is_empty());

    // 7.) agent should be able to retrieve metadata
    let result = agent_service.getAgentMetadataForTopic(service_identity.sid, "topic1");
    assert!(result.is_ok());

    // 8.) App should be able to register multiple topics on same process id (no createIdentity
    //   need to be called again)
    let result = service.registerTopic("topic2", &metadata, &app_metadata);
    assert!(result.is_ok());
    let token = result.unwrap();
    let result = agent_service.setAgentMetadata(&token, &agent_metadata);
    assert!(result.is_ok());

    let result = service.getTopic(&fqin, "topic2");
    assert!(result.is_ok());
}
