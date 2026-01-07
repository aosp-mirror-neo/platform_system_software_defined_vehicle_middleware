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

use super::uds_pubs_provisioner::UdsPubsProvisioner;
use google_sdv_mesh_provisioning::aidl::google::sdv::mesh::provisioning::IUdsPubsProvisioner::{
    BnUdsPubsProvisioner, BpUdsPubsProvisioner, IUdsPubsProvisioner, IUdsPubsProvisionerAsyncServer,
};
use google_sdv_mesh_provisioning::aidl::google::sdv::mesh::provisioning::IWriteCallback::{
    IWriteCallback,
    //
};
use google_sdv_mesh_provisioning::binder;

use anyhow::Context;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

pub(crate) struct UdsPubsProvisioningService {
    provisioner: Arc<Mutex<UdsPubsProvisioner>>,
}

impl binder::Interface for UdsPubsProvisioningService {}

#[async_trait]
impl IUdsPubsProvisionerAsyncServer for UdsPubsProvisioningService {
    async fn writeUdsPubs(
        &self,
        listener: &binder::Strong<dyn IWriteCallback>,
    ) -> binder::Result<()> {
        let mut guarded_provisioner = self.provisioner.lock().unwrap();
        match guarded_provisioner.write_uds_pubs_when_ready(listener.clone()) {
            Ok(()) => Ok(()),
            Err(error_code) => listener.onFailure(error_code),
        }
    }
}

impl UdsPubsProvisioningService {
    pub(crate) fn create_and_register<R>(
        provisioner: Arc<Mutex<UdsPubsProvisioner>>,
        binder_runtime: R,
        features: binder::BinderFeatures,
    ) -> anyhow::Result<()>
    where
        R: binder::binder_impl::BinderAsyncRuntime + Send + Sync + 'static,
    {
        let service_impl = UdsPubsProvisioningService { provisioner };

        let descriptor =
            <BpUdsPubsProvisioner as IUdsPubsProvisioner>::get_descriptor().to_owned() + "/default";

        let service =
            BnUdsPubsProvisioner::new_async_binder(service_impl, binder_runtime, features);

        binder::add_service(&descriptor, service.as_binder())
            .with_context(|| format!("Failed to register {descriptor}"))?;

        Ok(())
    }
}
