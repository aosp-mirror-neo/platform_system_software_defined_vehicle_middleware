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

pub(crate) async fn run_power_state_reporter(
    _tokio_runtime: std::sync::Arc<tokio::runtime::Runtime>,
    _sender: tokio::sync::broadcast::Sender<crate::power_state::SimplePowerState>,
    _mesh_manager_watcher_state_rx: tokio::sync::watch::Receiver<
        crate::mesh::manager::WatcherState,
    >,
) {
    log::info!("Reacting to power state changes is disabled.");
    // Don't let the task complete, otherwise _sender will be dropped, which closes the
    // channel, generating an error on all readers.
    futures::future::pending::<()>().await;
}
