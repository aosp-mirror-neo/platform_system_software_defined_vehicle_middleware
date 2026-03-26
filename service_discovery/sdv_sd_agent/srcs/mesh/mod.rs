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

//! SDV SD Mesh RPC service.

pub(crate) mod aidl_to_proto;
pub(crate) mod async_client;

#[allow(dead_code)]
pub(crate) mod manager;

pub(crate) mod proto_to_aidl;
pub(crate) mod service;

mod boot_mode;
pub(crate) use boot_mode::get_sdv_boot_mode;
pub(crate) use boot_mode::SdvBootMode;

pub(crate) mod custom_verifier;
pub(crate) mod dice_policy_checker;

mod uds_pubs;

pub(crate) mod vvm_config;
pub(crate) use vvm_config::VvmConfig;

mod dice_chains;
pub(crate) use dice_chains::{fetch_local_vm_uds_pubs, AuthenticatedCsr};

mod uds_pubs_provisioner;
pub(crate) use uds_pubs_provisioner::UdsPubsProvisioner;

mod uds_pubs_provisioning_service;
pub(crate) use uds_pubs_provisioning_service::{
    DisabledUdsPubsProvisioningService, UdsPubsProvisioningService,
};
mod mesh_status_service;
pub(crate) use mesh_status_service::MeshStatusService;

pub(crate) mod credentials_checker;
pub(crate) mod secure_world_dice_checker;

pub(crate) mod sd_certificate;
pub(crate) mod sd_certificate_claims;

mod peer_connection;
