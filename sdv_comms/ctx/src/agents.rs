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

use binder::Strong;
use google_sdv_comms::aidl::google::sdv::comms::ContextInitializationToken::ContextInitializationToken;
use google_sdv_data_tunnel::aidl::google::sdv::data_tunnel::IAgentService::{BpAgentService as BpDataTunnelAgent, IAgentService as IDataTunnelAgent};
use google_sdv_rpc::aidl::google::sdv::rpc::IRpcAgent::{BpRpcAgent, IRpcAgent};
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent::{BpServiceDiscoveryAgent, IServiceDiscoveryAgent};
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceRegistrationAgent::{BpServiceRegistrationAgent, IServiceRegistrationAgent};
use paste::paste;
use sdv_status_rs::{derive, SdvResult, SdvStatus, SdvStatusCode};

macro_rules! get_agent_binder_name {
    ($service_name:ident) => {
        paste! {
            <[<Bp $service_name>] as [<I $service_name>]>::get_descriptor().to_owned() + "/default"
        }
    };
}

pub struct SdvAgents {
    /// Service Discovery Agent.
    sr_binder: Strong<dyn IServiceRegistrationAgent>,

    /// Service Discovery Agent.
    sd_binder: Strong<dyn IServiceDiscoveryAgent>,

    /// Data Tunnel Agent.
    dt_binder: Strong<dyn IDataTunnelAgent>,

    /// Rpc Agent.
    rpc_binder: Strong<dyn IRpcAgent>,
}

impl SdvAgents {
    /// Creates a new SdvAgents implementation.
    pub fn new(init: &ContextInitializationToken) -> SdvResult<SdvAgents> {
        let sr_binder = init.sr_agent.clone().ok_or(SdvStatus::with_message(
            SdvStatusCode::InvalidArgument,
            "Service Registration Binder is missing",
        ))?;

        let sd_binder = init.sd_agent.clone().ok_or(SdvStatus::with_message(
            SdvStatusCode::InvalidArgument,
            "Service Discovery Binder is missing",
        ))?;

        let dt_binder = init.dt_agent.clone().ok_or(SdvStatus::with_message(
            SdvStatusCode::InvalidArgument,
            "Data Tunnel Binder is missing",
        ))?;

        let rpc_binder = init.rpc_agent.clone().ok_or(SdvStatus::with_message(
            SdvStatusCode::InvalidArgument,
            "RPC Binder is missing",
        ))?;

        Ok(SdvAgents { sr_binder, sd_binder, dt_binder, rpc_binder })
    }

    /// Creates a new SdvAgents implementation.
    pub fn new_static() -> SdvResult<SdvAgents> {
        let sr_binder = {
            let descriptor = get_agent_binder_name!(ServiceRegistrationAgent);
            binder::check_interface(&descriptor).map_err(derive!(
                SdvStatusCode::Internal,
                format!("Binder Error retrieving agent interface: {}", descriptor)
            ))?
        };

        let sd_binder = {
            let descriptor = get_agent_binder_name!(ServiceDiscoveryAgent);
            binder::check_interface(&descriptor).map_err(derive!(
                SdvStatusCode::Internal,
                format!("Binder Error retrieving agent interface: {}", descriptor)
            ))?
        };

        let dt_binder = {
            let descriptor = get_agent_binder_name!(DataTunnelAgent);
            binder::check_interface(&descriptor).map_err(derive!(
                SdvStatusCode::Internal,
                format!("Binder Error retrieving agent interface: {}", descriptor)
            ))?
        };

        let rpc_binder = {
            let descriptor = get_agent_binder_name!(RpcAgent);
            binder::check_interface(&descriptor).map_err(derive!(
                SdvStatusCode::Internal,
                format!("Binder Error retrieving agent interface: {}", descriptor)
            ))?
        };

        Ok(SdvAgents { sr_binder, sd_binder, dt_binder, rpc_binder })
    }

    /// Returns a Binder proxy for Service Registration Agent.
    pub fn get_sr_binder(&self) -> &Strong<dyn IServiceRegistrationAgent> {
        &self.sr_binder
    }

    /// Returns a Binder proxy for Service Discovery Agent.
    pub fn get_sd_binder(&self) -> &Strong<dyn IServiceDiscoveryAgent> {
        &self.sd_binder
    }

    /// Returns a Binder proxy for Data Tunnel Agent.
    pub fn get_dt_binder(&self) -> &Strong<dyn IDataTunnelAgent> {
        &self.dt_binder
    }

    /// Returns a Binder proxy for Rpc Agent.
    pub fn get_rpc_binder(&self) -> &Strong<dyn IRpcAgent> {
        &self.rpc_binder
    }
}

/// Ensures that the Comms Stack Agents are started, or start them if any
/// is configured as a dynamic service and isn't yet started.
pub fn wait_for_agents() -> SdvResult<()> {
    {
        let descriptor = get_agent_binder_name!(ServiceRegistrationAgent);
        binder::wait_for_interface::<dyn IServiceRegistrationAgent>(&descriptor).map_err(
            derive!(
                SdvStatusCode::Internal,
                format!("Binder Error waiting for agent interface: {}", descriptor)
            ),
        )?;
    };

    {
        let descriptor = get_agent_binder_name!(ServiceDiscoveryAgent);
        binder::wait_for_interface::<dyn IServiceDiscoveryAgent>(&descriptor).map_err(derive!(
            SdvStatusCode::Internal,
            format!("Binder Error waiting for agent interface: {}", descriptor)
        ))?;
    };

    {
        let descriptor = get_agent_binder_name!(DataTunnelAgent);
        binder::wait_for_interface::<dyn IDataTunnelAgent>(&descriptor).map_err(derive!(
            SdvStatusCode::Internal,
            format!("Binder Error waiting for agent interface: {}", descriptor)
        ))?;
    };

    {
        let descriptor = get_agent_binder_name!(RpcAgent);
        binder::wait_for_interface::<dyn IRpcAgent>(&descriptor).map_err(derive!(
            SdvStatusCode::Internal,
            format!("Binder Error waiting for agent interface: {}", descriptor)
        ))?;
    };

    Ok(())
}
