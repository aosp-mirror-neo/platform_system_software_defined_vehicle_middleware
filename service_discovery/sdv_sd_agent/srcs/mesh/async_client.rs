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

use anyhow::bail;
use grpcio::{CallOption, Channel, ChannelBuilder, Environment, Result as GrpcResult};

use openssl::{
    pkey::{PKey, Private},
    x509::X509,
};
pub use sdv_sd_mesh_api::sdv_sd_mesh::{
    ExchangeRootCertsRequest, GetPeerLocalAclsRequest, GetPeerLocalAclsResponse, GetUdsPubRequest,
    PeerLocalServiceUnitsEvent, PingRequest, PingResponse, SubscribePeerLocalServiceUnitsRequest,
};
use sdv_sd_mesh_api::sdv_sd_mesh_grpc::MeshManagerClient;

use crate::mesh::{
    credentials_checker::CredentialsChecker, custom_verifier::make_channel_credentials,
};
use async_trait::async_trait;
use futures::Stream;
use log::info;
use mockall::automock;
use std::pin::Pin;
use std::sync::Arc;
use std::{net::SocketAddr, time::Duration};
use thiserror::Error;

pub(crate) type PeerUnitStream =
    Pin<Box<dyn Stream<Item = GrpcResult<PeerLocalServiceUnitsEvent>> + Send>>;

#[derive(Error, Debug)]
pub(crate) enum MeshManagerError {
    #[error("Failed to subscribe for mesh node service unit updates: {0}")]
    SubscriptionError(String),
}

/// How long to wait for an answer to a gRPC call.
///
/// Note that MeshManager::Ping uses a separate timeout value.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// How long to wait for an answer to a gRPC MeshManager::Ping call.
const PING_TIMEOUT: Duration = Duration::from_secs(1);

#[automock]
#[async_trait]
pub(crate) trait MeshManagerAsyncClient: Send + Sync {
    fn subscribe_peer_local_service_units(
        &self,
        req: &SubscribePeerLocalServiceUnitsRequest,
    ) -> Result<PeerUnitStream, MeshManagerError>;
    async fn get_peer_local_acls(
        &self,
        req: &GetPeerLocalAclsRequest,
    ) -> GrpcResult<GetPeerLocalAclsResponse>;
    async fn ping(&self, req: &PingRequest) -> GrpcResult<PingResponse>;

    /// Requests UDS public key (or keys) of the device where the peer VM is running.
    async fn get_uds_pub(&self) -> anyhow::Result<Vec<Vec<u8>>>;

    /// Exchanges the root certs between the local and the remote VM.
    ///
    /// root_cert is the local VM's PEM-encoded root X.509 certificate.
    async fn exchange_root_certs(&self, root_cert: &[u8]) -> anyhow::Result<Vec<u8>>;
}

#[derive(Clone)]
pub struct MeshManagerAsyncClientImpl {
    inner_client: MeshManagerClient,
}

impl MeshManagerAsyncClientImpl {
    pub fn new(channel: Channel) -> Self {
        Self { inner_client: MeshManagerClient::new(channel) }
    }
}

#[async_trait]
impl MeshManagerAsyncClient for MeshManagerAsyncClientImpl {
    fn subscribe_peer_local_service_units(
        &self,
        req: &SubscribePeerLocalServiceUnitsRequest,
    ) -> Result<PeerUnitStream, MeshManagerError> {
        // Note: There should not be a timeout here since this request is expected to
        // never finish as the returned stream will be kept open for returning updates.
        Ok(Box::pin(
            self.inner_client
                .subscribe_peer_local_service_units(req)
                .map_err(|err| MeshManagerError::SubscriptionError(err.to_string()))?,
        ))
    }

    async fn get_peer_local_acls(
        &self,
        req: &GetPeerLocalAclsRequest,
    ) -> GrpcResult<GetPeerLocalAclsResponse> {
        self.inner_client
            .get_peer_local_acls_async_opt(req, CallOption::default().timeout(REQUEST_TIMEOUT))?
            .await
    }

