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

use std::sync::Arc;

use google_sdv_mesh_status::aidl::google::sdv::mesh::IMeshStatus::{
    BnMeshStatus, BpMeshStatus, IMeshStatus, IMeshStatusAsyncServer,
};

use google_sdv_mesh_status::aidl::google::sdv::mesh::PeerConnectionStatus::{
    ConnectionStatus::ConnectionStatus, DisconnectionReason::DisconnectionReason,
    PeerConnectionStatus,
};

use anyhow::Context;
use async_trait::async_trait;
use google_sdv_mesh_provisioning::binder;
use tracing::instrument;

use crate::mesh::manager::{MeshManager, PeerStatus};

fn into_connection_status(status: PeerStatus) -> ConnectionStatus {
    match status {
        PeerStatus::Connected => ConnectionStatus::CONNECTED,
        _ => ConnectionStatus::DISCONNECTED,
    }
}

pub(crate) struct MeshStatusService {
    mesh_manager: Arc<MeshManager>,
}

impl binder::Interface for MeshStatusService {}

#[async_trait]
#[allow(non_snake_case)] // generated AIDL interface
impl IMeshStatusAsyncServer for MeshStatusService {
    #[instrument(name = "sd::IMeshStatus::isComplete", skip_all)]
    async fn isComplete(&self) -> binder::Result<bool> {
        let peers_status: Vec<(String, PeerStatus)> = self.mesh_manager.get_peers_status();
        Ok(peers_status.into_iter().all(|(_, status)| status == PeerStatus::Connected))
    }

    #[instrument(name = "sd::IMeshStatus::getPeersConnectionStatus", skip_all)]
    async fn getPeersConnectionStatus(&self) -> binder::Result<Vec<PeerConnectionStatus>> {
        let peers_status: Vec<(String, PeerStatus)> = self.mesh_manager.get_peers_status();
        Ok(peers_status
            .into_iter()
            .map(|(vm_name, status)| PeerConnectionStatus {
                vmName: vm_name.to_string(),
                connectionStatus: into_connection_status(status),
                // TODO: b/394516452 - populate with the actual reason
                disconnectionReason: DisconnectionReason::UNKNOWN,
            })
            .collect())
    }
}

impl MeshStatusService {
    pub(crate) fn create_and_register<R>(
        mesh_manager: Arc<MeshManager>,
        binder_runtime: R,
        features: binder::BinderFeatures,
    ) -> anyhow::Result<()>
    where
        R: binder::binder_impl::BinderAsyncRuntime + Send + Sync + 'static,
    {
        let service_impl = MeshStatusService { mesh_manager };

        let descriptor = <BpMeshStatus as IMeshStatus>::get_descriptor().to_owned() + "/default";

        let service = BnMeshStatus::new_async_binder(service_impl, binder_runtime, features);

        binder::add_service(&descriptor, service.as_binder())
            .with_context(|| format!("Failed to register {descriptor}"))?;

        Ok(())
    }
}
