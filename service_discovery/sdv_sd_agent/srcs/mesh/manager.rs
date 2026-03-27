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

use rustutils::android::system_properties;

use super::peer_connection::PeerConnection;
use super::{vvm_config::VmConfig, VvmConfig};
use crate::authz::proto::from_peer_local_acls_proto;
use crate::authz::MeshAclPolicyProvider;
use crate::ca::service::RootCertCommand;
use crate::kv::ServiceRegistry;
use crate::mesh::async_client::{MeshConnector, MeshManagerAsyncClient};
use crate::mesh::UdsPubsProvisioner;
use crate::power_state::SimplePowerState;
use sdv_authz::AuthzService;

use anyhow::{bail, Context};
use crypto_rpc::cert::base32_x509_common_name_to_string;
use futures::future::join_all;
use grpcio::Environment;
use log::{debug, error, info, warn};
use openssl::x509::X509;
use std::borrow::Borrow;
use std::cmp;
use std::collections::HashSet;
use std::fmt::Debug;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{
    broadcast::{self, error::RecvError},
    mpsc::UnboundedSender,
    watch,
};
use tokio::time::sleep;
use tracing::instrument;

use sdv_comms_id::names::VmName;
use sdv_sd_mesh_api::sdv_sd_mesh::GetPeerLocalAclsRequest;

#[cfg(sdv_sd_agent_for_a15)]
const SYSTEM_PROP_MESH_MISSING_VMS: &str = "vendor.sdv.mesh_missing_vms.count";

#[cfg(not(sdv_sd_agent_for_a15))]
const SYSTEM_PROP_MESH_MISSING_VMS: &str = "sdv.mesh_missing_vms.count";

/// Status of the peer.
/// We add new peers in Connected state.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) enum PeerStatus {
    Connected,
    Disconnected,
}

type MeshGroup = Arc<Mutex<HashSet<VmName>>>;

#[derive(Clone)]
pub(crate) struct LockedMesh {
    /// The PEM-encoded X.509 root certificate of the local VM.
    pub local_root_cert: Vec<u8>,

    /// Sender for CA root cert commands.
    pub ca_command_sender: UnboundedSender<RootCertCommand>,
}

#[derive(Clone)]
pub(crate) struct UnlockedMesh {
    pub uds_pubs_provisioner: Arc<Mutex<UdsPubsProvisioner>>,
}

/// Specifies in which mode the mesh is and carries any extra data that MeshManager needs in that
/// particular mesh mode.
#[derive(Clone)]
pub(crate) enum MeshMode {
    /// Mesh is locked (secure). We use a CA with root certificates that are generated at runtime
    /// and exchanged over the secure mesh.
    Locked(LockedMesh),

    /// Mesh is unlocked (insecure). State used for provisioning of the uds_pubs file and for
    /// development.
    Unlocked(UnlockedMesh),
}

/// State of the MeshManager's watcher tasks.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) enum WatcherState {
    /// Not all peer VM watcher tasks have been created yet.
    ///
    /// This is the initial state.
    Stopped,

    /// At least one peer VM watcher task is running.
    Running,

    /// All peer VM watcher tasks are paused.
    Paused,
}

const MESH_MIN_SCAN_INTERVAL: Duration = Duration::from_secs(1);
const MAX_SCAN_INTERVAL_MULTIPLIER: u32 = 8;

/// Perform peer registration actions that are specific to a given mesh mode.
///
/// In `Unlocked` mode, it retrieves the UDS public keys of the peer and adds them uds_pubs
/// provisioner.
///
/// In `Locked` mode, it exchanges the root certificates with the peer and sends the received
/// root certificate to the local CA service.
enum MeshModeRegistration {
    Locked {
        vm_name: VmName,
        peer_root_cert: Vec<u8>,
        ca_command_sender: UnboundedSender<RootCertCommand>,
    },
    Unlocked {
        vm_name: VmName,
        uds_pubs_provisioner: Arc<Mutex<UdsPubsProvisioner>>,
    },
}

impl MeshModeRegistration {
    async fn try_new(
        mesh_mode: &MeshMode,
        vm_name: &VmName,
        peer: &dyn MeshManagerAsyncClient,
    ) -> anyhow::Result<Self> {
        match mesh_mode {
            MeshMode::Unlocked(unlocked_mesh) => {
                let UnlockedMesh { uds_pubs_provisioner } = unlocked_mesh;

                let uds_pub = peer.get_uds_pub().await?;

                let mut guarded_provisioner = uds_pubs_provisioner.lock().unwrap();

                guarded_provisioner.add_uds_pubs(vm_name.as_ref(), uds_pub)?;

                Ok(Self::Unlocked {
                    vm_name: vm_name.clone(),
                    uds_pubs_provisioner: uds_pubs_provisioner.clone(),
                })
            }
            MeshMode::Locked(locked_mesh) => {
                let LockedMesh { local_root_cert, ca_command_sender } = locked_mesh;

                let peer_root_cert = peer.exchange_root_certs(local_root_cert).await?;
                Self::validate_root_cert_pem(peer_root_cert.as_slice(), vm_name)
                    .context("Failed to validate root cert received from peer")?;

                if let Err(err) = ca_command_sender.send(RootCertCommand::Add {
                    vm_name: vm_name.clone().into(),
                    root_cert: peer_root_cert.clone(),
                }) {
                    bail!("Failed to send root cert through channel: {}", err);
                }

                Ok(Self::Locked {
                    vm_name: vm_name.clone(),
                    peer_root_cert,
                    ca_command_sender: ca_command_sender.clone(),
                })
            }
        }
    }

    fn validate_root_cert_pem(
        root_cert_pem: &[u8],
        expected_vm_name: &VmName,
    ) -> anyhow::Result<()> {
        let root_cert = X509::from_pem(root_cert_pem)?;

        let subject_name = base32_x509_common_name_to_string(root_cert.subject_name())?;

        if subject_name.as_str() != expected_vm_name.as_ref() {
            bail!(
                "VM root cert has subject name \"{subject_name}\", while we expected \
                \"{expected_vm_name}\""
            );
        }

        Ok(())
    }
}

impl Drop for MeshModeRegistration {
    fn drop(&mut self) {
        match self {
            MeshModeRegistration::Unlocked { vm_name, uds_pubs_provisioner } => {
                let mut guarded_provisioner = uds_pubs_provisioner.lock().unwrap();
                guarded_provisioner.forget_vm(vm_name.as_ref());
            }
            MeshModeRegistration::Locked { vm_name, peer_root_cert, ca_command_sender } => {
                if let Err(err) = ca_command_sender.send(RootCertCommand::Remove {
                    vm_name: vm_name.clone().into(),
                    root_cert: peer_root_cert.clone(),
                }) {
                    error!("Failed to notify CAService about credentials removal: {err:?}");
                }
            }
        }
    }
}

async fn do_exponential_delay(
    delay_multiplier: &mut u32,
    wake_receiver: &mut broadcast::Receiver<()>,
    power_state_rx: &mut broadcast::Receiver<SimplePowerState>,
    vm_name: &VmName,
) -> WakeUpReason {
    do_exponential_delay_internal(
        MESH_MIN_SCAN_INTERVAL,
        MAX_SCAN_INTERVAL_MULTIPLIER,
        delay_multiplier,
        wake_receiver,
        power_state_rx,
        vm_name,
    )
    .await
}

#[derive(PartialEq, Eq, Debug)]
enum WakeUpReason {
    Notification,
    PowerState(SimplePowerState),
    Timeout,
}

async fn do_exponential_delay_internal(
    min_scan_interval: Duration,
    max_delay_multiplier: u32,
    delay_multiplier: &mut u32,
    wake_receiver: &mut broadcast::Receiver<()>,
    power_state_rx: &mut broadcast::Receiver<SimplePowerState>,
    vm_name: &VmName,
) -> WakeUpReason {
    let max_wait_time = min_scan_interval * *delay_multiplier;

    let wake_up_reason = loop {
        let reason = tokio::select! {
            res = wake_receiver.recv() => match res {
                Ok(_) => Some(WakeUpReason::Notification),
                Err(RecvError::Lagged(_)) => {
                    // Woken up by a notification, even if we lagged. This is fine.
                    debug!("Watcher task {vm_name} woken up by lagged notification.");
                    Some(WakeUpReason::Notification)
                }
                Err(RecvError::Closed) => {
                    panic!("Watcher task {vm_name}: wake_receiver channel closed.");
                }
            },
            res = power_state_rx.recv() => match res {
                Ok(power_state) => Some(WakeUpReason::PowerState(power_state)),
                Err(RecvError::Lagged(n)) => {
                    warn!("Watcher task {vm_name}: Power state receiver lagged by {n} messages. \
                        Re-reading.");
                    None // Loop again to re-read
                }
                Err(RecvError::Closed) => {
                    panic!("Watcher task {vm_name}: power_state_rx channel closed.");
                }
            },
            _ = sleep(max_wait_time) => Some(WakeUpReason::Timeout),
        };

        if let Some(reason) = reason {
            break reason;
        }
    };

    debug!("Watcher task {vm_name} woken up by: {:?}", wake_up_reason);

    if wake_up_reason == WakeUpReason::Timeout {
        *delay_multiplier = cmp::min(*delay_multiplier * 2, max_delay_multiplier);
    } else {
        *delay_multiplier = 1; // Reset delay on any other event.
    }

    wake_up_reason
}