    async fn ping(&self, req: &PingRequest) -> GrpcResult<PingResponse> {
        self.inner_client.ping_async_opt(req, CallOption::default().timeout(PING_TIMEOUT))?.await
    }

    async fn get_uds_pub(&self) -> anyhow::Result<Vec<Vec<u8>>> {
        let grpc_result = self
            .inner_client
            .get_uds_pub_async_opt(
                &GetUdsPubRequest { ..Default::default() },
                CallOption::default().timeout(REQUEST_TIMEOUT),
            )?
            .await;
        Ok(grpc_result?.uds_pub)
    }

    async fn exchange_root_certs(&self, root_cert: &[u8]) -> anyhow::Result<Vec<u8>> {
        let request =
            ExchangeRootCertsRequest { root_cert: root_cert.to_vec(), ..Default::default() };
        let grpc_result = self
            .inner_client
            .exchange_root_certs_async_opt(
                &request,
                CallOption::default().timeout(REQUEST_TIMEOUT),
            )?
            .await;
        Ok(grpc_result?.root_cert)
    }
}

#[automock]
#[async_trait]
pub(crate) trait MeshConnector: Sync + Send {
    async fn try_connect(
        &self,
        env: Arc<Environment>,
        socket_addr: &SocketAddr,
        vm_name: &str,
    ) -> anyhow::Result<Box<dyn MeshManagerAsyncClient>>;
}

/// Connector that establishes a secure TLS connection with DICE-based authentication.
pub(crate) struct SecureMeshConnector {
    sd_cert: X509,
    sd_private_key: PKey<Private>,
    credentials_checker: CredentialsChecker,
}

impl SecureMeshConnector {
    pub(crate) fn new(
        credentials_checker: CredentialsChecker,
        sd_cert: &X509,
        sd_private_key: &PKey<Private>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            sd_cert: sd_cert.clone(),
            sd_private_key: sd_private_key.clone(),
            credentials_checker,
        })
    }
}

#[async_trait]
impl MeshConnector for SecureMeshConnector {
    async fn try_connect(
        &self,
        env: Arc<Environment>,
        socket_addr: &SocketAddr,
        vm_name: &str,
    ) -> anyhow::Result<Box<dyn MeshManagerAsyncClient>> {
        let credentials = make_channel_credentials(
            self.credentials_checker.clone(),
            &self.sd_cert,
            &self.sd_private_key,
        )?;

        let channel = {
            let builder = ChannelBuilder::new(env)
                .override_ssl_target(vm_name.as_bytes())
                .set_credentials(credentials);

            info!("Connecting to {vm_name} at {socket_addr} using secure TLS channel with DICE authentication");
            builder.connect(&socket_addr.to_string())
        };

        if !channel.wait_for_connected(Duration::from_secs(1)).await {
            bail!(
                "Unable to connect to mesh. Channel state: {:?}",
                channel.check_connectivity_state(/*try_connect=*/ false)
            );
        }
        Ok(Box::new(MeshManagerAsyncClientImpl::new(channel)))
    }
}

pub(crate) struct UnsecureMeshConnector {}

#[async_trait]
impl MeshConnector for UnsecureMeshConnector {
    async fn try_connect(
        &self,
        env: Arc<Environment>,
        socket_addr: &SocketAddr,
        _vm_name: &str,
    ) -> anyhow::Result<Box<dyn MeshManagerAsyncClient>> {
        let builder = ChannelBuilder::new(env);
        info!("Connecting to {} using unsecure channel", socket_addr);
        let channel = builder.connect(socket_addr.to_string().as_ref());
        if !channel.wait_for_connected(Duration::from_secs(1)).await {
            bail!("Unable to connect to mesh");
        }
        Ok(Box::new(MeshManagerAsyncClientImpl::new(channel)))
    }
}
