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
//! SDV Agents accessors for integration tests.

use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::{
    IServiceRegistrationAgent::BpServiceRegistrationAgent,
    IServiceRegistrationAgent::IServiceRegistrationAgent,
    ITransportSupportAgent::BpTransportSupportAgent,
    ITransportSupportAgent::ITransportSupportAgent,
};

use google_sdv_identity::aidl::google::sdv::identity::IIdentityAgent::{
    BpIdentityAgent, IIdentityAgent,
};

use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent::{
    BpServiceDiscoveryAgent, IServiceDiscoveryAgent};

/// Returns a Binder proxy for Identity interface.
pub fn get_identity_agent() -> binder::Strong<dyn IIdentityAgent> {
    let descriptor = <BpIdentityAgent as IIdentityAgent>::get_descriptor().to_owned() + "/default";
    binder::get_interface(&descriptor).unwrap()
}

/// Returns a Binder proxy for Transport Support interface.
pub fn get_transport_support_agent_service() -> binder::Strong<dyn ITransportSupportAgent> {
    let descriptor = <BpTransportSupportAgent as ITransportSupportAgent>::get_descriptor()
        .to_owned()
        + "/default";
    binder::get_interface(&descriptor).unwrap()
}

/// Returns a Binder proxy for Service Registration interface.
pub fn get_registration_agent() -> binder::Strong<dyn IServiceRegistrationAgent> {
    let descriptor = <BpServiceRegistrationAgent as IServiceRegistrationAgent>::get_descriptor()
        .to_owned()
        + "/default";
    binder::get_interface(&descriptor).unwrap()
}

/// Returns a Binder proxy for Service Discovery interface.
pub fn get_service_discovery_agent() -> binder::Strong<dyn IServiceDiscoveryAgent> {
    let descriptor = <BpServiceDiscoveryAgent as IServiceDiscoveryAgent>::get_descriptor()
        .to_owned()
        + "/default";
    binder::get_interface(&descriptor).unwrap()
}
