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

use crate::mesh::manager::WatcherState;
use crate::power_state::SimplePowerState;
use async_trait::async_trait;
use binder::{BinderFeatures, DeathRecipient, IBinder, Strong};
use binder_tokio::{Tokio, TokioRuntime};
use google_sdv_vpm::aidl::google::sdv::vpm::IPowerStateClientApi::{
    BpPowerStateClientApi, IPowerStateClientApi, IPowerStateClientApiAsync,
};
use google_sdv_vpm::aidl::google::sdv::vpm::IPowerStateReportListener::{
    BnPowerStateReportListener, IPowerStateReportListenerAsyncServer,
};
use google_sdv_vpm::aidl::google::sdv::vpm::PowerStateReport::PowerStateReport;
use log::{debug, error, warn};
use std::sync::Arc;
use tokio::{
    sync::{broadcast, watch::Receiver, Notify},
    time::{sleep, Duration},
};

#[derive(Clone)]
struct PowerStateListener {
    report_tx: broadcast::Sender<SimplePowerState>,
    mesh_manager_watcher_state_rx: Receiver<WatcherState>,
}

impl binder::Interface for PowerStateListener {}

fn to_simple_power_state(power_state_report: PowerStateReport) -> SimplePowerState {
    if matches!(
        power_state_report,
        PowerStateReport::REPORT_UNSPECIFIED
            | PowerStateReport::POWER_OFF_EXIT
            | PowerStateReport::SUSPEND_TO_RAM_EXIT
            | PowerStateReport::SUSPEND_TO_DISK_EXIT
            | PowerStateReport::ON
            | PowerStateReport::SUSPEND_TO_RAM_ENTER
            | PowerStateReport::SUSPEND_TO_DISK_ENTER
            | PowerStateReport::WAIT_FOR_FINISH
            | PowerStateReport::SHUTDOWN_CANCELLED
    ) {
        SimplePowerState::On
    } else {
        SimplePowerState::Off
    }
}

#[async_trait]
impl IPowerStateReportListenerAsyncServer for PowerStateListener {
    async fn onPowerStateReportChanged(
        &self,
        power_state_report: PowerStateReport,
    ) -> binder::Result<()> {
        debug!("Received power state report: {power_state_report:?}");
        if let Err(e) = self.report_tx.send(to_simple_power_state(power_state_report)) {
            error!("Failed to propagate power state report: {e}");
            return Ok(());
        }

        if to_simple_power_state(power_state_report) == SimplePowerState::Off {
            // The power state change will only proceed after we return from this method.
            // Therefore waiting here ensures we properly leave the mesh before the system is
            // suspended or shut down.
            let mut watcher_state_rx = self.mesh_manager_watcher_state_rx.clone();
            debug!("Waiting for the MeshManager watcher thread to pause...");
            if let Err(e) = watcher_state_rx.wait_for(watcher_is_paused).await {
                error!("Failed to wait for MeshManager watcher state: {e}");
            };
        }

        debug!("Acking power state report: {power_state_report:?}");
        Ok(())
    }
}

fn watcher_is_paused(watcher_state: &WatcherState) -> bool {
    *watcher_state == WatcherState::Paused
}

const WAIT_FOR_INTERFACE_RETRY_DELAY: Duration = Duration::from_secs(1);
const SUBSCRIBE_RETRY_DELAY: Duration = Duration::from_secs(3);

fn get_power_state_service_name() -> String {
    <BpPowerStateClientApi as IPowerStateClientApi>::get_descriptor().to_owned() + "/default"
}

/// Waits indefinitely for the PowerState service.
async fn wait_for_power_state_service() -> Strong<dyn IPowerStateClientApiAsync<Tokio>> {
    let power_state_service_name = get_power_state_service_name();
    loop {
        debug!("Waiting for {power_state_service_name}...");
        let result = binder_tokio::wait_for_interface::<dyn IPowerStateClientApiAsync<Tokio>>(
            &power_state_service_name,
        )
        .await;
        match result {
            Ok(binder_service) => {
                debug!("Successfully connected to {power_state_service_name}");
                return binder_service;
            }
            Err(status_code) => {
                error!(
                    "Failed to connect to {power_state_service_name}: {status_code:?}. Retrying in {WAIT_FOR_INTERFACE_RETRY_DELAY:?}..."
                );
            }
        }
        sleep(WAIT_FOR_INTERFACE_RETRY_DELAY).await;
    }
}

async fn subscribe_to_power_state_reports(
    tokio_runtime: Arc<tokio::runtime::Runtime>,
    sender: broadcast::Sender<SimplePowerState>,
    mesh_manager_watcher_state_rx: Receiver<WatcherState>,
    service: Strong<dyn IPowerStateClientApiAsync<Tokio>>,
) {
    let listener = PowerStateListener { report_tx: sender.clone(), mesh_manager_watcher_state_rx };
    let listener_binder = BnPowerStateReportListener::new_async_binder(
        listener,
        TokioRuntime(tokio_runtime.clone()),
        BinderFeatures::default(),
    );

    let service_died_notify = Arc::new(Notify::new());
    let service_died_notifier = service_died_notify.clone();
    let mut death_recipient = DeathRecipient::new(move || {
        service_died_notifier.notify_one();
    });

    if let Err(e) = service.as_binder().link_to_death(&mut death_recipient) {
        error!("Failed to link_to_death for PowerStateClientApi: {e:?}.");
        return;
    }

    match service.subscribeToPowerStateReport(&listener_binder).await {
        Ok(report) => {
            debug!("Subscribed to power state reports. Initial report: {report:?}");
            if let Err(e) = sender.send(to_simple_power_state(report)) {
                error!("Failed to propagate initial power state report: {e}");
            }
            // Wait for the service to die
            service_died_notify.notified().await;
            warn!("PowerStateClientApi died.");
        }
        Err(e) => error!("Failed to subscribe to power state reports: {e:?}."),
    }

    // It will fail if the service is already dead. But do it anyway just in case. Just don't
    // bother reporting the probable error.
    let _ = service.as_binder().unlink_to_death(&mut death_recipient);
}

/// Listens to power state reports received from the VPM binder service and forwards them through
/// the given channel sender. This function will continuously try to connect to the VPM service
/// if the connection drops.
///
/// On power states where the MeshManager watcher should be paused (eg: suspend, shut down), it
/// will wait for that state to be reached before acking the power state notification coming from
/// the VPM service.
pub(crate) async fn run_power_state_reporter(
    tokio_runtime: Arc<tokio::runtime::Runtime>,
    sender: broadcast::Sender<SimplePowerState>,
    mesh_manager_watcher_state_rx: Receiver<WatcherState>,
) {
    loop {
        let service = wait_for_power_state_service().await;
        subscribe_to_power_state_reports(
            tokio_runtime.clone(),
            sender.clone(),
            mesh_manager_watcher_state_rx.clone(),
            service,
        )
        .await;
        debug!(
            "Retrying connection to PowerStateClientApi after a {SUBSCRIBE_RETRY_DELAY:?} delay..."
        );
        sleep(SUBSCRIBE_RETRY_DELAY).await;
    }
}
