// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! SDV COMMS DT static state management.

#![allow(unused)]

use crate::core::comm_channel::CommChannel;
use crate::ffi::ffi_types;

use core::ffi::c_void;
use google_sdv_data_tunnel::aidl::google::sdv::data_tunnel::ClientDescriptor::ClientDescriptor as ClientDescriptorAidl;
use log::debug;
use once_cell::sync::OnceCell;
use sdv_comms_ctx::ContextImpl;
use sdv_status_rs::SdvResult;
use std::os::fd::AsRawFd;
use std::sync::RwLock;

/// Struct representing the static state of the
pub(crate) struct DataTunnelState {
    client_descriptor: ClientDescriptorAidl,
    pub comm_channel: RwLock<CommChannel>,
}

impl DataTunnelState {
    /// Retrieves the Data Tunnel client_id stored in the state.
    pub(crate) fn get_client_id(&self) -> u32 {
        self.client_descriptor.client_id as u32
    }
}

/// Obtains the Data Tunnel State; initialises the state if it does not already exist.
pub(crate) fn get_state(ctx: &ContextImpl) -> SdvResult<&DataTunnelState> {
    static STATE: OnceCell<DataTunnelState> = OnceCell::new();

    let init_state = || -> SdvResult<DataTunnelState> {
        let mut cd = ClientDescriptorAidl::default();
        let sid = ctx.get_identity().sid();

        debug!("sdv_dt: ctx.get_agents().get_dt_binder().Connect()");
        ctx.get_agents().get_dt_binder().Connect(sid as i64, &mut cd)?;

        let read_fd = cd.comm_pipe_handle.fds.remove(0);
        let write_fd = cd.comm_pipe_handle.fds.remove(0);
        let mut cc = CommChannel::create(read_fd.into(), write_fd.into());
        Ok(DataTunnelState { client_descriptor: cd, comm_channel: RwLock::new(cc) })
    };

    STATE.get_or_try_init(init_state)
}