#[derive(Clone)]
pub(crate) struct MeshManager {
    mesh_acl_policy_provider: Arc<MeshAclPolicyProvider>,
    service_registry: Arc<ServiceRegistry>,
    authz_service: Arc<AuthzService>,
    peers: MeshGroup,
    vvm_config: Arc<VvmConfig>,
    connector: Arc<dyn MeshConnector>,
    mesh_mode: MeshMode,
    env: Arc<Environment>,
    wake_notifier: broadcast::Sender<()>,
    power_state_tx: broadcast::Sender<SimplePowerState>,
    watcher_state_tx: watch::Sender<WatcherState>,
    paused_watchers_count: Arc<AtomicUsize>,
}

pub(crate) struct MeshManagerParams {
    pub mesh_acl_policy_provider: Arc<MeshAclPolicyProvider>,
    pub service_registry: Arc<ServiceRegistry>,
    pub authz_service: Arc<AuthzService>,
    pub vvm_config: Arc<VvmConfig>,
    pub connector: Box<dyn MeshConnector>,
    pub mesh_mode: MeshMode,
    pub env: Arc<Environment>,
    pub wake_notifier: broadcast::Sender<()>,
    pub power_state_tx: broadcast::Sender<SimplePowerState>,
}

impl MeshManager {
    pub(crate) fn new(params: MeshManagerParams) -> MeshManager {
        MeshManager {
            mesh_acl_policy_provider: params.mesh_acl_policy_provider,
            service_registry: params.service_registry,
            authz_service: params.authz_service,
            peers: Default::default(),
            vvm_config: params.vvm_config,
            connector: Arc::from(params.connector),
            mesh_mode: params.mesh_mode,
            env: params.env,
            wake_notifier: params.wake_notifier,
            power_state_tx: params.power_state_tx,
            watcher_state_tx: watch::Sender::new(WatcherState::Stopped),
            paused_watchers_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Returns status of the peers that are listed in VVM Config
    pub(crate) fn get_peers_status(&self) -> Vec<(String, PeerStatus)> {
        let peers = self.peers.lock().unwrap();
        let vvmconfig = &self.vvm_config;

        vvmconfig
            .remote_cfgs
            .iter()
            .map(|vm_cfg| {
                (
                    vm_cfg.name.name().to_owned(),
                    if peers.get(&vm_cfg.name).is_some() {
                        PeerStatus::Connected
                    } else {
                        PeerStatus::Disconnected
                    },
                )
            })
            .collect()
    }

    pub(crate) fn subscribe_to_watcher_thread_state(&self) -> watch::Receiver<WatcherState> {
        self.watcher_state_tx.subscribe()
    }

    /// Spawns a thread that periodically attempts to connect to remote peers and checks existing
    /// connections.
    pub(crate) async fn spawn_watcher_tasks(&self, tokio_runtime: Arc<tokio::runtime::Runtime>) {
        let mut join_handles = Vec::new();
        for vm_cfg in &self.vvm_config.remote_cfgs {
            let manager = self.clone();
            let handle =
                tokio_runtime.spawn(watcher_task(manager, vm_cfg.clone(), tokio_runtime.clone()));
            join_handles.push(handle);
        }

        join_all(join_handles).await;
    }

    fn set_missing_vms_system_property(&self) {
        let vm_count = self.vvm_config.remote_cfgs.len();
        let peers = self.peers.lock().unwrap();
        let missing_vms = (vm_count - peers.len()).to_string();
        let _ = system_properties::write(SYSTEM_PROP_MESH_MISSING_VMS, &missing_vms).inspect_err(
            |err| {
                error!("Failed to update system property {SYSTEM_PROP_MESH_MISSING_VMS}: {err:?}");
            },
        );
    }

    fn insert_vm(&self, vm_name: &VmName) {
        let vm_name = vm_name.to_owned();
        let _ = self.peers.lock().unwrap().insert(vm_name);
        self.set_missing_vms_system_property();
    }

    /// Do all unregistrations and removals related to the given VM name
    fn remove_vm(&self, vm_name: &VmName) {
        debug!("Unregistering or removing all information related to VM {vm_name}");
        self.peers.lock().unwrap().remove(vm_name);
        self.set_missing_vms_system_property();
        self.service_registry.remove_by_vm_name(vm_name.as_ref());
        self.mesh_acl_policy_provider.remove_remote_acl_policy_provider(vm_name);
    }

    async fn connect_to_peer(
        &self,
        vm_cfg: &VmConfig,
    ) -> anyhow::Result<Box<dyn MeshManagerAsyncClient>> {
        self.connector.try_connect(Arc::clone(&self.env), &vm_cfg.addr, vm_cfg.name.as_ref()).await
    }

    async fn reconnect(
        &self,
        vm_cfg: &VmConfig,
        tokio_runtime: Arc<tokio::runtime::Runtime>,
    ) -> anyhow::Result<(PeerConnection, MeshModeRegistration)> {
        let rpc_client = self.connect_to_peer(vm_cfg).await?;
        info!("Connected to SD Agent on VM: {}", vm_cfg.name);

        let mesh_mode_registration =
            MeshModeRegistration::try_new(&self.mesh_mode, &vm_cfg.name, rpc_client.borrow())
                .await?;

        self.update_acls_from_remote(rpc_client.borrow(), &vm_cfg.name).await?;

        let peer_connection = PeerConnection::try_new(
            vm_cfg.name.as_ref(),
            rpc_client,
            self.service_registry.clone(),
            self.authz_service.clone(),
            tokio_runtime,
        )?;

        Ok((peer_connection, mesh_mode_registration))
    }

    #[instrument(name = "authz_perf::mesh::rpc::update_acls_from_remote", skip(self, peer))]
    async fn update_acls_from_remote(
        &self,
        peer: &dyn MeshManagerAsyncClient,
        vm_name: &VmName,
    ) -> anyhow::Result<()> {
        let proto = peer
            .get_peer_local_acls(&GetPeerLocalAclsRequest { ..Default::default() })
            .await
            .inspect_err(|err| {
                self.mesh_acl_policy_provider.remove_remote_acl_policy_provider(vm_name);
                error!("Failed to get list of ACLs from VM {vm_name}: {err:?}");
            })?;

        let remote_acl_policy_provider = from_peer_local_acls_proto(proto).inspect_err(|err| {
            error!("Failed to parse remote ACLs from VM {vm_name}: {err:?}");
        })?;

        self.mesh_acl_policy_provider
            .set_remote_acl_policy_provider(vm_name.clone(), Box::new(remote_acl_policy_provider));

        Ok(())
    }

    async fn wait_for_power_back_on(
        &self,
        vm_name: &VmName,
        power_state_rx: &mut broadcast::Receiver<SimplePowerState>,
    ) {
        let num_watchers = self.vvm_config.remote_cfgs.len();
        // fetch_add() returns the previous value, so we add 1 to get the new value.
        if self.paused_watchers_count.fetch_add(1, Ordering::SeqCst) + 1 == num_watchers {
            // This is the last watcher to pause
            let _ = self.watcher_state_tx.send_replace(WatcherState::Paused);
        }
        debug!("Watcher task {vm_name}: Waiting for power to be back on...");
        loop {
            match power_state_rx.recv().await {
                Ok(SimplePowerState::On) => break,
                Ok(_) => continue, // Not on, so continue waiting.
                Err(RecvError::Lagged(n)) => {
                    warn!("Watcher task {vm_name}: Power state receiver lagged by {n} messages.");
                    continue; // Try again to receive the oldest event still retained.
                }
                Err(RecvError::Closed) => {
                    panic!("Watcher task {vm_name}: Power state channel closed.");
                }
            }
        }
        debug!("Watcher task {vm_name}: Power is back on. Resuming.");
        // fetch_sub() returns the previous value
        if self.paused_watchers_count.fetch_sub(1, Ordering::SeqCst) == num_watchers {
            // This is the first watcher to resume
            let _ = self.watcher_state_tx.send_replace(WatcherState::Running);
        }
    }
}

async fn watcher_task(
    manager: MeshManager,
    vm_cfg: VmConfig,
    tokio_runtime: Arc<tokio::runtime::Runtime>,
) {
    info!("Watcher task for remote VM {} is running", vm_cfg.name);
    let _ = manager.watcher_state_tx.send_if_modified(set_watcher_state_running);

    let mut maybe_peer_connection: Option<(PeerConnection, MeshModeRegistration)> = None;
    let mut wake_receiver = manager.wake_notifier.subscribe();
    let mut power_state_rx = manager.power_state_tx.subscribe();
    let mut exponential_delay_multiplier = 1;
    // At first, we assume the power state is On.
    let mut current_power_state = SimplePowerState::On;

    loop {
        // If the task shouldn't be running, close the connection and pause it.
        if current_power_state == SimplePowerState::Off {
            if let Some((mut peer_connection, _mesh_mode_registration)) =
                maybe_peer_connection.take()
            {
                info!("Power state change: disconnecting from peer {}", vm_cfg.name);
                peer_connection.close().await;
                manager.remove_vm(&vm_cfg.name);
            }
            manager.wait_for_power_back_on(&vm_cfg.name, &mut power_state_rx).await;
            current_power_state = SimplePowerState::On;
            exponential_delay_multiplier = 1; // Reset delay after resuming.
        }

        // If we have a connection, check its liveness and close it if it's dead.
        if let Some((mut peer_connection, mesh_mode_registration)) = maybe_peer_connection.take() {
            match peer_connection.check_liveness().await {
                Ok(_) => {
                    maybe_peer_connection = Some((peer_connection, mesh_mode_registration));
                }
                Err(error) => {
                    warn!("{error}");
                    peer_connection.close().await;
                    manager.remove_vm(&vm_cfg.name);
                }
            }
        }

        // If we don't have a connection, try to create one.
        if maybe_peer_connection.is_none() {
            match manager.reconnect(&vm_cfg, tokio_runtime.clone()).await {
                Ok((peer_connection, mesh_mode_registration)) => {
                    manager.insert_vm(&vm_cfg.name);
                    // Reset delay on successful (re)connection.
                    exponential_delay_multiplier = 1;
                    maybe_peer_connection = Some((peer_connection, mesh_mode_registration));
                }
                Err(err) => {
                    warn!("Attempt to reconnect to {} failed: {err:?}", &vm_cfg.name);
                }
            }
        }

        // Wait for an event to come, with a timeout
        let wake_up_reason = do_exponential_delay(
            &mut exponential_delay_multiplier,
            &mut wake_receiver,
            &mut power_state_rx,
            &vm_cfg.name,
        )
        .await;
        if let WakeUpReason::PowerState(new_power_state) = wake_up_reason {
            current_power_state = new_power_state;
        }
    }
}

fn set_watcher_state_running(state: &mut WatcherState) -> bool {
    if *state != WatcherState::Running {
        *state = WatcherState::Running;
        true
    } else {
        false
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    pub(crate) use crate::mesh::peer_connection::test_support::{
        VM_INSTANCE_1_NAME, VM_INSTANCE_2_NAME,
    };
    use crate::mesh::vvm_config::{create_test_uds_ca_pub, VmConfig};
    use anyhow::anyhow;
    use crypto_rpc::cert::{build_ec_private_key, build_root_certificate};
    use openssl::pkey::PKey;
    use sdv_authz::{
        LocallyStoredAuthzPoliciesProvider, SingleVmAclPolicyProvider, VmLevelPermissionsProvider,
    };
    use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
    use sdv_authz_policy_proto_rust::vm_authz_policy::VmAuthzPolicy as VmAuthzPolicyProto;
    use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName};
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;

    pub struct TestSingleVmAclPolicyProvider {
        mapping: HashMap<(PackageName, Option<ServiceBundleName>), AclPolicyProto>,
    }

    impl TestSingleVmAclPolicyProvider {
        pub fn new() -> Self {
            Self { mapping: HashMap::new() }
        }

        pub fn add_policy(
            &mut self,
            package_name: PackageName,
            service_bundle_name: ServiceBundleName,
            policy: AclPolicyProto,
        ) {
            self.mapping.insert((package_name, Some(service_bundle_name)), policy);
        }
    }

    impl LocallyStoredAuthzPoliciesProvider for TestSingleVmAclPolicyProvider {}

    impl SingleVmAclPolicyProvider for TestSingleVmAclPolicyProvider {
        fn get_acl_policy_proto(
            &self,
            package_name: &PackageName,
            service_bundle_name: Option<&ServiceBundleName>,
        ) -> std::result::Result<AclPolicyProto, anyhow::Error> {
            self.mapping
                .get(&(package_name.clone(), service_bundle_name.cloned()))
                .cloned()
                .ok_or_else(|| anyhow!("No ACL policy found for the given service bundle"))
        }

        fn get_namespace_with_acl_policy(
            &self,
        ) -> std::result::Result<Vec<(PackageName, Option<ServiceBundleName>)>, anyhow::Error>
        {
            Ok(self.mapping.keys().cloned().collect())
        }
    }

    impl VmLevelPermissionsProvider for TestSingleVmAclPolicyProvider {
        fn get_vm_authz_policy_proto(
            &self,
            _vm_name: &VmName,
        ) -> Result<VmAuthzPolicyProto, anyhow::Error> {
            unimplemented!("get_vm_authz_policy_proto is not implemented for test")
        }
    }

    /// Creates a PEM-encoded root cert for the given VM name
    pub fn create_root_cert_pem_for_vm(vm_name: &str) -> anyhow::Result<Vec<u8>> {
        let root_key = PKey::from_ec_key(build_ec_private_key()?)?;
        let root_cert = build_root_certificate(&root_key, vm_name)?;
        Ok(root_cert.to_pem()?)
    }

    /// Creates a disabled authz service for test.
    ///
    /// We use a disabled AuthzService because the tests in this module focus on mesh connection
    /// establishment, peer discovery, service unit registration, and ACL exchange. Enabling
    /// authorization would require populating the AuthzService with all necessary policies for
    /// every test, which is unnecessary since authorization itself is tested elsewhere.
    pub fn create_test_authz_service() -> sdv_authz::AuthzService {
        use sdv_authz::{AuthzEnforcementFlag, AuthzStrategy};
        use std::str::FromStr;

        struct TestAuthzEnforcementFlag {}
        impl AuthzEnforcementFlag for TestAuthzEnforcementFlag {
            fn get_enforcement_strategy(&self) -> AuthzStrategy {
                AuthzStrategy::Disabled
            }
        }

        struct TestRegistry {}
        impl sdv_authz::ServiceUnitRegistry for TestRegistry {
            fn get_service_unit(
                &self,
                _fqin: &google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin,
                _unit_name: &str,
            ) -> Option<google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::ServiceUnitDefinition>{
                None
            }
        }

        let vm_name = VmName::from_str("local-vm").unwrap();

        sdv_authz::AuthzService::new(
            vm_name.clone(),
            Arc::new(MeshAclPolicyProvider::new(
                vm_name.clone(),
                Arc::new(TestSingleVmAclPolicyProvider::new()),
            )),
            Arc::new(TestAuthzEnforcementFlag {}),
            Arc::new(TestRegistry {}),
        )
    }

    /// Creates a VvmConfig with a local VM named VM_INSTANCE_1_NAME and
    /// a remote VM named VM_INSTANCE_2_NAME
    pub fn create_test_vvm_config() -> anyhow::Result<VvmConfig> {
        Ok(VvmConfig {
            uds_ca_pub: create_test_uds_ca_pub()?,
            uds_ca_rev_list: vvmtruststore::CertificateRevocationList::default(),
            policies: vec![],
            remote_cfgs: vec![VmConfig::new_for_testing(
                VM_INSTANCE_2_NAME.to_owned().try_into()?,
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 98, 4)), 2002),
            )],
            own_cfg: VmConfig::new_for_testing(
                VM_INSTANCE_1_NAME.to_owned().try_into()?,
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 98, 3)), 2001),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{
        create_root_cert_pem_for_vm, create_test_vvm_config, TestSingleVmAclPolicyProvider,
    };
    use super::*;
    use crate::mesh::vvm_config::{create_test_uds_ca_pub, VmConfig};
    use async_trait::async_trait;
    use mockall::Sequence;
    use std::collections::HashMap;
    use std::result::Result::Ok;
    use std::str::FromStr;

