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
use log::{debug, error, info, warn};
use std::sync::Arc;
use tokio::{
    sync::{broadcast, watch::Receiver, Notify},
    time::{sleep, Duration},
};
use google_sdv_privileged_gatewaynetworking::aidl::google::sdv::privileged::gatewaynetworking::CarPowerState::CarPowerState;
use google_sdv_privileged_gatewaynetworking::aidl::google::sdv::privileged::gatewaynetworking::INetworkStateListener::{
  BnNetworkStateListener, INetworkStateListenerAsyncServer,
};
use google_sdv_privileged_gatewaynetworking::aidl::google::sdv::privileged::gatewaynetworking::IPrivilegedGatewayNetworking::{
  BpPrivilegedGatewayNetworking, IPrivilegedGatewayNetworking, IPrivilegedGatewayNetworkingAsync,
};
use std::ffi::{c_int, c_longlong};

#[link(name = "android_net")]
unsafe extern "C" {
    pub fn android_setprocnetwork(network_handle: c_longlong) -> c_int;
}

fn setprocnetwork(network_handle: i64) -> Result<(), i32> {
    // SAFETY:
    // Systemcall-like call which only involes primitive types, and is
    // safe to call from multiple threads.
    match unsafe { android_setprocnetwork(network_handle) } {
        0 => Ok(()),
        err => Err(err),
    }
}

fn get_gateway_networking_service_name() -> String {
    <BpPrivilegedGatewayNetworking as IPrivilegedGatewayNetworking>::get_descriptor().to_owned()
        + "/default"
}

const WAIT_FOR_INTERFACE_RETRY_DELAY: Duration = Duration::from_secs(1);

async fn wait_for_gateway_networking_service(
) -> Strong<dyn IPrivilegedGatewayNetworkingAsync<Tokio>> {
    let gateway_networking_service_name = get_gateway_networking_service_name();
    loop {
        debug!("Waiting for gateway networking service...");
        let result =
            binder_tokio::wait_for_interface::<dyn IPrivilegedGatewayNetworkingAsync<Tokio>>(
                &gateway_networking_service_name,
            )
            .await;
        match result {
            Ok(service) => {
                info!("Connected to {gateway_networking_service_name}");
                return service;
            }
            Err(code) => {
                error!("Failed to connect to {gateway_networking_service_name}: {code}");
            }
        }
        sleep(WAIT_FOR_INTERFACE_RETRY_DELAY).await;
    }
}

#[derive(Clone)]
struct NetworkStateListener {
    report_tx: broadcast::Sender<SimplePowerState>,
}

impl binder::Interface for NetworkStateListener {}

fn send_power_report_ignore_error(
    report_tx: &broadcast::Sender<SimplePowerState>,
    report: SimplePowerState,
) {
    if let Err(e) = report_tx.send(report) {
        error!("Failed to send {report:?} signal: {e}");
    }
}

#[async_trait]
impl INetworkStateListenerAsyncServer for NetworkStateListener {
    async fn onSdvNetworkHandleChanged(&self, sdv_network_handle: i64) -> binder::Result<()> {
        debug!("Received network handle {sdv_network_handle}");
        if sdv_network_handle == 0 {
            send_power_report_ignore_error(&self.report_tx, SimplePowerState::Off);
        } else {
            // Case 1: The original network handle is 0, which means the network is disconnected. In this case, the first off should be no-op.
            // Case 2: The network handle is changed from X to Y, which means the network is
            // disconnected and reconnected.
            match setprocnetwork(sdv_network_handle) {
                Ok(_) => {
                    info!("Set process network handle to {sdv_network_handle:x}");
                }
                Err(error_code) => {
                    error!(
                        "Failed to set process network handle \
                    to {sdv_network_handle:x}: {error_code}"
                    );
                }
            }
            // Network handle only applies to new sockets.
            // MeshServiceController listens on this to start / stop the service.
            // Turn off and turn on the server to recreate the server socket with new network handle.
            send_power_report_ignore_error(&self.report_tx, SimplePowerState::Off);
            send_power_report_ignore_error(&self.report_tx, SimplePowerState::On);
        }
        Ok(())
    }

    async fn onCarPowerStateChanged(&self, power_state: CarPowerState) -> binder::Result<()> {
        debug!("Received power state: {power_state:?}");
        // Refer to sdv_gateway/libsdvgateway/CarPowerStateReportSupport.cpp and
        // data_tunnel/src/service.cpp for the mapping.
        let signal_to_send = match power_state {
            CarPowerState::INVALID => None,
            CarPowerState::WAIT_FOR_VHAL => None,
            CarPowerState::SUSPEND_ENTER => None,
            CarPowerState::SUSPEND_EXIT => Some(SimplePowerState::On),
            CarPowerState::SHUTDOWN_ENTER => None,
            CarPowerState::ON => Some(SimplePowerState::On),
            CarPowerState::SHUTDOWN_PREPARE => None,
            CarPowerState::SHUTDOWN_CANCELLED => None,
            CarPowerState::HIBERNATION_ENTER => None,
            CarPowerState::HIBERNATION_EXIT => Some(SimplePowerState::On),
            CarPowerState::PRE_SHUTDOWN_PREPARE => None,
            CarPowerState::POST_SUSPEND_ENTER => Some(SimplePowerState::Off),
            CarPowerState::POST_SHUTDOWN_ENTER => None,
            CarPowerState::POST_HIBERNATION_ENTER => Some(SimplePowerState::Off),
            CarPowerState(4) => None,     /* not used */
            CarPowerState(..=-1) => None, /* not used */
            CarPowerState(15..) => None,  /* not used */
        };

        if let Some(signal) = signal_to_send {
            send_power_report_ignore_error(&self.report_tx, signal);
        }
        Ok(())
    }
}

async fn subscribe_to_network_state(
    tokio_runtime: Arc<tokio::runtime::Runtime>,
    sender: broadcast::Sender<SimplePowerState>,
    service: Strong<dyn IPrivilegedGatewayNetworkingAsync<Tokio>>,
) {
    let listener_binder = BnNetworkStateListener::new_async_binder(
        NetworkStateListener { report_tx: sender },
        TokioRuntime(tokio_runtime.clone()),
        BinderFeatures::default(),
    );

    let service_died_notify = Arc::new(Notify::new());
    let service_died_notifier = service_died_notify.clone();
    let mut death_recipient = DeathRecipient::new(move || {
        service_died_notifier.notify_one();
    });

    if let Err(e) = service.as_binder().link_to_death(&mut death_recipient) {
        error!("Failed to link to death: {e}");
        return;
    }

    match service.registerNetworkStateListener(&listener_binder).await {
        Ok(()) => {
            debug!("Registered network state listener");
            service_died_notify.notified().await;
            warn!("Service died");
        }
        Err(code) => {
            error!("Failed to register network state listener: {code}");
        }
    }

    let _ = service.as_binder().unlink_to_death(&mut death_recipient);
}

pub(crate) async fn run_power_state_reporter(
    tokio_runtime: Arc<tokio::runtime::Runtime>,
    sender: broadcast::Sender<SimplePowerState>,
    _mesh_manager_watcher_state_rx: Receiver<WatcherState>,
) {
    loop {
        let service = wait_for_gateway_networking_service().await;

        subscribe_to_network_state(tokio_runtime.clone(), sender.clone(), service).await;
    }
}