    use core::default::Default;
    use std::borrow::BorrowMut;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use sdv_authz::AclPolicyProvider;
    use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName, ServiceUnitName};
    use tokio::sync::Notify;

    use crate::mesh::peer_connection::test_support::{
        create_instance2_service_unit_events, create_service_unit_events, wait_for,
        wait_for_instance2_service_units_registration, wait_for_remote_service_units_registration,
        FakePeerUnitStream, VM_INSTANCE_1_NAME, VM_INSTANCE_2_NAME, VM_INSTANCE_3_NAME,
    };

    use crate::mesh::async_client::{
        GetPeerLocalAclsResponse, MockMeshConnector, MockMeshManagerAsyncClient, PingResponse,
    };
    use crate::mesh::uds_pubs::test_support::{create_example_key1, create_example_key2};

    use anyhow::anyhow;

    use grpcio::{EnvBuilder, Error as GrpcError, RpcStatus, RpcStatusCode};
    use protobuf::text_format;

    use crate::ca::service::test_support::{
        receive_command_add_root_cert, receive_command_remove_root_cert,
    };

    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

    use coset::CborSerializable;

    fn create_test_vvm_config_2_remotes() -> anyhow::Result<VvmConfig> {
        Ok(VvmConfig {
            uds_ca_pub: create_test_uds_ca_pub()?,
            uds_ca_rev_list: vvmtruststore::CertificateRevocationList::default(),
            policies: vec![],
            remote_cfgs: vec![
                VmConfig::new_for_testing(
                    VM_INSTANCE_2_NAME.to_owned().try_into()?,
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 98, 4)), 2002),
                ),
                VmConfig::new_for_testing(
                    VM_INSTANCE_3_NAME.to_owned().try_into()?,
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 98, 5)), 2003),
                ),
            ],
            own_cfg: VmConfig::new_for_testing(
                VM_INSTANCE_1_NAME.to_owned().try_into()?,
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 98, 3)), 2001),
            ),
        })
    }

    /// A fake MeshConnector that returns clients from a channel.
    /// It's then possible to control **when** try_connect() returns, allowing
    /// synchronization between test code and try_connect() calls.
    struct FakeMeshConnector {
        clients_rx: Arc<
            tokio::sync::Mutex<UnboundedReceiver<anyhow::Result<Box<dyn MeshManagerAsyncClient>>>>,
        >,
    }

    impl FakeMeshConnector {
        fn new() -> (Self, UnboundedSender<anyhow::Result<Box<dyn MeshManagerAsyncClient>>>) {
            let (tx, rx) = unbounded_channel();
            (Self { clients_rx: Arc::new(tokio::sync::Mutex::new(rx)) }, tx)
        }
    }

    #[async_trait]
    impl MeshConnector for FakeMeshConnector {
        async fn try_connect(
            &self,
            _env: Arc<Environment>,
            _socket_addr: &SocketAddr,
            _vm_name: &str,
        ) -> anyhow::Result<Box<dyn MeshManagerAsyncClient>> {
            let mut rx = self.clients_rx.lock().await;
            rx.recv().await.ok_or_else(|| anyhow!("FakeMeshConnector channel closed"))?
        }
    }

    /// Set expectation for the get_peer_local_acls() method to be called once.
    fn expect_get_peer_local_acls(mock_client: &mut Box<MockMeshManagerAsyncClient>) {
        mock_client.expect_get_peer_local_acls().return_once(move |_| {
            Ok(text_format::parse_from_str::<GetPeerLocalAclsResponse>(
                r#"
                    service_bundle_acl_policies {
                        package_name: "com.sdv.sd_test"
                        service_bundle_name: "MeshManagerTest"
                        acl_policy {
                            acls {
                                name: "Acl"
                                include: "*"
                            }
                            service_units {
                                name: "test-service-unit"
                                acl_names: "Acl"
                            }
                        }
                    }
                "#,
            )
            .unwrap())
        });
    }

    /// Builds a MeshManager configured for unit tests
    #[derive(Default)]
    struct MeshManagerBuilder {
        connector: Option<Box<dyn MeshConnector>>,
        mesh_mode: Option<MeshMode>,
        power_state_tx: Option<broadcast::Sender<SimplePowerState>>,
        vvm_config: Option<Arc<VvmConfig>>,
        authz_service: Option<Arc<AuthzService>>,
    }

    impl MeshManagerBuilder {
        pub fn new() -> MeshManagerBuilder {
            Default::default()
        }
        pub fn with_connector(mut self, connector: Box<dyn MeshConnector>) -> Self {
            self.connector = Some(connector);
            self
        }

        pub fn with_mesh_mode(mut self, mesh_mode: MeshMode) -> Self {
            self.mesh_mode = Some(mesh_mode);
            self
        }

        pub fn with_power_state_tx(
            mut self,
            power_state_tx: broadcast::Sender<SimplePowerState>,
        ) -> Self {
            self.power_state_tx = Some(power_state_tx);
            self
        }

        pub fn with_vvm_config(mut self, vvm_config: Arc<VvmConfig>) -> Self {
            self.vvm_config = Some(vvm_config);
            self
        }

        pub fn with_authz_service(mut self, authz_service: Arc<AuthzService>) -> Self {
            self.authz_service = Some(authz_service);
            self
        }

        pub fn build(self) -> MeshManager {
            let vvm_config =
                self.vvm_config.unwrap_or_else(|| Arc::new(create_test_vvm_config().unwrap()));
            let service_registry = Arc::new(ServiceRegistry::default());
            let mesh_acl_policy_provider = Arc::new(MeshAclPolicyProvider::new(
                vvm_config.own_cfg.name.clone(),
                Arc::new(TestSingleVmAclPolicyProvider::new()),
            ));

            let wake_notifier = broadcast::Sender::new(1);

            let power_state_tx = self.power_state_tx.unwrap_or_else(|| broadcast::Sender::new(16));

            let authz_service = self.authz_service.unwrap_or_else(|| {
                Arc::new(crate::mesh::manager::test_support::create_test_authz_service())
            });

            MeshManager::new(MeshManagerParams {
                mesh_acl_policy_provider,
                service_registry,
                authz_service,
                vvm_config,
                connector: self.connector.expect("connector must be set"),
                mesh_mode: self.mesh_mode.expect("mesh_mode must be set"),
                env: Arc::new(EnvBuilder::new().build()),
                wake_notifier,
                power_state_tx,
            })
        }
    }

    /// Creates a MockMeshManagerAsyncClient that expects all the calls
    /// that take place in an unlocked mesh.
    fn create_unlocked_mesh_mock_client() -> Box<dyn MeshManagerAsyncClient> {
        let mut mock_client = Box::new(MockMeshManagerAsyncClient::new());

        let remote_uds_keys_cbor = [create_example_key2()]
            .iter()
            .map(|k| k.clone().to_vec().unwrap())
            .collect::<Vec<Vec<u8>>>();
        mock_client.expect_get_uds_pub().return_once(move || Ok(remote_uds_keys_cbor));

        expect_get_peer_local_acls(mock_client.borrow_mut());

        mock_client
            .expect_subscribe_peer_local_service_units()
            .return_once(move |_| Ok(Box::pin(FakePeerUnitStream::new(vec![]))));

        mock_client.expect_ping().returning(|_| Ok(Default::default()));

        mock_client
    }

    #[test]
    fn watcher_task_connects_and_pings_peer() -> anyhow::Result<()> {
        let mut mock_client = Box::new(MockMeshManagerAsyncClient::new());
        expect_get_peer_local_acls(mock_client.borrow_mut());

        let service_unit_events = create_instance2_service_unit_events();

        let instance2_root_cert_pem = create_root_cert_pem_for_vm(VM_INSTANCE_2_NAME).unwrap();

        mock_client
            .expect_subscribe_peer_local_service_units()
            .return_once(move |_| Ok(Box::pin(FakePeerUnitStream::new(service_unit_events))));

        let instance2_root_cert_pem_clone = instance2_root_cert_pem.clone();
        mock_client
            .expect_exchange_root_certs()
            .return_once(move |_| Ok(instance2_root_cert_pem_clone));

        let ping_notifier = Arc::new(Notify::new());
        let ping_notifier_clone = ping_notifier.clone();
        mock_client.expect_ping().returning(move |_request| {
            ping_notifier_clone.notify_one();
            Ok(PingResponse::default())
        });

        let mut mock_connector = Box::new(MockMeshConnector::new());
        mock_connector.expect_try_connect().return_once(move |_, _, _| Ok(mock_client));

        let tokio_runtime =
            Arc::new(tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap());

        let (ca_command_sender, mut ca_command_receiver) = unbounded_channel();
        let mesh_mode = MeshMode::Locked(LockedMesh {
            local_root_cert: b"root_cert_PEM".to_vec(),
            ca_command_sender,
        });

        let manager = MeshManagerBuilder::new()
            .with_connector(mock_connector)
            .with_vvm_config(Arc::new(create_test_vvm_config().unwrap()))
            .with_mesh_mode(mesh_mode)
            .build();

        let remote_vm_cfg = &manager.vvm_config.remote_cfgs[0];

        assert!(manager
            .service_registry
            .get_all_service_units_by_vm_name(VM_INSTANCE_2_NAME)
            .is_empty());

        let manager_clone = manager.clone();
        let remote_vm_cfg_clone = remote_vm_cfg.clone();
        let tokio_runtime_clone = tokio_runtime.clone();
        let _watcher_handle = tokio_runtime.spawn(async move {
            watcher_task(manager_clone, remote_vm_cfg_clone, tokio_runtime_clone).await;
        });

        let manager_clone = manager.clone();
        let remote_vm_cfg_clone = remote_vm_cfg.clone();
        tokio_runtime.block_on(async {
            let manager = manager_clone.clone();
            let vm_name = remote_vm_cfg_clone.name.clone();
            wait_for(move || {
                let peers = manager.peers.lock().unwrap().clone();
                peers.len() == 1 && peers.contains(&vm_name)
            })
            .await
            .unwrap();

            wait_for_instance2_service_units_registration(manager_clone.service_registry)
                .await
                .unwrap();

            // Ensure wather_task doesn't sleep before the next loop iteration.
            // Otherwise the test would take longer than necessary.
            manager_clone.wake_notifier.send(()).unwrap();

            // Wait until mock_client receives a ping() call.
            ping_notifier.notified().await;
        });

        let acl_policy_model = manager
            .mesh_acl_policy_provider
            .get_acl_policy(
                &remote_vm_cfg.name,
                &PackageName::from_str("com.sdv.sd_test").unwrap(),
                Some(&ServiceBundleName::from_str("MeshManagerTest").unwrap()),
            )
            .unwrap();
        assert!(acl_policy_model
            .has_service_unit(&ServiceUnitName::from_str("test-service-unit").unwrap()));

        // Check that MeshManager forwarded the root cert it received in exchange_root_certs()
        // to the CA service via the channel.
        receive_command_add_root_cert(
            ca_command_receiver.borrow_mut(),
            VM_INSTANCE_2_NAME,
            &instance2_root_cert_pem,
            &tokio_runtime,
        )
        .unwrap();
        assert!(ca_command_receiver.is_empty());

        Ok(())
    }

    #[test]
    fn watcher_task_reconnects_on_failed_ping() -> anyhow::Result<()> {
        let tokio_runtime =
            Arc::new(tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap());

        let (connector, clients_tx) = FakeMeshConnector::new();

        // First connection attempt will use a client that fails on ping.
        let mut mock_client = Box::new(MockMeshManagerAsyncClient::new());
        expect_get_peer_local_acls(mock_client.borrow_mut());
        let service_unit_events = create_instance2_service_unit_events();
        let instance2_root_cert_pem = create_root_cert_pem_for_vm(VM_INSTANCE_2_NAME).unwrap();
        mock_client
            .expect_subscribe_peer_local_service_units()
            .return_once(move |_| Ok(Box::pin(FakePeerUnitStream::new(service_unit_events))));
        let instance2_root_cert_pem_clone = instance2_root_cert_pem.clone();
        mock_client
            .expect_exchange_root_certs()
            .return_once(move |_| Ok(instance2_root_cert_pem_clone));
        let ping_notifier = Arc::new(Notify::new());
        let ping_notifier_clone = ping_notifier.clone();
        mock_client.expect_ping().returning(move |_request| {
            ping_notifier_clone.notify_one();
            Err(GrpcError::RpcFailure(RpcStatus::new(RpcStatusCode::DEADLINE_EXCEEDED)))
        });
        clients_tx.send(Ok(mock_client)).unwrap();

        let (ca_command_sender, mut ca_command_receiver) = unbounded_channel();
        let mesh_mode = MeshMode::Locked(LockedMesh {
            local_root_cert: b"root_cert_PEM".to_vec(),
            ca_command_sender,
        });

        let manager = MeshManagerBuilder::new()
            .with_connector(Box::new(connector))
            .with_mesh_mode(mesh_mode)
            .build();

        let remote_vm_cfg = &manager.vvm_config.remote_cfgs[0];

        assert!(manager
            .service_registry
            .get_all_service_units_by_vm_name(VM_INSTANCE_2_NAME)
            .is_empty());

        let manager_clone = manager.clone();
        let remote_vm_cfg_clone = remote_vm_cfg.clone();
        let tokio_runtime_clone = tokio_runtime.clone();
        let instance2_root_cert_pem_reconnect =
            create_root_cert_pem_for_vm(VM_INSTANCE_2_NAME).unwrap();
        let _watcher_handle = tokio_runtime.spawn(async move {
            watcher_task(manager_clone, remote_vm_cfg_clone, tokio_runtime_clone).await;
        });

        let manager_clone = manager.clone();
        let remote_vm_cfg_clone = remote_vm_cfg.clone();
        tokio_runtime.block_on(async {
            let manager = manager_clone.clone();
            let vm_name = remote_vm_cfg_clone.name.clone();
            wait_for(move || manager.peers.lock().unwrap().contains(&vm_name)).await.unwrap();

            wait_for_instance2_service_units_registration(manager_clone.service_registry.clone())
                .await
                .unwrap();

            // Ensure wather_task doesn't sleep before the next loop iteration.
            // Otherwise the test would take longer than necessary.
            manager_clone.wake_notifier.send(()).unwrap();

            // Mock client should get a ping(), which will fail.
            ping_notifier.notified().await;

            // The peer will be removed from the set
            let manager = manager_clone.clone();
            wait_for(move || manager.peers.lock().unwrap().is_empty()).await.unwrap();

            // The service units for that remote VM will be removed.
            let service_registry = manager_clone.service_registry.clone();
            wait_for(move || {
                service_registry.get_all_service_units_by_vm_name(VM_INSTANCE_2_NAME).is_empty()
            })
            .await
            .unwrap();

            // Prepare and send the second client for reconnection.
            let mut mock_client_reconnect = Box::new(MockMeshManagerAsyncClient::new());
            expect_get_peer_local_acls(mock_client_reconnect.borrow_mut());
            let service_unit_events = create_instance2_service_unit_events();
            mock_client_reconnect
                .expect_subscribe_peer_local_service_units()
                .return_once(move |_| Ok(Box::pin(FakePeerUnitStream::new(service_unit_events))));
            let pem_clone = instance2_root_cert_pem_reconnect.clone();
            mock_client_reconnect.expect_exchange_root_certs().return_once(move |_| Ok(pem_clone));
            mock_client_reconnect.expect_ping().returning(|_request| Ok(PingResponse::default()));
            clients_tx.send(Ok(mock_client_reconnect as Box<dyn MeshManagerAsyncClient>)).unwrap();

            // The remote peer has been added back again.
            let manager = manager_clone.clone();
            let vm_name = remote_vm_cfg_clone.name.clone();
            wait_for(move || manager.peers.lock().unwrap().contains(&vm_name)).await.unwrap();

            // The service units from the remote VM are added back again
            wait_for_instance2_service_units_registration(manager_clone.service_registry.clone())
                .await
                .unwrap();
        });

        let acl_policy_model = manager
            .mesh_acl_policy_provider
            .get_acl_policy(
                &remote_vm_cfg.name,
                &PackageName::from_str("com.sdv.sd_test").unwrap(),
                Some(&ServiceBundleName::from_str("MeshManagerTest").unwrap()),
            )
            .unwrap();
        assert!(acl_policy_model
            .has_service_unit(&ServiceUnitName::from_str("test-service-unit").unwrap()));

        // Check that MeshManager forwarded the root cert it received in exchange_root_certs()
        // to the CA service via the channel.
        receive_command_add_root_cert(
            ca_command_receiver.borrow_mut(),
            VM_INSTANCE_2_NAME,
            &instance2_root_cert_pem,
            &tokio_runtime,
        )
        .unwrap();
        // Disconnection due to failed ping triggers cert removal
        receive_command_remove_root_cert(
            ca_command_receiver.borrow_mut(),
            VM_INSTANCE_2_NAME,
            &tokio_runtime,
        )?;
        // Reconnection causes cert to be added back again
        receive_command_add_root_cert(
            ca_command_receiver.borrow_mut(),
            VM_INSTANCE_2_NAME,
            &instance2_root_cert_pem_reconnect,
            &tokio_runtime,
        )?;
        assert!(ca_command_receiver.is_empty());

        Ok(())
    }

    /// If a connection attempt fails, checks that the watcher_task() will try again
    /// after a delay.
    #[test]
    fn watcher_task_connection_failure() -> anyhow::Result<()> {
        let tokio_runtime =
            Arc::new(tokio::runtime::Builder::new_current_thread().enable_all().build()?);

        let mut mock_connector = MockMeshConnector::new();
        let mut seq = mockall::Sequence::new();

        let first_attempt_notifier = Arc::new(Notify::new());
        let first_attempt_notifier_clone = first_attempt_notifier.clone();

        mock_connector.expect_try_connect().times(1).in_sequence(&mut seq).returning(
            move |_, _, _| {
                first_attempt_notifier_clone.notify_one();
                Err(anyhow!("Connection failed"))
            },
        );
        let instance2_root_cert_pem = create_root_cert_pem_for_vm(VM_INSTANCE_2_NAME).unwrap();
        let instance2_root_cert_pem_clone = instance2_root_cert_pem.clone();

        mock_connector.expect_try_connect().times(1).in_sequence(&mut seq).returning(
            move |_, _, _| {
                let mut mock_client = Box::new(MockMeshManagerAsyncClient::new());
                expect_get_peer_local_acls(mock_client.borrow_mut());
                let service_unit_events = create_instance2_service_unit_events();
                mock_client.expect_subscribe_peer_local_service_units().return_once(move |_| {
                    Ok(Box::pin(FakePeerUnitStream::new(service_unit_events)))
                });
                let pem_clone = instance2_root_cert_pem_clone.clone();
                mock_client.expect_exchange_root_certs().return_once(move |_| Ok(pem_clone));
                mock_client.expect_ping().returning(|_request| Ok(PingResponse::default()));
                Ok(mock_client)
            },
        );

        let (ca_command_sender, mut ca_command_receiver) = unbounded_channel();
        let mesh_mode = MeshMode::Locked(LockedMesh {
            local_root_cert: b"root_cert_PEM".to_vec(),
            ca_command_sender,
        });

        let manager = MeshManagerBuilder::new()
            .with_connector(Box::new(mock_connector))
            .with_mesh_mode(mesh_mode)
            .build();

        let remote_vm_cfg = &manager.vvm_config.remote_cfgs[0];

        let manager_clone = manager.clone();
        let remote_vm_cfg_clone = remote_vm_cfg.clone();
        let tokio_runtime_clone = tokio_runtime.clone();
        let _watcher_handle = tokio_runtime.spawn(async move {
            watcher_task(manager_clone, remote_vm_cfg_clone, tokio_runtime_clone).await;
        });

        tokio_runtime.block_on(async {
            // Wait for the first connection attempt to fail.
            first_attempt_notifier.notified().await;

            // Wake up the watcher task so it doesn't sleep.
            manager.wake_notifier.send(()).unwrap();

            // Wait for reconnection and successful connection.
            let manager_clone = manager.clone();
            let vm_name = remote_vm_cfg.name.clone();
            wait_for(move || manager_clone.peers.lock().unwrap().contains(&vm_name)).await.unwrap();

            // Service units should be registered now.
            wait_for_instance2_service_units_registration(manager.service_registry.clone())
                .await
                .unwrap();
        });

        // Check that CA command was sent.
        receive_command_add_root_cert(
            ca_command_receiver.borrow_mut(),
            VM_INSTANCE_2_NAME,
            &instance2_root_cert_pem,
            &tokio_runtime,
        )?;
        assert!(ca_command_receiver.is_empty());

        Ok(())
    }

    /// Tests that the peer connection is considered as failed if the get_peer_local_acls()
    /// times out.
    #[test]
    fn watcher_task_get_acls_timeout_err() -> anyhow::Result<()> {
        let tokio_runtime =
            Arc::new(tokio::runtime::Builder::new_current_thread().enable_all().build()?);

        let mut mock_connector = Box::new(MockMeshConnector::new());
        let mut seq = Sequence::new();

        let instance2_root_cert_pem_fail = create_root_cert_pem_for_vm(VM_INSTANCE_2_NAME)?;
        let instance2_root_cert_pem_success = create_root_cert_pem_for_vm(VM_INSTANCE_2_NAME)?;

        // First connection attempt will fail on get_peer_local_acls
        let first_attempt_notifier = Arc::new(Notify::new());
        let first_attempt_notifier_clone = first_attempt_notifier.clone();
        let cert_clone_fail = instance2_root_cert_pem_fail.clone();
        mock_connector.expect_try_connect().times(1).in_sequence(&mut seq).returning(
            move |_, _, _| {
                let mut mock_client = Box::new(MockMeshManagerAsyncClient::new());
                let cert_clone = cert_clone_fail.clone();
                mock_client.expect_exchange_root_certs().return_once(move |_| Ok(cert_clone));
                let notifier_clone = first_attempt_notifier_clone.clone();
                mock_client.expect_get_peer_local_acls().return_once(move |_| {
                    notifier_clone.notify_one();
                    Err(GrpcError::RpcFailure(RpcStatus::new(RpcStatusCode::DEADLINE_EXCEEDED)))
                });
                Ok(mock_client)
            },
        );

        // Second connection attempt will succeed
        let cert_clone_success = instance2_root_cert_pem_success.clone();
        mock_connector.expect_try_connect().times(1).in_sequence(&mut seq).returning(
            move |_, _, _| {
                let mut mock_client = Box::new(MockMeshManagerAsyncClient::new());
                expect_get_peer_local_acls(mock_client.borrow_mut());
                let service_unit_events = create_instance2_service_unit_events();
                mock_client.expect_subscribe_peer_local_service_units().return_once(move |_| {
                    Ok(Box::pin(FakePeerUnitStream::new(service_unit_events)))
                });
                let pem_clone = cert_clone_success.clone();
                mock_client.expect_exchange_root_certs().return_once(move |_| Ok(pem_clone));
                mock_client.expect_ping().returning(|_request| Ok(PingResponse::default()));
                Ok(mock_client)
            },
        );

        let (ca_command_sender, mut ca_command_receiver) = unbounded_channel();
        let mesh_mode = MeshMode::Locked(LockedMesh {
            local_root_cert: b"root_cert_PEM".to_vec(),
            ca_command_sender,
        });

        let manager = MeshManagerBuilder::new()
            .with_connector(mock_connector)
            .with_mesh_mode(mesh_mode)
            .build();

        let remote_vm_cfg = &manager.vvm_config.remote_cfgs[0];

        let manager_clone = manager.clone();
        let remote_vm_cfg_clone = remote_vm_cfg.clone();
        let tokio_runtime_clone = tokio_runtime.clone();
        let _watcher_handle = tokio_runtime.spawn(async move {
            watcher_task(manager_clone, remote_vm_cfg_clone, tokio_runtime_clone).await
        });

        tokio_runtime.block_on(async {
            // Wait for the first connection attempt to fail on get_peer_local_acls.
            first_attempt_notifier.notified().await;

            // At this point, the peer should not be in the set.
            assert!(manager.peers.lock().unwrap().is_empty());

            // Wake up the watcher task so it doesn't sleep.
            manager.wake_notifier.send(()).unwrap();

            // Wait for reconnection and successful connection.
            let manager_clone = manager.clone();
            let vm_name = remote_vm_cfg.name.clone();
            wait_for(move || manager_clone.peers.lock().unwrap().contains(&vm_name)).await.unwrap();

            // Service units should be registered now.
            wait_for_instance2_service_units_registration(manager.service_registry.clone())
                .await
                .unwrap();
        });

        // Since the mock client exchange_root_certs() call succeeds, the peer root cert should be
        // added.
        receive_command_add_root_cert(
            ca_command_receiver.borrow_mut(),
            VM_INSTANCE_2_NAME,
            &instance2_root_cert_pem_fail,
            &tokio_runtime,
        )?;
        // But since the next mock client call, get_peer_local_acls(), fails, that peer root cert
        // addition has to be undone.
        receive_command_remove_root_cert(
            ca_command_receiver.borrow_mut(),
            VM_INSTANCE_2_NAME,
            &tokio_runtime,
        )?;
        // Root cert is added back again on the second connection attempt. As it's successful,
        // it's not followed by a removal.
        receive_command_add_root_cert(
            ca_command_receiver.borrow_mut(),
            VM_INSTANCE_2_NAME,
            &instance2_root_cert_pem_success,
            &tokio_runtime,
        )?;
        assert!(ca_command_receiver.is_empty());

        Ok(())
    }

    /// Tests that when MeshManager::spawn_watcher_tasks() is called. MeshMaanger connects all
    /// remote VMs listed in the vvmconfig.
    ///
    /// It checks that MeshManager::service_registry and MeshManager::peers are populated
    /// accordigly, that the ACLs are fetched and that that CA commands are sent to add
    /// the root certs of the remote VMs.
    #[test]
    fn spawn_watcher_tasks_connects_and_pings_remote_services() -> anyhow::Result<()> {
        let tokio_runtime =
            Arc::new(tokio::runtime::Builder::new_current_thread().enable_all().build()?);

        let vvm_config = Arc::new(create_test_vvm_config_2_remotes()?);

        let instance2_root_cert_pem = create_root_cert_pem_for_vm(VM_INSTANCE_2_NAME)?;
        let instance3_root_cert_pem = create_root_cert_pem_for_vm(VM_INSTANCE_3_NAME)?;

        let mut mock_connector = Box::new(MockMeshConnector::new());
        let i2_pem_clone = instance2_root_cert_pem.clone();
        let i3_pem_clone = instance3_root_cert_pem.clone();

        mock_connector.expect_try_connect().times(2).returning(move |_, _, vm_name| {
            let mut mock_client = Box::new(MockMeshManagerAsyncClient::new());
            expect_get_peer_local_acls(mock_client.borrow_mut());
            let service_unit_events = create_service_unit_events(vm_name);
            mock_client
                .expect_subscribe_peer_local_service_units()
                .return_once(move |_| Ok(Box::pin(FakePeerUnitStream::new(service_unit_events))));

            if vm_name == VM_INSTANCE_2_NAME {
                let pem_clone = i2_pem_clone.clone();
                mock_client.expect_exchange_root_certs().return_once(move |_| Ok(pem_clone));
            } else if vm_name == VM_INSTANCE_3_NAME {
                let pem_clone = i3_pem_clone.clone();
                mock_client.expect_exchange_root_certs().return_once(move |_| Ok(pem_clone));
            } else {
                panic!("Unexpected vm_name: {vm_name}");
            }

            mock_client.expect_ping().returning(|_request| Ok(PingResponse::default()));
            Ok(mock_client)
        });

        let (ca_command_sender, mut ca_command_receiver) = unbounded_channel();
        let mesh_mode = MeshMode::Locked(LockedMesh {
            local_root_cert: b"root_cert_PEM".to_vec(),
            ca_command_sender,
        });

        let manager = MeshManagerBuilder::new()
            .with_vvm_config(vvm_config)
            .with_connector(mock_connector)
            .with_mesh_mode(mesh_mode)
            .build();

        let manager_clone = manager.clone();
        let tokio_runtime_clone = tokio_runtime.clone();
        tokio_runtime.spawn(async move {
            manager_clone.spawn_watcher_tasks(tokio_runtime_clone).await;
        });

        let manager_clone = manager.clone();
        tokio_runtime.block_on(async move {
            wait_for(move || manager_clone.peers.lock().unwrap().len() == 2).await.unwrap();

            wait_for_remote_service_units_registration(
                manager_clone.service_registry.clone(),
                VM_INSTANCE_2_NAME,
            )
            .await
            .unwrap();
            wait_for_remote_service_units_registration(
                manager_clone.service_registry.clone(),
                VM_INSTANCE_3_NAME,
            )
            .await
            .unwrap();
        });

        let peers = manager.peers.lock().unwrap();
        assert_eq!(peers.len(), 2);
        assert!(peers.contains(&VmName::from_str(VM_INSTANCE_2_NAME)?));
        assert!(peers.contains(&VmName::from_str(VM_INSTANCE_3_NAME)?));

        // Check CA commands
        let mut received_certs = HashMap::new();
        for _ in 0..2 {
            let (vm_name, cert) = match ca_command_receiver.try_recv() {
                Ok(RootCertCommand::Add { vm_name, root_cert }) => (vm_name, root_cert),
                e => panic!("Expected Add command, got {e:?}"),
            };
            received_certs.insert(vm_name, cert);
        }

        assert_eq!(received_certs.get(VM_INSTANCE_2_NAME).unwrap(), &instance2_root_cert_pem);
        assert_eq!(received_certs.get(VM_INSTANCE_3_NAME).unwrap(), &instance3_root_cert_pem);
        assert!(ca_command_receiver.is_empty());

        Ok(())
    }

    /// In an unlocked mesh, mesh::manager should get the UDS public keys of the remote VM and
    /// pass them to the local UdsPubsProvisioner
    #[test]
    fn unlocked_mesh_manager_gets_uds_pub() -> anyhow::Result<()> {
        let tokio_runtime =
            Arc::new(tokio::runtime::Builder::new_current_thread().enable_all().build()?);

        let vvm_config = Arc::new(create_test_vvm_config()?);
        let remote_vm_cfg = &vvm_config.remote_cfgs[0];

        // The UDS keys the remote peer will present.
        let remote_uds_keys = vec![create_example_key1(), create_example_key2()];
        let remote_uds_keys_cbor =
            remote_uds_keys.iter().map(|k| k.clone().to_vec().unwrap()).collect::<Vec<Vec<u8>>>();

        // Set up mocks
        let mut mock_client = Box::new(MockMeshManagerAsyncClient::new());
        expect_get_peer_local_acls(mock_client.borrow_mut());
        mock_client
            .expect_subscribe_peer_local_service_units()
            .return_once(move |_| Ok(Box::pin(FakePeerUnitStream::new(vec![]))));
        let keys_clone = remote_uds_keys_cbor.clone();
        mock_client.expect_get_uds_pub().return_once(move || Ok(keys_clone));
        mock_client.expect_ping().returning(|_request| Ok(PingResponse::default()));

        let mut mock_connector = Box::new(MockMeshConnector::new());
        mock_connector.expect_try_connect().return_once(move |_, _, _| Ok(mock_client));

        // Set up unlocked mesh mode with a provisioner
        let local_vm_uds_pubs = vec![create_example_key1()];
        let provisioner = Arc::new(Mutex::new(UdsPubsProvisioner::try_new(
            &vvm_config,
            local_vm_uds_pubs,
            tokio_runtime.clone(),
        )?));
        let mesh_mode =
            MeshMode::Unlocked(UnlockedMesh { uds_pubs_provisioner: provisioner.clone() });

        let manager = MeshManagerBuilder::new()
            .with_vvm_config(vvm_config.clone())
            .with_connector(mock_connector)
            .with_mesh_mode(mesh_mode)
            .build();

        // Initially, the provisioner only knows about the local VM.
        assert_eq!(provisioner.lock().unwrap().keys_map().len(), 1);
        assert!(provisioner.lock().unwrap().keys_map().contains_key(VM_INSTANCE_1_NAME));

        // Spawn the watcher task
        let manager_clone = manager.clone();
        let remote_vm_cfg_clone = remote_vm_cfg.clone();
        let tokio_runtime_clone = tokio_runtime.clone();
        let _watcher_handle = tokio_runtime.spawn(async move {
            watcher_task(manager_clone, remote_vm_cfg_clone, tokio_runtime_clone).await;
        });

        // Wait for the watcher to connect and update the provisioner
        tokio_runtime.block_on(async {
            wait_for(move || {
                let guarded_provisioner = provisioner.lock().unwrap();
                guarded_provisioner.keys_map().get(VM_INSTANCE_2_NAME) == Some(&remote_uds_keys)
            })
            .await
        })?;

        Ok(())
    }

    #[tokio::test]
    async fn do_exponential_delay_timeout_doubles_next_delay() {
        // Very short so that the test doesn't take long to run.
        const MIN_SCAN_INTERVAL: Duration = Duration::from_millis(10);

        const MAX_DELAY_MULTIPLIER: u32 = 4;

        let mut delay_multiplier = 1;
        let (_wake_sender, mut wake_receiver) = broadcast::channel(1);
        let (_power_state_tx, mut power_state_rx) = broadcast::channel(16);

        let vm_name = VmName::from_str("test-vm").unwrap();

        let wake_up_reason = do_exponential_delay_internal(
            MIN_SCAN_INTERVAL,
            MAX_DELAY_MULTIPLIER,
            &mut delay_multiplier,
            &mut wake_receiver,
            &mut power_state_rx,
            &vm_name,
        )
        .await;
        assert_eq!(wake_up_reason, WakeUpReason::Timeout);
        assert_eq!(delay_multiplier, 2);

        let wake_up_reason = do_exponential_delay_internal(
            MIN_SCAN_INTERVAL,
            MAX_DELAY_MULTIPLIER,
            &mut delay_multiplier,
            &mut wake_receiver,
            &mut power_state_rx,
            &vm_name,
        )
        .await;
        assert_eq!(wake_up_reason, WakeUpReason::Timeout);
        assert_eq!(delay_multiplier, MAX_DELAY_MULTIPLIER);

        // Check that it's capped at MAX_DELAY_MULTIPLIER
        let wake_up_reason = do_exponential_delay_internal(
            MIN_SCAN_INTERVAL,
            MAX_DELAY_MULTIPLIER,
            &mut delay_multiplier,
            &mut wake_receiver,
            &mut power_state_rx,
            &vm_name,
        )
        .await;
        assert_eq!(wake_up_reason, WakeUpReason::Timeout);
        assert_eq!(delay_multiplier, MAX_DELAY_MULTIPLIER);
    }

    #[tokio::test]
    async fn do_exponential_delay_interrupted() {
        // Some arbitrarily long time so that the timeout in do_exponential_delay_internal
        // doesn't happen.
        const MIN_SCAN_INTERVAL: Duration = Duration::from_secs(60 * 60 /* 1 hour */);
        const MAX_DELAY_MULTIPLIER: u32 = 4;

        let (wake_sender, mut wake_receiver) = broadcast::channel(1);
        let (_power_state_tx, mut power_state_rx) = broadcast::channel(16);

        let vm_name = VmName::from_str("test-vm").unwrap();

        let task = tokio::spawn(async move {
            let mut delay_multiplier = MAX_DELAY_MULTIPLIER;
            let wake_up_reason = do_exponential_delay_internal(
                MIN_SCAN_INTERVAL,
                MAX_DELAY_MULTIPLIER,
                &mut delay_multiplier,
                &mut wake_receiver,
                &mut power_state_rx,
                &vm_name,
            )
            .await;
            // Return the state of delay_multiplier after the call along with the wakeup reason.
            (wake_up_reason, delay_multiplier)
        });

        // Let the task run a bit and start waiting on the receiver.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Send a wake notification. This should interrupt the sleep inside
        // do_exponential_delay_internal()
        wake_sender.send(()).unwrap();

        // Wait for the task to finish.
        let (wake_up_reason, final_delay_multiplier) = task.await.unwrap();

        // Next delay is reset when sleep is interrupted by new client notification.
        assert_eq!(final_delay_multiplier, 1);
        assert_eq!(wake_up_reason, WakeUpReason::Notification);
    }

    #[tokio::test]
    async fn watcher_tasks_pause_and_resume() -> anyhow::Result<()> {
        let power_state_tx = broadcast::Sender::new(16);
        let (ca_command_sender, mut ca_command_receiver) = unbounded_channel();

        let mesh_mode = MeshMode::Locked(LockedMesh {
            local_root_cert: create_root_cert_pem_for_vm(VM_INSTANCE_1_NAME)?,
            ca_command_sender,
        });

        let instance2_root_cert_pem = create_root_cert_pem_for_vm(VM_INSTANCE_2_NAME).unwrap();

        // Mock connector and client
        let (connector, clients_tx) = FakeMeshConnector::new();

        let manager = MeshManagerBuilder::new()
            .with_connector(Box::new(connector))
            .with_mesh_mode(mesh_mode)
            .with_power_state_tx(power_state_tx.clone())
            .build();

        let watcher_state_rx = manager.subscribe_to_watcher_thread_state();

        // Spawn watcher tasks
        let manager_clone = manager.clone();
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let tokio_runtime_clone = tokio_runtime.clone();
        tokio_runtime.spawn(async move {
            manager_clone.spawn_watcher_tasks(tokio_runtime_clone).await;
        });

        // --- Initial run: Wait for watcher to be running and connected ---
        // Provide a client for the first connection
        let mut mock_client = Box::new(MockMeshManagerAsyncClient::new());
        let pem_clone = instance2_root_cert_pem.clone();
        mock_client.expect_exchange_root_certs().return_once(move |_| Ok(pem_clone));
        expect_get_peer_local_acls(mock_client.borrow_mut());
        let service_unit_events = create_instance2_service_unit_events();
        mock_client
            .expect_subscribe_peer_local_service_units()
            .return_once(move |_| Ok(Box::pin(FakePeerUnitStream::new(service_unit_events))));
        mock_client.expect_ping().returning(|_| Ok(Default::default()));
        clients_tx.send(Ok(mock_client)).unwrap();

        let mut watcher_state_rx_clone = watcher_state_rx.clone();
        watcher_state_rx_clone.wait_for(|s| *s == WatcherState::Running).await.unwrap();

        let remote_vm_name = manager.vvm_config.remote_cfgs[0].name.clone();
        let manager_clone = manager.clone();
        wait_for(move || manager_clone.peers.lock().unwrap().contains(&remote_vm_name)).await?;

        let ca_command = ca_command_receiver.recv().await.unwrap();
        match ca_command {
            RootCertCommand::Add { vm_name, root_cert } => {
                assert_eq!(vm_name, VM_INSTANCE_2_NAME);
                assert_eq!(root_cert, instance2_root_cert_pem);
            }
            _ => panic!("Expected Add command"),
        }
        assert!(ca_command_receiver.is_empty());

        wait_for_instance2_service_units_registration(manager.service_registry.clone()).await?;

        // --- Simulate Power Down (Pause) ---
        power_state_tx.send(SimplePowerState::Off)?;

        let mut watcher_state_rx_clone = watcher_state_rx.clone();
        watcher_state_rx_clone.wait_for(|s| *s == WatcherState::Paused).await.unwrap();

        // Verify disconnection
        let remote_vm_name = manager.vvm_config.remote_cfgs[0].name.clone();
        let manager_clone = manager.clone();
        let remote_vm_name_clone = remote_vm_name.clone();
        wait_for(move || !manager_clone.peers.lock().unwrap().contains(&remote_vm_name_clone))
            .await?;

        let ca_command = ca_command_receiver.recv().await.unwrap();
        match ca_command {
            RootCertCommand::Remove { vm_name, root_cert } => {
                assert_eq!(vm_name, VM_INSTANCE_2_NAME);
                assert_eq!(root_cert, instance2_root_cert_pem);
            }
            _ => panic!("Expected Remove command"),
        }
        assert!(manager
            .service_registry
            .get_all_service_units_by_vm_name(remote_vm_name.as_ref())
            .is_empty());

        // --- Simulate Power Up (Resume) ---
        power_state_tx.send(SimplePowerState::On)?;

        // Provide a client for reconnection
        let mut mock_client_reconnect = Box::new(MockMeshManagerAsyncClient::new());
        let pem_clone = instance2_root_cert_pem.clone();
        mock_client_reconnect.expect_exchange_root_certs().return_once(move |_| Ok(pem_clone));
        expect_get_peer_local_acls(mock_client_reconnect.borrow_mut());
        let service_unit_events = create_instance2_service_unit_events();
        mock_client_reconnect
            .expect_subscribe_peer_local_service_units()
            .return_once(move |_| Ok(Box::pin(FakePeerUnitStream::new(service_unit_events))));
        mock_client_reconnect.expect_ping().returning(|_| Ok(Default::default()));
        clients_tx.send(Ok(mock_client_reconnect)).unwrap();

        let mut watcher_state_rx_clone = watcher_state_rx.clone();
        watcher_state_rx_clone.wait_for(|s| *s == WatcherState::Running).await.unwrap();

        // Verify reconnection
        let remote_vm_name = manager.vvm_config.remote_cfgs[0].name.clone();
        let manager_clone = manager.clone();
        wait_for(move || manager_clone.peers.lock().unwrap().contains(&remote_vm_name)).await?;

        let ca_command = ca_command_receiver.recv().await.unwrap();
        match ca_command {
            RootCertCommand::Add { vm_name, root_cert } => {
                assert_eq!(vm_name, VM_INSTANCE_2_NAME);
                assert_eq!(root_cert, instance2_root_cert_pem);
            }
            _ => panic!("Expected Add command"),
        }
        assert!(ca_command_receiver.is_empty());

        wait_for_instance2_service_units_registration(manager.service_registry.clone()).await?;

        Ok(())
    }

    #[test]
    fn do_exponential_delay_interrupted_by_power_state_event() {
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());

        tokio_runtime.block_on(async {
            let (wake_notifier, mut wake_receiver) = broadcast::channel(1);

            let (power_state_report_tx, mut power_state_rx) = broadcast::channel(16);

            let mut delay_multiplier = 1;
            let vm_name = VmName::from_str("test-vm").unwrap();

            let wake_up_reason = do_exponential_delay_internal(
                Duration::from_millis(100), // min scan interval. Short so that it times out fast.
                MAX_SCAN_INTERVAL_MULTIPLIER,
                &mut delay_multiplier,
                &mut wake_receiver,
                &mut power_state_rx,
                &vm_name,
            )
            .await;
            // It should time out, causing the delay to double.
            assert_eq!(wake_up_reason, WakeUpReason::Timeout);
            assert_eq!(delay_multiplier, 2);

            let handle = {
                let wake_notifier = wake_notifier.clone();
                let mut power_state_rx = power_state_report_tx.subscribe();
                tokio::spawn(async move {
                    let mut delay_multiplier = 4;
                    let vm_name = VmName::from_str("test-vm").unwrap();
                    // A new receiver is needed for the spawned task.
                    let mut wake_receiver = wake_notifier.subscribe();
                    let wake_up_reason = do_exponential_delay_internal(
                        Duration::from_secs(100), // long interval to ensure it's interrupted
                        MAX_SCAN_INTERVAL_MULTIPLIER,
                        &mut delay_multiplier,
                        &mut wake_receiver,
                        &mut power_state_rx,
                        &vm_name,
                    )
                    .await;
                    assert_eq!(wake_up_reason, WakeUpReason::PowerState(SimplePowerState::On));
                    // delay_multiplier should have been reset.
                    assert_eq!(delay_multiplier, 1);
                })
            };

            // Let the spawned task run for a bit.
            tokio::time::sleep(Duration::from_millis(50)).await;

            // Send a new power state report. This should interrupt the delay.
            power_state_report_tx.send(SimplePowerState::On).unwrap();

            // This should happen roughly immediately. But timeout anyway, in case
            // do_exponential_delay_internal() is not woken up (ie, test failure).
            tokio::time::timeout(Duration::from_secs(5), handle).await.unwrap().unwrap();
        });
    }

    /// Tests watcher_task() in a scenario where power goes off and then back to on.
    ///
    /// More specifically, the following steps:
    /// - Expectation: watcher_task() attemtps to connect.
    /// - Test: A connection to the peer VM is established
    /// - Expectation: watcher_task() reports the new connection
    /// - Test: Power state changes to Off
    /// - Expectation: watcher_task() drops the connection.
    /// - Test: Power state changes to On
    /// - It's expect that watcher_task() tries to reconnect.
    /// - Test: The reconnection attempt will fail
    /// - Expectation: watcher_task() will attempt to reconnect.
    /// - Test: The second reconnection attempt will succeed.
    /// - Expectation: watcher_task() reports the new connection
    #[test]
    fn watcher_task_reconnects_after_power_cycle() {
        let power_state_tx = broadcast::Sender::new(16);
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());

        let vvm_config = create_test_vvm_config().unwrap();

        let provisioner = Arc::new(Mutex::new(
            UdsPubsProvisioner::try_new(
                &vvm_config,
                vec![create_example_key1()],
                tokio_runtime.clone(),
            )
            .unwrap(),
        ));

        // Using an unlocked mesh for setup simplicity
        let mesh_mode = MeshMode::Unlocked(UnlockedMesh { uds_pubs_provisioner: provisioner });

        let (connector, clients_tx) = FakeMeshConnector::new();

        let manager = MeshManagerBuilder::new()
            .with_vvm_config(Arc::new(vvm_config))
            .with_connector(Box::new(connector))
            .with_mesh_mode(mesh_mode)
            .with_power_state_tx(power_state_tx.clone())
            .build();

        let manager_clone = manager.clone();
        let remote_vm_cfg = manager.vvm_config.remote_cfgs[0].clone();
        let tokio_runtime_clone = tokio_runtime.clone();
        tokio_runtime.spawn(async move {
            watcher_task(manager_clone, remote_vm_cfg, tokio_runtime_clone).await;
        });

        // --- Initial connection ---
        clients_tx.send(Ok(create_unlocked_mesh_mock_client())).unwrap();

        let remote_vm_name = manager.vvm_config.remote_cfgs[0].name.clone();
        let manager_clone = manager.clone();
        tokio_runtime
            .block_on(async move {
                wait_for(move || manager_clone.peers.lock().unwrap().contains(&remote_vm_name))
                    .await
            })
            .unwrap();

        // --- Power Off ---
        power_state_tx.send(SimplePowerState::Off).unwrap();

        let manager_clone = manager.clone();
        tokio_runtime
            .block_on(async move {
                wait_for(move || manager_clone.peers.lock().unwrap().is_empty()).await
            })
            .unwrap();

        // --- Power On and Reconnect ---
        power_state_tx.send(SimplePowerState::On).unwrap();

        // The first connection attempt fails. We expect watcher_task() to try again
        // after a short timeout.
        clients_tx.send(Err(anyhow!("Some temporary failure."))).unwrap();

        // The second attempt will succeed.
        clients_tx.send(Ok(create_unlocked_mesh_mock_client())).unwrap();

        // Without the fix, the watcher task would get stuck and this would time out.
        let remote_vm_name = manager.vvm_config.remote_cfgs[0].name.clone();
        let manager_clone = manager.clone();
        tokio_runtime
            .block_on(async move {
                wait_for(move || manager_clone.peers.lock().unwrap().contains(&remote_vm_name))
                    .await
            })
            .unwrap();
    }
}
