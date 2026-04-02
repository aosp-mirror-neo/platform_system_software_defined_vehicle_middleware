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

use sdv_authz::SingleVmAclPolicyProvider;
use sdv_sd_mesh_api::identity::ServiceUnitIdentifier;
use sdv_sd_mesh_api::sdv_sd_mesh::{
    peer_local_service_units_event::Event::{AddServiceUnit, RemoveByIdentity, RemoveServiceUnit},
    AddServiceUnitEvent, ExchangeRootCertsRequest, ExchangeRootCertsResponse,
    GetPeerLocalAclsRequest, GetPeerLocalAclsResponse, GetUdsPubRequest, GetUdsPubResponse,
    PeerLocalServiceUnitsEvent, PingRequest, PingResponse, RemoveByIdentityEvent,
    RemoveServiceUnitEvent, SubscribePeerLocalServiceUnitsRequest,
};
use sdv_sd_mesh_api::sdv_sd_mesh_grpc::{create_mesh_manager, MeshManager};

use crate::authz::proto::to_peer_local_acls_proto;
use crate::ca::service::RootCertCommand;
use crate::kv::{ServiceRegistry, ServiceUnitEvent};
use crate::mesh::aidl_to_proto::ToProto;
use crate::mesh::credentials_checker::CredentialsChecker;
use crate::mesh::custom_verifier::make_server_credentials;
use crate::mesh::sd_certificate_claims::SdCertificateClaims;
use sdv_comms_id::names::VmName;
use std::str::FromStr;

use crate::power_state::SimplePowerState;

use crate::mesh::VvmConfig;
use crypto_rpc::cert::base32_x509_common_name_to_string;
use std::ops::Mul;
use std::sync::Arc;

use sdv_authz_local_vm_acl_policy_provider_factory::create_local_vm_acl_policy_provider;

use futures::{stream, SinkExt, StreamExt};
use futures_util::future::{FutureExt as _, TryFutureExt as _};
use tokio_stream::wrappers::UnboundedReceiverStream;

use openssl::{
    pkey::{PKey, Private},
    x509::X509,
};

use grpcio::{
    AuthContext, ChannelBuilder, Environment, RpcContext, RpcStatus, RpcStatusCode,
    ServerCredentials, ServerStreamingSink, UnarySink, WriteFlags,
};

use async_trait::async_trait;
use coset::{CborSerializable, CoseError, CoseKey};
use log::{error, info, warn};
use mockall::automock;
use protobuf::MessageField;
use tokio::select;
use tokio::sync::{broadcast, watch};
use tokio::time::sleep;
use tokio::time::Duration;
use tracing::instrument;

/// When this mesh service is called by the mesh manager client of a new peer VM, we expect that
/// the local mesh manager client will connect to that peer's mesh service within that period (in
/// seconds).
const MESH_MANAGER_GRACE_PERIOD_SECS: u64 = 10;

/// Minimum time before retrying to start the gRPC server.
const START_SERVER_RETRY_MIN_DURATION: Duration = Duration::from_secs(1);
/// Maximum time before retrying to start the gRPC server.
const START_SERVER_RETRY_MAX_DURATION: Duration = Duration::from_secs(16);

#[derive(Clone)]
pub(crate) struct LockedMesh {
    /// The PEM-encoded X.509 root certificate of the local VM.
    root_cert: Vec<u8>,

    /// Sender for CA commands.
    ca_command_sender: tokio::sync::mpsc::UnboundedSender<RootCertCommand>,

    /// Tokio runtime used to spawn a task that will remove a root cert after a grace period.
    tokio_runtime: Arc<tokio::runtime::Runtime>,

    /// Sleeper for MESH_MANAGER_GRACE_PERIOD_SECS seconds
    grace_period_sleeper: Arc<dyn GracePeriodSleeper>,
}

#[derive(Clone)]
pub(crate) struct UnlockedMesh {
    /// The UDS public keys of the device where this VM is running.
    /// Encoded as COSE_Key.
    /// Normally it would be only one key, but in some cases it might be two, hence the vector.
    pub local_vm_uds_pubs: Vec<Vec<u8>>,
}

/// Specifies in which mode the mesh is and carries any extra data that MeshManagerService needs in
/// that particular mesh mode.
#[derive(Clone)]
pub(crate) enum MeshMode {
    /// Mesh is locked (secure). We use a CA with root certificates that are generated at runtime
    /// and exchanged over the secure mesh.
    Locked(LockedMesh),

    /// Mesh is unlocked (insecure). State used for provisioning of the uds_pubs file and for
    /// development.
    Unlocked(UnlockedMesh),
}

/// Trait for sleeping until MESH_MANAGER_GRACE_PERIOD_SECS is elapsed.
///
/// Note that this trait exists only to make unit testing possible.  Ideally we would use
/// tokio::time::pause() and related functions instead.  But on Android it's made available in
/// libtokio_for_test and this lib doesn't seem to be compatible with other tokio libraries such as
/// libtokio_stream.
#[async_trait]
trait GracePeriodSleeper: Send + Sync {
    /// Sleeps for MESH_MANAGER_GRACE_PERIOD_SECS seconds
    async fn sleep(&self);
}
struct GracePeriodSleeperImpl {}
#[async_trait]
impl GracePeriodSleeper for GracePeriodSleeperImpl {
    async fn sleep(&self) {
        tokio::time::sleep(Duration::from_secs(MESH_MANAGER_GRACE_PERIOD_SECS)).await;
    }
}

impl MeshMode {
    pub(crate) fn new_locked(
        root_cert: Vec<u8>,
        ca_command_sender: tokio::sync::mpsc::UnboundedSender<RootCertCommand>,
        tokio_runtime: Arc<tokio::runtime::Runtime>,
    ) -> MeshMode {
        let grace_period_sleeper = Arc::new(GracePeriodSleeperImpl {});

        MeshMode::Locked(LockedMesh {
            root_cert,
            ca_command_sender,
            tokio_runtime,
            grace_period_sleeper,
        })
    }

    pub(crate) fn new_unlocked(local_vm_uds_pubs: Vec<CoseKey>) -> anyhow::Result<MeshMode> {
        let local_vm_uds_pubs = local_vm_uds_pubs
            .into_iter()
            .map(CborSerializable::to_vec)
            .collect::<Result<Vec<Vec<u8>>, CoseError>>()?;

        Ok(MeshMode::Unlocked(UnlockedMesh { local_vm_uds_pubs }))
    }
}

impl LockedMesh {
    pub fn get_uds_pub(
        &self,
        ctx: RpcContext,
        _req: GetUdsPubRequest,
        sink: UnarySink<GetUdsPubResponse>,
    ) {
        warn!("GetUdsPub called on a locked mesh");
        ctx.spawn(
            sink.fail(RpcStatus::new(RpcStatusCode::FAILED_PRECONDITION))
                .map_err(move |e| error!("failed to reply: {e:?}"))
                .map(|_| ()),
        );
    }

    pub fn exchange_root_certs(
        &self,
        ctx: &RpcContext,
        request: ExchangeRootCertsRequest,
    ) -> anyhow::Result<ExchangeRootCertsResponse, RpcStatus> {
        let ExchangeRootCertsRequest { root_cert, special_fields: _ } = request;

        let vm_name = extract_vm_name_from_root_cert(root_cert.as_slice())?;

        let auth_ctx = match ctx.auth_context() {
            Some(auth_ctx) => auth_ctx,
            None => {
                error!("No auth context available");
                return Err(RpcStatus::new(RpcStatusCode::UNAUTHENTICATED));
            }
        };

        // Extra check, given that it's not possible for a client to connect to the gRPC server in
        // a locked-mesh configuration without authentication. Connections must be mutually
        // authenticated TLS in a locked mesh.
        if !auth_ctx.peer_is_authenticated() {
            error!("Peer is not authenticated");
            return Err(RpcStatus::new(RpcStatusCode::UNAUTHENTICATED));
        }

        // The name of the SDV VM instance where the peer service discovery agent is running,
        // which is the gRPC client that called this method.
        // That name is stated in the gRPC client's certificate subject used in the DICE-based
        // TLS authentication. See crate::mesh::credentials_checker::CredentialsChecker.
        let peer_vm_name = get_vm_name_from_auth_ctx(&auth_ctx)?;
        if peer_vm_name != vm_name {
            error!(
                "VM name mismatch: root cert is for '{vm_name}', but TLS identity is for \
                '{peer_vm_name}'"
            );
            return Err(RpcStatus::new(RpcStatusCode::INVALID_ARGUMENT));
        }

        if let Err(err) = self.ca_command_sender.send(RootCertCommand::Add {
            vm_name: vm_name.to_string(),
            root_cert: root_cert.clone(),
        }) {
            error!("Failed to send root cert through channel: {err}");
        }

        // Since we're not tracking client disconnection, we remove the received remote root cert
        // after some arbitrary time.
        // Before that happens, we expect that our mesh::manager gRPC client has been able to
        // connect to the mesh::service gRPC server running on the remote peer VM, which will
        // increase the counter for that root cert. Then our remove command will bring the counter
        // back to 1 instead of causing the root cert to be really removed from our CA service.
        //
        // If our gRPC client (mesh::manager) never manages to connect to the peer gRPC server (the
        // peer's own mesh::service), our CA service would keep the peer's root cert forever. Which
        // is why we need to decrease the "add count" of that root cert in the CA service after
        // some time.
        //
        // This situation can happen if the peer connects to our service and calls
        // exchange_root_certs() but then goes offline for whatever reason. Or the peer's
        // mesh::service is not accepting connections, or whatever.
        //
        // Or maybe when it goes back online, we refuse connecting to it as it no longer passes our
        // DICE-based authentication. And that now-untrusted VM could still be carrying the private
        // key of that root cert that it gave us from back when it was trusted.
        //
        // Thus it's better to ensure that we are not keeping around a stale root cert, as it could
        // lead to a security problem.
        //
        // TODO: b/409967811 - If we had only a single connection to synchronize data between two
        // SD agents (instead of the current two) we wouldn't need to do this.
        self.tokio_runtime.spawn(remove_vm_root_cert(
            vm_name.to_string(),
            root_cert,
            self.grace_period_sleeper.clone(),
            self.ca_command_sender.clone(),
        ));

        Ok(ExchangeRootCertsResponse { root_cert: self.root_cert.clone(), ..Default::default() })
    }
}

async fn remove_vm_root_cert(
    vm_name: String,
    root_cert: Vec<u8>,
    grace_period_sleeper: Arc<dyn GracePeriodSleeper>,
    ca_command_sender: tokio::sync::mpsc::UnboundedSender<RootCertCommand>,
) {
    grace_period_sleeper.sleep().await;
    if let Err(err) = ca_command_sender.send(RootCertCommand::Remove { vm_name, root_cert }) {
        error!("Failed to send remove root cert command through channel: {err}");
    }
}

impl UnlockedMesh {
    pub fn get_uds_pub(
        &self,
        ctx: RpcContext,
        _req: GetUdsPubRequest,
        sink: UnarySink<GetUdsPubResponse>,
    ) {
        let response =
            GetUdsPubResponse { uds_pub: self.local_vm_uds_pubs.clone(), ..Default::default() };
        ctx.spawn(
            sink.success(response).map_err(move |e| error!("failed to reply: {e:?}")).map(|_| ()),
        );
    }
    fn exchange_root_certs(
        &self,
        _ctx: &RpcContext,
        _request: ExchangeRootCertsRequest,
    ) -> anyhow::Result<ExchangeRootCertsResponse, RpcStatus> {
        warn!("ExchangeRootCerts called on an unlocked mesh");
        Err(RpcStatus::new(RpcStatusCode::FAILED_PRECONDITION))
    }
}

#[automock]
pub(crate) trait Server: Send {}

#[automock]
pub(crate) trait ServerFactory: Send {
    /// Starts a server instance using the provided environment and secure credentials.
    fn start_secure(
        &self,
        env: Arc<Environment>,
        credentials_checker: &CredentialsChecker,
        sd_cert: &X509,
        sd_private_key: &PKey<Private>,
    ) -> anyhow::Result<(Box<dyn Server>, u16)>;

    /// Starts a server instance using the provided environment and insecure credentials.
    fn start_unsecure(&self, env: Arc<Environment>) -> anyhow::Result<(Box<dyn Server>, u16)>;
}

struct ServerFactoryImpl {
    local_vm_acl_policy_provider: Arc<dyn SingleVmAclPolicyProvider>,
    service_registry: Arc<ServiceRegistry>,
    vvm_config: Arc<VvmConfig>,
    mesh_mode: MeshMode,
    new_client_notifier: broadcast::Sender<()>,
}

impl ServerFactory for ServerFactoryImpl {
    /// Starts a server instance using the provided environment and secure credentials.
    fn start_secure(
        &self,
        env: Arc<Environment>,
        credentials_checker: &CredentialsChecker,
        sd_cert: &X509,
        sd_private_key: &PKey<Private>,
    ) -> anyhow::Result<(Box<dyn Server>, u16)> {
        info!("Starting DICE secure mesh server");
        let server_credentials =
            make_server_credentials(credentials_checker.clone(), sd_cert, sd_private_key)?;
        let (server, port) = self.start(env, server_credentials)?;

        Ok((Box::new(server), port))
    }

    /// Starts a server instance using the provided environment and insecure credentials.
    fn start_unsecure(&self, env: Arc<Environment>) -> anyhow::Result<(Box<dyn Server>, u16)> {
        info!("Starting unsecure mesh server");
        let (server, port) = self.start(env, ServerCredentials::insecure())?;

        Ok((Box::new(server), port))
    }
}

impl ServerFactoryImpl {
    /// Starts a grpcio server instance with the given environment and credentials.
    fn start(
        &self,
        env: Arc<Environment>,
        server_credentials: ServerCredentials,
    ) -> anyhow::Result<(grpcio::Server, u16)> {
        let socket_addr_str = self.vvm_config.own_cfg.addr.to_string();

        let mesh_manager_service = MeshManagerService::new(
            self.local_vm_acl_policy_provider.clone(),
            self.service_registry.clone(),
            self.vvm_config.clone(),
            self.mesh_mode.clone(),
            self.new_client_notifier.clone(),
        );

        let grpc_service = create_mesh_manager(mesh_manager_service);

        let ch_builder = ChannelBuilder::new(env.clone());

        let mut grpc_server = grpcio::ServerBuilder::new(env.clone())
            .register_service(grpc_service)
            .channel_args(ch_builder.build_args())
            .build()?;

        let bound_port =
            grpc_server.add_listening_port(socket_addr_str.clone(), server_credentials)?;
        grpc_server.start();

        info!("Mesh server listening on {socket_addr_str}");
        Ok((grpc_server, bound_port))
    }
}

#[derive(Clone)]
struct MeshManagerService {
    local_vm_acl_policy_provider: Arc<dyn SingleVmAclPolicyProvider>,
    service_registry: Arc<ServiceRegistry>,
    vvm_config: Arc<VvmConfig>,
    mesh_mode: MeshMode,
    new_client_notifier: broadcast::Sender<()>,
}

impl MeshManagerService {
    fn new(
        local_vm_acl_policy_provider: Arc<dyn SingleVmAclPolicyProvider>,
        service_registry: Arc<ServiceRegistry>,
        vvm_config: Arc<VvmConfig>,
        mesh_mode: MeshMode,
        new_client_notifier: broadcast::Sender<()>,
    ) -> Self {
        MeshManagerService {
            local_vm_acl_policy_provider,
            service_registry,
            vvm_config,
            mesh_mode,
            new_client_notifier,
        }
    }

    fn notify_new_client(&mut self) {
        let _ = self.new_client_notifier.send(());
    }
}

fn event_into_proto(event: ServiceUnitEvent) -> PeerLocalServiceUnitsEvent {
    let event = match event {
        ServiceUnitEvent::RegisterServiceUnit(service_unit_and_metadata) => {
            AddServiceUnit(AddServiceUnitEvent {
                unit_definition: MessageField(Some(Box::new(
                    service_unit_and_metadata.get_service_unit_definition().to_proto(),
                ))),
                transport_metadata: MessageField(Some(Box::new(
                    service_unit_and_metadata.get_transport_metadata().to_proto(),
                ))),
                ..Default::default()
            })
        }
        ServiceUnitEvent::UnregisterServiceUnit(service_unit_identifier) => {
            RemoveServiceUnit(RemoveServiceUnitEvent {
                service_unit_identifier: MessageField(Some(Box::new(ServiceUnitIdentifier {
                    sid: service_unit_identifier.sid.into(),
                    service_unit_name: service_unit_identifier.name,
                    ..Default::default()
                }))),
                ..Default::default()
            })
        }
        ServiceUnitEvent::UnregisterByIdentity(fqin) => RemoveByIdentity(RemoveByIdentityEvent {
            service_fqin: MessageField(Some(Box::new(fqin.to_proto()))),
            ..Default::default()
        }),
    };

    PeerLocalServiceUnitsEvent { event: Some(event), ..Default::default() }
}

impl MeshManager for MeshManagerService {
    /// Service side implementation of the subscription to the VM unit type changes.
    /// This accesses the `service_registry` storage and atomically receives the current state and the stream of the changes.
    /// This allows us to combine it to a single stream and send over gRPC.
    fn subscribe_peer_local_service_units(
        &mut self,
        ctx: RpcContext,
        _req: SubscribePeerLocalServiceUnitsRequest,
        mut event_sink: ServerStreamingSink<PeerLocalServiceUnitsEvent>,
    ) {
        // A new remote VM has just started and is attempting to subscribe to changes in the local
        // VM's service unit availabilities.
        // Notify MeshManager to ensure it runs its next loop iteration for trying to connect to
        // new clients ASAP.
        self.notify_new_client();

        let (local_service_units, update_receiver) =
            self.service_registry.get_local_service_units_and_subscribe_for_changes(
                self.vvm_config.own_cfg.name.as_ref(),
            );

        let available_units = local_service_units
            .into_iter()
            .map(|service_unit_and_metadata| {
                ServiceUnitEvent::RegisterServiceUnit(service_unit_and_metadata)
            })
            .map(event_into_proto)
            .map(|e| Ok((e, WriteFlags::default())));

        let mut unit_updates = UnboundedReceiverStream::new(update_receiver)
            .map(event_into_proto)
            .map(|proto| Ok((proto, WriteFlags::default())));

        ctx.spawn(async move {
            // async block allows to borrow temporary input stream object
            if let Err(e) = event_sink.send_all(&mut stream::iter(available_units)).await {
                error!("failed to send available units: {e:?}");
                // event_sink has resolved into a failed state, so it can no longer be used.
                return;
            }

            if let Err(e) = event_sink.send_all(&mut unit_updates).await {
                error!("failed to send unit updates: {e:?}");
            }
        });
    }

    #[instrument(name = "authz_perf::mesh::rpc::get_peer_local_acls", skip_all)]
    fn get_peer_local_acls(
        &mut self,
        ctx: RpcContext,
        _req: GetPeerLocalAclsRequest,
        sink: UnarySink<GetPeerLocalAclsResponse>,
    ) {
        let result = match to_peer_local_acls_proto(self.local_vm_acl_policy_provider.as_ref()) {
            Ok(proto) => sink.success(proto),
            Err(err) => {
                error!("Failed to fetch local ACL policy to send over mesh: {err:?}");
                sink.fail(RpcStatus::new(RpcStatusCode::INTERNAL))
            }
        };

        ctx.spawn(result.map_err(move |e| error!("failed to reply: {e:?}")).map(|_| ()));
    }

    fn ping(&mut self, ctx: RpcContext, _req: PingRequest, sink: UnarySink<PingResponse>) {
        ctx.spawn(
            sink.success(PingResponse::default())
                .map_err(move |e| error!("failed to reply: {e:?}"))
                .map(|_| ()),
        );
    }

    fn get_uds_pub(
        &mut self,
        ctx: RpcContext,
        req: GetUdsPubRequest,
        sink: UnarySink<GetUdsPubResponse>,
    ) {
        match &self.mesh_mode {
            MeshMode::Locked(locked) => locked.get_uds_pub(ctx, req, sink),
            MeshMode::Unlocked(unlocked) => unlocked.get_uds_pub(ctx, req, sink),
        }
    }

    fn exchange_root_certs(
        &mut self,
        ctx: RpcContext,
        req: ExchangeRootCertsRequest,
        sink: UnarySink<ExchangeRootCertsResponse>,
    ) {
        let result = match &self.mesh_mode {
            MeshMode::Locked(locked) => locked.exchange_root_certs(&ctx, req),
            MeshMode::Unlocked(unlocked) => unlocked.exchange_root_certs(&ctx, req),
        };

        match result {
            Ok(response) => {
                ctx.spawn(
                    sink.success(response)
                        .map_err(move |e| error!("failed to reply: {e:?}"))
                        .map(|_| ()),
                );
            }
            Err(rpc_status) => {
                ctx.spawn(
                    sink.fail(rpc_status)
                        .map_err(move |e| error!("failed to reply: {e:?}"))
                        .map(|_| ()),
                );
            }
        }
    }
}

fn get_root_cert_vm_name(root_cert_pem: &[u8]) -> anyhow::Result<String> {
    let root_cert = X509::from_pem(root_cert_pem)?;
    base32_x509_common_name_to_string(root_cert.subject_name())
}

fn extract_vm_name_from_root_cert(root_cert: &[u8]) -> Result<VmName, RpcStatus> {
    let vm_name_str = match get_root_cert_vm_name(root_cert) {
        Ok(vm_name) => vm_name,
        Err(error) => {
            error!("Failed to get VM name from the received PEM-encoded root cert: {error}");
            return Err(RpcStatus::new(RpcStatusCode::INVALID_ARGUMENT));
        }
    };

    match VmName::from_str(&vm_name_str) {
        Ok(name) => Ok(name),
        Err(error) => {
            error!("Received VM name '{vm_name_str}' is not valid: {error}");
            Err(RpcStatus::new(RpcStatusCode::INVALID_ARGUMENT))
        }
    }
}

fn get_vm_name_from_x509_pem(pem_cert: &str) -> Result<VmName, RpcStatus> {
    match X509::from_pem(pem_cert.as_bytes()) {
        Ok(cert) => match SdCertificateClaims::from_x509(&cert) {
            Ok(claims) => Ok(claims.vm_name),
            Err(e) => {
                error!("Failed to parse SD claims from peer certificate: {e}");
                Err(RpcStatus::new(RpcStatusCode::UNAUTHENTICATED))
            }
        },
        Err(e) => {
            error!("Failed to parse peer certificate from PEM: {e}");
            Err(RpcStatus::new(RpcStatusCode::UNAUTHENTICATED))
        }
    }
}

/// Extracts the VM name from the peer's certificate stored in the gRPC authentication context.
fn get_vm_name_from_auth_ctx(auth_ctx: &AuthContext) -> Result<VmName, RpcStatus> {
    // TSI_X509_PEM_CERT_PROPERTY from ssl_transport_security.h
    const X509_PEM_CERT_PROPERTY: &str = "x509_pem_cert";

    // The underlying grpc_auth_context_property_iterator() function can return multiple properties
    // with the same name, such as TSI_X509_SUBJECT_ALTERNATIVE_NAME_PEER_PROPERTY.  But
    // TSI_X509_PEM_CERT_PROPERTY is added only once and it does not make sense to have multiple of
    // those, given that there's only one leaf certificate.
    for prop in auth_ctx.into_iter() {
        if prop.name() == X509_PEM_CERT_PROPERTY {
            if let Ok(pem_cert) = prop.value_str() {
                return get_vm_name_from_x509_pem(pem_cert);
            }
        }
    }
    error!("Could not find valid {X509_PEM_CERT_PROPERTY} property in auth context");
    Err(RpcStatus::new(RpcStatusCode::UNAUTHENTICATED))
}

pub struct LockedMeshServiceControllerBuilder {
    pub service_registry: Arc<ServiceRegistry>,
    pub vvm_config: Arc<VvmConfig>,
    pub root_cert_pem: Vec<u8>,
    pub ca_command_sender: tokio::sync::mpsc::UnboundedSender<RootCertCommand>,
    pub tokio_runtime: Arc<tokio::runtime::Runtime>,
    pub mesh_manager_wake_notifier: broadcast::Sender<()>,
    pub env: Arc<Environment>,
    pub sd_cert: X509,
    pub sd_priv: PKey<Private>,
    pub credentials_checker: CredentialsChecker,
    pub server_port_tx: watch::Sender<Option<u16>>,
}

impl LockedMeshServiceControllerBuilder {
    pub fn build(self) -> anyhow::Result<MeshServiceController> {
        let server_factory = Box::new(ServerFactoryImpl {
            local_vm_acl_policy_provider: create_local_vm_acl_policy_provider()?,
            service_registry: self.service_registry,
            vvm_config: self.vvm_config,
            mesh_mode: MeshMode::new_locked(
                self.root_cert_pem,
                self.ca_command_sender,
                self.tokio_runtime,
            ),
            new_client_notifier: self.mesh_manager_wake_notifier,
        });

        Ok(MeshServiceController {
            server_factory,
            env: self.env,
            server_port_tx: self.server_port_tx,
            retry_min_duration: START_SERVER_RETRY_MIN_DURATION,
            retry_max_duration: START_SERVER_RETRY_MAX_DURATION,
            mesh_mode: ControllerMeshMode::Locked {
                sd_cert: self.sd_cert,
                sd_priv: self.sd_priv,
                credentials_checker: self.credentials_checker,
            },
        })
    }
}

pub struct UnlockedMeshServiceControllerBuilder {
    pub service_registry: Arc<ServiceRegistry>,
    pub vvm_config: Arc<VvmConfig>,
    pub local_vm_uds_pubs: Vec<coset::CoseKey>,
    pub mesh_manager_wake_notifier: broadcast::Sender<()>,
    pub env: Arc<Environment>,
    pub server_port_tx: watch::Sender<Option<u16>>,
}

/// Specifies in which mode the mesh is and carries any extra data that MeshServiceController needs
/// in that particular mesh mode.
#[derive(Clone)]
pub(crate) enum ControllerMeshMode {
    /// Mesh is locked (secure). We use a CA with root certificates that are generated at runtime
    /// and exchanged over the secure mesh.
    Locked { sd_cert: X509, sd_priv: PKey<Private>, credentials_checker: CredentialsChecker },

    /// Mesh is unlocked (insecure). State used for provisioning of the uds_pubs file and for
    /// development.
    Unlocked,
}

/// Controls the lifecycle of the mesh gRPC service, starting and stopping it based on the
/// system's power state.
///
pub struct MeshServiceController {
    server_factory: Box<dyn ServerFactory>,
    env: Arc<Environment>,
    server_port_tx: watch::Sender<Option<u16>>,
    retry_min_duration: Duration,
    retry_max_duration: Duration,
    mesh_mode: ControllerMeshMode,
}

impl UnlockedMeshServiceControllerBuilder {
    pub fn build(self) -> anyhow::Result<MeshServiceController> {
        let server_factory = Box::new(ServerFactoryImpl {
            local_vm_acl_policy_provider: create_local_vm_acl_policy_provider()?,
            service_registry: self.service_registry,
            vvm_config: self.vvm_config,
            mesh_mode: MeshMode::new_unlocked(self.local_vm_uds_pubs)?,
            new_client_notifier: self.mesh_manager_wake_notifier,
        });

        Ok(MeshServiceController {
            server_factory,
            env: self.env,
            server_port_tx: self.server_port_tx,
            retry_min_duration: START_SERVER_RETRY_MIN_DURATION,
            retry_max_duration: START_SERVER_RETRY_MAX_DURATION,
            mesh_mode: ControllerMeshMode::Unlocked,
        })
    }
}

impl Server for grpcio::Server {}

enum RunState {
    // The state is used to just hold the ownership of the Service instance, not to access it.
    // Hence the need for allowing "dead code".
    #[allow(dead_code)]
    Started(Box<dyn Server>),
    Retrying(Duration),
    Stopped,
}

impl RunState {
    /// Maximum amount of time to wait for a power state change event.
    fn max_wait_time(&self) -> Duration {
        if let RunState::Retrying(duration) = self {
            *duration
        } else {
            // Wait indefinitely.
            Duration::MAX
        }
    }
}

impl MeshServiceController {
    pub async fn run(self, mut power_state_rx: broadcast::Receiver<SimplePowerState>) {
        let mut mesh_server_state = RunState::Stopped;

        // Initially we assume the power is on.
        let mut current_power_state = SimplePowerState::On;

        loop {
            // Start or stop the service according to the current power state.
            if current_power_state == SimplePowerState::On
                && !matches!(mesh_server_state, RunState::Started(_))
            {
                info!("Power state is On, starting mesh gRPC server.");
                mesh_server_state = self.try_start_server(&mesh_server_state);
            } else if current_power_state == SimplePowerState::Off
                && !matches!(mesh_server_state, RunState::Stopped)
            {
                info!("Power state is Off, stopping mesh gRPC server.");
                // Dropping the server instance will shut it down.
                mesh_server_state = RunState::Stopped;
                let _ = self.server_port_tx.send_replace(None);
            }

            // Wait for the next power state event or a timeout, which ever comes first.
            select! {
                res = power_state_rx.recv() => match res {
                    Ok(power_state) => {
                        info!("Received new power state: {power_state:?}");
                        current_power_state = power_state;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Power state receiver lagged by {n} messages. \
                            Some changes were missed.");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        error!("Power state channel closed. Stopping MeshService permanently.");
                        break;
                    }
                },
                _ = sleep(mesh_server_state.max_wait_time()) => (),
            };
        }
    }

    fn try_start_server(&self, current_state: &RunState) -> RunState {
        match self.start_server() {
            Ok((instance, port)) => {
                let _ = self.server_port_tx.send_replace(Some(port));
                RunState::Started(instance)
            }
            Err(err) => {
                let retry_duration = if let RunState::Retrying(duration) = current_state {
                    std::cmp::min(self.retry_max_duration, duration.mul(2))
                } else {
                    self.retry_min_duration
                };
                error!("Failed to start server: {err}, retry in {retry_duration:?}");
                RunState::Retrying(retry_duration)
            }
        }
    }

    fn start_server(&self) -> anyhow::Result<(Box<dyn Server>, u16)> {
        match &self.mesh_mode {
            ControllerMeshMode::Locked { sd_cert, sd_priv, credentials_checker } => self
                .server_factory
                .start_secure(self.env.clone(), credentials_checker, sd_cert, sd_priv),
            ControllerMeshMode::Unlocked => self.server_factory.start_unsecure(self.env.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::ServiceUnitIdentifier as KvServiceUnitIdentifier;
    use crate::mesh::async_client::{MeshConnector, SecureMeshConnector, UnsecureMeshConnector};
    use crate::mesh::manager::test_support::TestSingleVmAclPolicyProvider;
    use crate::mesh::AuthenticatedCsr;
    use crypto_rpc::cert::{build_ec_private_key, build_root_certificate};
    use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinV2;
    use grpcio::EnvBuilder;
    use sdv_comms_id::names::{PackageName, ServiceName};

    use std::borrow::BorrowMut;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use tokio::sync::Notify;

    use crate::ca::service::test_support::{
        receive_command_add_root_cert, receive_command_remove_root_cert,
    };

    use crate::mesh::peer_connection::test_support::wait_for;
    use crate::mesh::uds_pubs::test_support::{create_example_key1, create_example_key2};
    use crate::mesh::vvm_config::{create_test_uds_ca_pub, VmConfig};
    struct FakeGracePeriodSleeper {
        notify: Arc<Notify>,
    }

    #[async_trait]
    impl GracePeriodSleeper for FakeGracePeriodSleeper {
        async fn sleep(&self) {
            // Just wait until the test code tells us to move on.
            self.notify.notified().await;
        }
    }

    fn create_root_cert_pem_for_vm(vm_name: &str) -> anyhow::Result<Vec<u8>> {
        let root_key = PKey::from_ec_key(build_ec_private_key()?)?;
        let root_cert = build_root_certificate(&root_key, vm_name)?;
        Ok(root_cert.to_pem()?)
    }

    fn prepare_test_vm_config() -> anyhow::Result<VvmConfig> {
        Ok(VvmConfig {
            uds_ca_pub: create_test_uds_ca_pub()?,
            uds_ca_rev_list: vvmtruststore::CertificateRevocationList::default(),
            policies: vec![],
            remote_cfgs: vec![],
            // Let the OS pick up a free ephemeral port by passing port number 0.
            own_cfg: VmConfig::new_for_testing(
                "instance1".to_owned().try_into()?,
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0),
            ),
        })
    }

    /// Tests the MeshManager::ExchangeRootCerts() gRPC method, with the service configured in
    /// locked mesh mode.
    #[test]
    fn locked_service_exchange_root_certs() {
        use crate::mesh::credentials_checker::tests::{
            create_custom_credentials_checker, create_vvmconfig, SdClaimsBuilder,
        };
        use crypto_rpc::cert::build_ec_private_key;
        use sample_dice_handover::create_random_uds;

        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let (ca_command_sender, mut ca_command_receiver) = tokio::sync::mpsc::unbounded_channel();

        let grace_period_notifier = Arc::new(Notify::new());

        let grace_period_sleeper =
            Arc::new(FakeGracePeriodSleeper { notify: grace_period_notifier.clone() });

        let locked_mesh = LockedMesh {
            root_cert: vec![1, 2, 3],
            ca_command_sender: ca_command_sender.clone(),
            tokio_runtime: tokio_runtime.clone(),
            grace_period_sleeper,
        };

        let (new_client_notifier, _) = broadcast::channel(1);

        let mut vvm_config = create_vvmconfig().unwrap();
        // Let the OS pick up a free ephemeral port by passing port number 0.
        vvm_config.own_cfg.addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
        let vvm_config = Arc::new(vvm_config);

        let server_factory = ServerFactoryImpl {
            local_vm_acl_policy_provider: Arc::new(TestSingleVmAclPolicyProvider::new()),
            service_registry: Arc::new(ServiceRegistry::default()),
            vvm_config: vvm_config.clone(),
            mesh_mode: MeshMode::Locked(locked_mesh.clone()),
            new_client_notifier,
        };

        let env = Arc::new(EnvBuilder::new().build());

        let uds = create_random_uds();
        let sd_priv: PKey<Private> = build_ec_private_key().unwrap().try_into().unwrap();
        let auth_csr = AuthenticatedCsr::create().unwrap();
        let credentials_checker =
            create_custom_credentials_checker(&uds, (*vvm_config).clone(), &auth_csr.cbor).unwrap();

        let sd_cert: X509 = SdClaimsBuilder::new(&vvm_config.own_cfg.name, &uds, &auth_csr)
            .with_sd_private_key(&sd_priv)
            .build()
            .unwrap()
            .to_x509(&sd_priv)
            .unwrap();

        let (_server, bound_port) = server_factory
            .start_secure(env.clone(), &credentials_checker, &sd_cert, &sd_priv)
            .unwrap();

        let mut actual_server_addr = vvm_config.own_cfg.addr;
        actual_server_addr.set_port(bound_port);

        // Client's VmConfig, with roles swapped.
        let client_vvm_config = VvmConfig {
            own_cfg: vvm_config.remote_cfgs[0].clone(),
            remote_cfgs: vec![vvm_config.own_cfg.clone()],
            ..(*vvm_config).clone()
        };
        let instance2_name = client_vvm_config.own_cfg.name.to_string();
        let instance2_root_cert_pem = create_root_cert_pem_for_vm(&instance2_name).unwrap();

        // Client's identity
        let client_sd_priv: PKey<Private> = build_ec_private_key().unwrap().try_into().unwrap();
        let client_sd_cert: X509 =
            SdClaimsBuilder::new(&client_vvm_config.own_cfg.name, &uds, &auth_csr)
                .with_sd_private_key(&client_sd_priv)
                .build()
                .unwrap()
                .to_x509(&client_sd_priv)
                .unwrap();

        let client_credentials_checker =
            create_custom_credentials_checker(&uds, client_vvm_config.clone(), &auth_csr.cbor)
                .unwrap();

        let client_connector =
            SecureMeshConnector::new(client_credentials_checker, &client_sd_cert, &client_sd_priv)
                .unwrap();

        let own_vm_name = vvm_config.own_cfg.name.clone();
        let instance2_root_cert_pem_clone = instance2_root_cert_pem.clone();
        tokio_runtime.block_on(async move {
            let client = client_connector
                .try_connect(env.clone(), &actual_server_addr, own_vm_name.as_ref())
                .await
                .unwrap();
            let response =
                client.exchange_root_certs(&instance2_root_cert_pem_clone).await.unwrap();

            // We should send back our own root cert.
            assert_eq!(response, locked_mesh.root_cert,);
        });

        // We should forward the peer root cert we received to our CA.
        receive_command_add_root_cert(
            ca_command_receiver.borrow_mut(),
            &instance2_name,
            &instance2_root_cert_pem,
            &tokio_runtime,
        )
        .unwrap();
        // There should be no further commands.
        assert!(ca_command_receiver.is_empty());

        // Emulate that MESH_MANAGER_GRACE_PERIOD_SECS seconds have passed.
        grace_period_notifier.notify_one();

        // We expect that our own mesh::manager gRPC client was able to connect to the peer
        // and call exchange_root_certs() himself in the meantime. Which will increase the
        // root cert add count to 2.
        // We can thus now remove our root cert add count and let our mesh::manager gRPC client
        // track liveness of the connection between the two SD agents.
        receive_command_remove_root_cert(
            ca_command_receiver.borrow_mut(),
            &instance2_name,
            &tokio_runtime,
        )
        .unwrap();
        // There should be no further commands.
        assert!(ca_command_receiver.is_empty());
    }

    /// Tests that MeshManager::ExchangeRootCerts() returns INVALID_ARGUMENT if the VM name in
    /// the root certificate passed as a parameter does not match the VM name in the certificate
    /// the client used to establish the TLS connection in which that gRPC call was made.
    #[test]
    fn locked_service_exchange_root_certs_vm_name_mismatch_err() {
        use crate::mesh::credentials_checker::tests::{
            create_custom_credentials_checker, create_vvmconfig, SdClaimsBuilder,
        };
        use crypto_rpc::cert::build_ec_private_key;
        use sample_dice_handover::create_random_uds;

        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let (ca_command_sender, _ca_command_receiver) = tokio::sync::mpsc::unbounded_channel();

        let grace_period_notifier = Arc::new(Notify::new());

        let grace_period_sleeper =
            Arc::new(FakeGracePeriodSleeper { notify: grace_period_notifier.clone() });

        let locked_mesh = LockedMesh {
            root_cert: vec![1, 2, 3],
            ca_command_sender: ca_command_sender.clone(),
            tokio_runtime: tokio_runtime.clone(),
            grace_period_sleeper,
        };

        let (new_client_notifier, _) = broadcast::channel(1);

        let mut vvm_config = create_vvmconfig().unwrap();
        // Let the OS pick up a free ephemeral port by passing port number 0.
        vvm_config.own_cfg.addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
        let vvm_config = Arc::new(vvm_config);

        let server_factory = ServerFactoryImpl {
            local_vm_acl_policy_provider: Arc::new(TestSingleVmAclPolicyProvider::new()),
            service_registry: Arc::new(ServiceRegistry::default()),
            vvm_config: vvm_config.clone(),
            mesh_mode: MeshMode::Locked(locked_mesh.clone()),
            new_client_notifier,
        };

        let env = Arc::new(EnvBuilder::new().build());

        let uds = create_random_uds();
        let sd_priv: PKey<Private> = build_ec_private_key().unwrap().try_into().unwrap();
        let auth_csr = AuthenticatedCsr::create().unwrap();
        let credentials_checker =
            create_custom_credentials_checker(&uds, (*vvm_config).clone(), &auth_csr.cbor).unwrap();

        let sd_cert: X509 = SdClaimsBuilder::new(&vvm_config.own_cfg.name, &uds, &auth_csr)
            .with_sd_private_key(&sd_priv)
            .build()
            .unwrap()
            .to_x509(&sd_priv)
            .unwrap();

        let (_server, bound_port) = server_factory
            .start_secure(env.clone(), &credentials_checker, &sd_cert, &sd_priv)
            .unwrap();

        let mut actual_server_addr = vvm_config.own_cfg.addr;
        actual_server_addr.set_port(bound_port);

        // Client's VmConfig, with roles swapped.
        let client_vvm_config = VvmConfig {
            own_cfg: vvm_config.remote_cfgs[0].clone(),
            remote_cfgs: vec![vvm_config.own_cfg.clone()],
            ..(*vvm_config).clone()
        };

        // ROOT CERT FOR A DIFFERENT VM NAME
        let other_name = "other";
        let other_root_cert_pem = create_root_cert_pem_for_vm(other_name).unwrap();

        // Client's identity
        let client_sd_priv: PKey<Private> = build_ec_private_key().unwrap().try_into().unwrap();
        let client_sd_cert: X509 =
            SdClaimsBuilder::new(&client_vvm_config.own_cfg.name, &uds, &auth_csr)
                .with_sd_private_key(&client_sd_priv)
                .build()
                .unwrap()
                .to_x509(&client_sd_priv)
                .unwrap();

        let client_credentials_checker =
            create_custom_credentials_checker(&uds, client_vvm_config.clone(), &auth_csr.cbor)
                .unwrap();

        let client_connector =
            SecureMeshConnector::new(client_credentials_checker, &client_sd_cert, &client_sd_priv)
                .unwrap();

        let own_vm_name = vvm_config.own_cfg.name.clone();
        tokio_runtime.block_on(async move {
            let client = client_connector
                .try_connect(env.clone(), &actual_server_addr, own_vm_name.as_ref())
                .await
                .unwrap();
            // Note that here we are passing a root cert whose subject says "other" instead
            // of "foo", which is what client_vvm_config.own_cfg.name has.
            let result = client.exchange_root_certs(&other_root_cert_pem).await;

            result.expect_err("Passing a root cert with a mismatching subject name must fail.");
        });
    }

    /// Tests the MeshManager::Ping() gRPC method, with the service configured in locked
    /// mesh mode and using secure (TLS) connections.
    ///
    /// Note that the most interesting part of this test is checking that the SD certificate
    /// verification in the TLS handshake works end-to-end.
    #[test]
    fn locked_service_ping_over_tls() {
        use crate::mesh::credentials_checker::tests::{
            create_custom_credentials_checker, create_vvmconfig, SdClaimsBuilder,
        };

        use sample_dice_handover::create_random_uds;

        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let (ca_command_sender, _ca_command_receiver) = tokio::sync::mpsc::unbounded_channel();

        let mut vvm_config = create_vvmconfig().unwrap();
        // Let the OS pick up a free ephemeral port by passing port number 0.
        vvm_config.own_cfg.addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
        let vvm_config = Arc::new(vvm_config);

        let env = Arc::new(EnvBuilder::new().build());
        let (new_client_notifier, _) = broadcast::channel(1);

        let mesh_mode = MeshMode::new_locked(
            vec![1, 2, 3], // root_cert
            ca_command_sender,
            tokio_runtime.clone(),
        );

        let server_factory = ServerFactoryImpl {
            local_vm_acl_policy_provider: Arc::new(TestSingleVmAclPolicyProvider::new()),
            service_registry: Arc::new(ServiceRegistry::default()),
            vvm_config: vvm_config.clone(),
            mesh_mode,
            new_client_notifier,
        };

        // Server's identity ("bar")
        let uds = create_random_uds();
        let sd_priv: PKey<Private> = build_ec_private_key().unwrap().try_into().unwrap();
        let auth_csr = AuthenticatedCsr::create().unwrap();
        let credentials_checker =
            create_custom_credentials_checker(&uds, (*vvm_config).clone(), &auth_csr.cbor).unwrap();

        let sd_cert: X509 = SdClaimsBuilder::new(&vvm_config.own_cfg.name, &uds, &auth_csr)
            .with_sd_private_key(&sd_priv)
            .build()
            .unwrap()
            .to_x509(&sd_priv)
            .unwrap();

        let (_server, bound_port) = server_factory
            .start_secure(env.clone(), &credentials_checker, &sd_cert, &sd_priv)
            .unwrap();

        let mut actual_server_addr = vvm_config.own_cfg.addr;
        actual_server_addr.set_port(bound_port);

        // Client's VmConfig, with roles swapped.
        let client_vvm_config = VvmConfig {
            own_cfg: vvm_config.remote_cfgs[0].clone(),
            remote_cfgs: vec![vvm_config.own_cfg.clone()],
            ..(*vvm_config).clone()
        };

        // Client's identity ("foo")
        let client_sd_priv: PKey<Private> = build_ec_private_key().unwrap().try_into().unwrap();
        // Use the same uds and auth_csr as the server, for simplicity.
        let client_sd_cert: X509 =
            SdClaimsBuilder::new(&client_vvm_config.own_cfg.name, &uds, &auth_csr)
                .with_sd_private_key(&client_sd_priv)
                .build()
                .unwrap()
                .to_x509(&client_sd_priv)
                .unwrap();

        // Client's credentials checker, to verify server's ("bar") identity.
        let client_credentials_checker =
            create_custom_credentials_checker(&uds, client_vvm_config.clone(), &auth_csr.cbor)
                .unwrap();

        let client_connector =
            SecureMeshConnector::new(client_credentials_checker, &client_sd_cert, &client_sd_priv)
                .unwrap();

        tokio_runtime.block_on(async move {
            let client = client_connector
                .try_connect(env, &actual_server_addr, vvm_config.own_cfg.name.as_ref())
                .await
                .unwrap();

            client.ping(&PingRequest::default()).await.unwrap();
        });
    }

    /// Tests the MeshManager::GetUdsPub() gRPC method, with the service configured in unlocked
    /// mesh mode.
    #[tokio::test]
    async fn unlocked_service_get_uds_pub() {
        let vvm_config = Arc::new(prepare_test_vm_config().unwrap());

        let local_uds_keys = vec![create_example_key1(), create_example_key2()];
        let mesh_mode = MeshMode::new_unlocked(local_uds_keys.clone()).unwrap();

        let (new_client_notifier, _) = broadcast::channel(1);

        let server_factory = ServerFactoryImpl {
            local_vm_acl_policy_provider: Arc::new(TestSingleVmAclPolicyProvider::new()),
            service_registry: Arc::new(ServiceRegistry::default()),
            vvm_config: vvm_config.clone(),
            mesh_mode,
            new_client_notifier,
        };

        let env = Arc::new(EnvBuilder::new().build());
        let (_server, bound_port) =
            server_factory.start(env.clone(), ServerCredentials::insecure()).unwrap();

        let mut actual_server_addr = vvm_config.own_cfg.addr;
        actual_server_addr.set_port(bound_port);

        let mesh_connector = UnsecureMeshConnector {};
        let own_vm_name = vvm_config.own_cfg.name.clone();
        let client = mesh_connector
            .try_connect(env, &actual_server_addr, own_vm_name.as_ref())
            .await
            .unwrap();

        let mut expected_uds_pubs_cbor: Vec<Vec<u8>> =
            local_uds_keys.iter().map(|k| k.clone().to_vec().unwrap()).collect();

        let mut received_uds_pubs = client.get_uds_pub().await.unwrap();

        // Sort to compare since order is not guaranteed.
        expected_uds_pubs_cbor.sort();
        received_uds_pubs.sort();

        assert_eq!(received_uds_pubs, expected_uds_pubs_cbor);
    }

    /// Tests the MeshManager::GetUdsPub() gRPC method, with the service configured in locked
    /// mesh mode.
    /// The gRPC client should receive an error.
    #[test]
    fn locked_service_get_uds_pub_err() {
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let (ca_command_sender, _ca_command_receiver) = tokio::sync::mpsc::unbounded_channel();

        let grace_period_sleeper =
            Arc::new(FakeGracePeriodSleeper { notify: Arc::new(Notify::new()) });

        let locked_mesh = LockedMesh {
            root_cert: vec![1, 2, 3],
            ca_command_sender,
            tokio_runtime: tokio_runtime.clone(),
            grace_period_sleeper,
        };

        let (new_client_notifier, _) = broadcast::channel(1);

        let vvm_config = Arc::new(prepare_test_vm_config().unwrap());

        let server_factory = ServerFactoryImpl {
            local_vm_acl_policy_provider: Arc::new(TestSingleVmAclPolicyProvider::new()),
            service_registry: Arc::new(ServiceRegistry::default()),
            vvm_config: vvm_config.clone(),
            mesh_mode: MeshMode::Locked(locked_mesh),
            new_client_notifier,
        };

        let env = Arc::new(EnvBuilder::new().build());
        let (_server, bound_port) =
            server_factory.start(env.clone(), ServerCredentials::insecure()).unwrap();

        let mut actual_server_addr = vvm_config.own_cfg.addr;
        actual_server_addr.set_port(bound_port);

        let mesh_connector = UnsecureMeshConnector {};
        let own_vm_name = vvm_config.own_cfg.name.clone();
        tokio_runtime.block_on(async move {
            let client = mesh_connector
                .try_connect(env.clone(), &actual_server_addr, own_vm_name.as_ref())
                .await
                .unwrap();

            client.get_uds_pub().await.expect_err("Calling GetUdsPub() on a locked mesh must fail");
        });
    }

    /// Tests the MeshManager::SubscribePeerLocalServiceUnits() gRPC method, with the service
    /// configured in locked mesh mode.
    #[test]
    fn locked_service_subscribe_peer_local_service_units() {
        use crate::kv::test_support::{get_default_test_unit_type, register_service_unit_for_test};
        use crate::mesh::proto_to_aidl::TryToAidl;
        use google_sdv_identity::aidl::google::sdv::identity::ProcessIdentifier::ProcessIdentifier;
        use sdv_sd_mesh_api::sdv_sd_mesh::peer_local_service_units_event;

        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let (new_client_notifier, _) = broadcast::channel(1);

        let vvm_config = Arc::new(prepare_test_vm_config().unwrap());

        let service_registry = Arc::new(ServiceRegistry::default());

        // Add a unit to the service registry. The gRPC server should return this unit
        // in the subscribe events list.
        let caller_pid = ProcessIdentifier { pid: 42, uid: 1001 };
        let registered_service_identity = register_service_unit_for_test(
            &service_registry,
            vvm_config.own_cfg.name.as_ref(),
            &get_default_test_unit_type(),
            &caller_pid,
            123,
        );

        let server_factory = ServerFactoryImpl {
            local_vm_acl_policy_provider: Arc::new(TestSingleVmAclPolicyProvider::new()),
            service_registry: service_registry.clone(),
            vvm_config: vvm_config.clone(),
            mesh_mode: MeshMode::new_locked(
                vec![1, 2, 3],                            // root_cert
                tokio::sync::mpsc::unbounded_channel().0, // ca_command_sender
                tokio_runtime.clone(),
            ),
            new_client_notifier,
        };

        let env = Arc::new(EnvBuilder::new().build());
        let (_server, bound_port) =
            server_factory.start(env.clone(), ServerCredentials::insecure()).unwrap();

        let mut actual_server_addr = vvm_config.own_cfg.addr;
        actual_server_addr.set_port(bound_port);

        let mesh_connector = UnsecureMeshConnector {};
        let own_vm_name = vvm_config.own_cfg.name.clone();
        tokio_runtime.block_on(async move {
            let client = mesh_connector
                .try_connect(env.clone(), &actual_server_addr, own_vm_name.as_ref())
                .await
                .unwrap();

            let mut response_stream = client
                .subscribe_peer_local_service_units(
                    &SubscribePeerLocalServiceUnitsRequest::default(),
                )
                .unwrap();

            // In this cascade of expect()s we're peeling the following struct:
            // Result<Option<Result<PeerLocalServiceUnitsEvent, Error>>, Elapsed>
            // Where PeerLocalServiceUnitsEvent contains an `event` member
            // defined as Option<peer_local_service_units_event::Event>
            let first_event: peer_local_service_units_event::Event =
                tokio::time::timeout(Duration::from_secs(3), response_stream.next())
                    .await
                    .expect("Stream should not have timed out")
                    .expect("response stream should have a first event")
                    .expect("first event should not be a gRPC failure")
                    .event
                    .expect("PeerLocalServiceUnitsEvent should contain an event");

            // Check that first_event is an AddServiceUnit with the service identity
            // that we added to ServiceRegistry.
            match first_event {
                AddServiceUnit(AddServiceUnitEvent { unit_definition, .. }) => {
                    let received_service_identity = unit_definition.try_to_aidl().unwrap().identity;
                    assert_eq!(received_service_identity, registered_service_identity);
                }
                _ => panic!("First event should be AddServiceUnit"),
            }
        });
    }

    /// Tests the MeshManager::GetPeerLocalAcls() gRPC method, with the service configured in
    /// unlocked mesh mode.
    #[tokio::test]
    async fn unlocked_service_get_peer_local_acls() {
        use protobuf::text_format;
        use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
        use std::str::FromStr;

        let vvm_config = Arc::new(prepare_test_vm_config().unwrap());

        let local_uds_keys = vec![create_example_key1(), create_example_key2()];
        let mesh_mode = MeshMode::new_unlocked(local_uds_keys.clone()).unwrap();

        let (new_client_notifier, _) = broadcast::channel(1);

        let mut acl_policy_provider = TestSingleVmAclPolicyProvider::new();
        let package_name = PackageName::from_str("com.google.test").unwrap();
        let service_name = ServiceName::from_str("TestService").unwrap();
        let acl_policy = text_format::parse_from_str::<AclPolicyProto>(
            r#"
                acls {
                    name: "Acl"
                    include: "*"
                }
                service_units {
                    name: "test-service-unit"
                    acl_names: "Acl"
                }
            "#,
        )
        .unwrap();
        acl_policy_provider.add_policy(package_name.clone(), service_name.clone(), acl_policy);
        let acl_policy_provider = Arc::new(acl_policy_provider);

        let server_factory = ServerFactoryImpl {
            local_vm_acl_policy_provider: acl_policy_provider.clone(),
            service_registry: Arc::new(ServiceRegistry::default()),
            vvm_config: vvm_config.clone(),
            mesh_mode,
            new_client_notifier,
        };

        let env = Arc::new(EnvBuilder::new().build());
        let (_server, bound_port) =
            server_factory.start(env.clone(), ServerCredentials::insecure()).unwrap();

        let mut actual_server_addr = vvm_config.own_cfg.addr;
        actual_server_addr.set_port(bound_port);

        let mesh_connector = UnsecureMeshConnector {};
        let own_vm_name = vvm_config.own_cfg.name.clone();
        let client = mesh_connector
            .try_connect(env, &actual_server_addr, own_vm_name.as_ref())
            .await
            .unwrap();

        let response =
            client.get_peer_local_acls(&GetPeerLocalAclsRequest::default()).await.unwrap();

        let expected_response = to_peer_local_acls_proto(acl_policy_provider.as_ref()).unwrap();

        assert_eq!(response, expected_response);
    }

    #[test]
    fn event_into_proto_unregister_service_unit() {
        // Prepare the function's input
        let kv_suid = KvServiceUnitIdentifier {
            sid: crate::kv::Sid::new_for_testing(42),
            name: "test-unit".to_string(),
        };
        let event = ServiceUnitEvent::UnregisterServiceUnit(kv_suid);

        // Call the function under test
        let proto_event = event_into_proto(event);

        // Verify the function's output
        let expected_proto_suid = ServiceUnitIdentifier {
            sid: 42,
            service_unit_name: "test-unit".to_string(),
            ..Default::default()
        };
        let expected_event_inner = RemoveServiceUnit(RemoveServiceUnitEvent {
            service_unit_identifier: MessageField::some(expected_proto_suid),
            ..Default::default()
        });
        assert_eq!(proto_event.event, Some(expected_event_inner));
    }

    #[test]
    fn event_into_proto_unregister_by_identity() {
        // Prepare the function's input
        let fqin_aidl = ServiceFqinV2 {
            sdvVmName: "vm-name".to_string(),
            sdvPackageName: "com.google.pkg".to_string(),
            serviceBundleName: "MyService".to_string(),
            serviceInstanceName: "default".to_string(),
        };
        let event = ServiceUnitEvent::UnregisterByIdentity(fqin_aidl.clone());

        // Call the function under test
        let proto_event = event_into_proto(event);

        // Verify the function's output
        let expected_event_inner = RemoveByIdentity(RemoveByIdentityEvent {
            service_fqin: MessageField::some(fqin_aidl.to_proto()),
            ..Default::default()
        });
        assert_eq!(proto_event.event, Some(expected_event_inner));
    }

    /// Tests that if a client disconnects while subscribe_peer_local_service_units() is sending
    /// its list of available service units (ServiceUnitEvent::RegisterServiceUnit), the server
    /// does not panic. This is a regression test for b/435609793.
    #[test]
    fn subscribe_and_disconnect_does_not_panic() {
        use crate::kv::test_support::{get_default_test_unit_type, register_service_unit_for_test};
        use google_sdv_identity::aidl::google::sdv::identity::ProcessIdentifier::ProcessIdentifier;
        use std::time::Duration;

        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let (new_client_notifier, _) = broadcast::channel(1);

        let vvm_config = Arc::new(prepare_test_vm_config().unwrap());

        let service_registry = Arc::new(ServiceRegistry::default());

        // Add a long list of units to the service registry. The gRPC server should return these
        // units in the subscribe events list.
        for i in 0..500 {
            let caller_pid = ProcessIdentifier { pid: 42 + i as i64, uid: 1001 };
            let mut unit_type = get_default_test_unit_type();
            unit_type.sdvPackageName = format!("{}{}", unit_type.sdvPackageName, i);
            register_service_unit_for_test(
                &service_registry,
                vvm_config.own_cfg.name.as_ref(),
                &unit_type,
                &caller_pid,
                123 + i as i64,
            );
        }

        let server_factory = ServerFactoryImpl {
            local_vm_acl_policy_provider: Arc::new(TestSingleVmAclPolicyProvider::new()),
            service_registry,
            vvm_config: vvm_config.clone(),
            mesh_mode: MeshMode::new_locked(
                vec![1, 2, 3],                            // root_cert
                tokio::sync::mpsc::unbounded_channel().0, // ca_command_sender
                tokio_runtime.clone(),
            ),
            new_client_notifier,
        };

        let env = Arc::new(EnvBuilder::new().build());
        let (_server, bound_port) =
            server_factory.start(env.clone(), ServerCredentials::insecure()).unwrap();

        let mut actual_server_addr = vvm_config.own_cfg.addr;
        actual_server_addr.set_port(bound_port);

        let own_vm_name = vvm_config.own_cfg.name.clone();
        tokio_runtime.block_on(async move {
            for _i in 0..10 {
                let mesh_connector = UnsecureMeshConnector {};
                let client = mesh_connector
                    .try_connect(env.clone(), &actual_server_addr, own_vm_name.as_ref())
                    .await
                    .unwrap();

                let _response_stream =
                    client.subscribe_peer_local_service_units(&Default::default()).unwrap();

                // Close the connection immediately by leaving the scope of _response_stream and
                // client.
            }
            // Give the server some time to process the disconnections and potentially panic.
            // If it panics, the test will fail.
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
    }

    /// Tests that the MeshServiceController starts and stops the server based on power state
    /// changes, in locked mode.
    #[test]
    fn locked_mesh_service_controller_power_state() {
        use crate::mesh::async_client::SecureMeshConnector;
        use crate::mesh::credentials_checker::tests::{create_vvmconfig, SdClaimsBuilder};
        use sample_dice_handover::create_random_uds;

        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let (ca_command_sender, _ca_command_receiver) = tokio::sync::mpsc::unbounded_channel();

        let mut vvm_config = create_vvmconfig().unwrap();
        // Let the OS pick up a free ephemeral port by passing port number 0.
        vvm_config.own_cfg.addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
        let vvm_config = Arc::new(vvm_config);

        let env = Arc::new(EnvBuilder::new().build());
        let (new_client_notifier, _) = broadcast::channel(1);

        let mesh_mode = MeshMode::new_locked(
            vec![1, 2, 3], // root_cert
            ca_command_sender,
            tokio_runtime.clone(),
        );

        let server_factory = Box::new(ServerFactoryImpl {
            local_vm_acl_policy_provider: Arc::new(TestSingleVmAclPolicyProvider::new()),
            service_registry: Arc::new(ServiceRegistry::default()),
            vvm_config: vvm_config.clone(),
            mesh_mode,
            new_client_notifier,
        });

        // Server's identity ("bar")
        let uds = create_random_uds();
        let sd_priv: PKey<Private> = build_ec_private_key().unwrap().try_into().unwrap();
        let auth_csr = AuthenticatedCsr::create().unwrap();

        let sd_cert: X509 = SdClaimsBuilder::new(&vvm_config.own_cfg.name, &uds, &auth_csr)
            .with_sd_private_key(&sd_priv)
            .build()
            .unwrap()
            .to_x509(&sd_priv)
            .unwrap();

        let (server_port_tx, mut server_port_rx) = watch::channel(None);

        let controller = {
            let credentials_checker =
                crate::mesh::credentials_checker::tests::create_custom_credentials_checker(
                    &uds,
                    (*vvm_config).clone(),
                    &auth_csr.cbor,
                )
                .unwrap();

            MeshServiceController {
                server_factory,
                env: env.clone(),
                server_port_tx,
                retry_min_duration: Duration::from_millis(10),
                retry_max_duration: Duration::from_millis(160),
                mesh_mode: ControllerMeshMode::Locked { sd_cert, sd_priv, credentials_checker },
            }
        };

        let (power_state_tx, power_state_rx) = broadcast::channel(1);

        let run_handle = tokio_runtime.spawn(async move {
            controller.run(power_state_rx).await;
        });

        // Client's VmConfig, with roles swapped.
        let client_vvm_config = VvmConfig {
            own_cfg: vvm_config.remote_cfgs[0].clone(),
            remote_cfgs: vec![vvm_config.own_cfg.clone()],
            ..(*vvm_config).clone()
        };

        // Client's identity ("foo")
        let client_sd_priv: PKey<Private> = build_ec_private_key().unwrap().try_into().unwrap();
        // Use the same uds and auth_csr as the server, for simplicity.
        let client_sd_cert: X509 =
            SdClaimsBuilder::new(&client_vvm_config.own_cfg.name, &uds, &auth_csr)
                .with_sd_private_key(&client_sd_priv)
                .build()
                .unwrap()
                .to_x509(&client_sd_priv)
                .unwrap();

        // Client's credentials checker, to verify server's ("bar") identity.
        let client_credentials_checker =
            crate::mesh::credentials_checker::tests::create_custom_credentials_checker(
                &uds,
                client_vvm_config.clone(),
                &auth_csr.cbor,
            )
            .unwrap();

        let client_connector =
            SecureMeshConnector::new(client_credentials_checker, &client_sd_cert, &client_sd_priv)
                .unwrap();

        tokio_runtime.block_on(async {
            // Server should start, as controller assumes power is on before reading power events.
            server_port_rx.wait_for(|p| p.is_some()).await.unwrap();
            let port = server_port_rx.borrow().unwrap();
            let mut server_addr = vvm_config.own_cfg.addr;
            server_addr.set_port(port);

            // Ping the server to confirm it's up.
            let client = client_connector
                .try_connect(env.clone(), &server_addr, vvm_config.own_cfg.name.as_ref())
                .await
                .unwrap();
            client.ping(&PingRequest::default()).await.unwrap();

            // Turn power off, server should stop.
            power_state_tx.send(SimplePowerState::Off).unwrap();
            server_port_rx.wait_for(|p| p.is_none()).await.unwrap();

            // Try to connect and expect failure.
            let connection_result = client_connector
                .try_connect(env.clone(), &server_addr, vvm_config.own_cfg.name.as_ref())
                .await;
            assert!(connection_result.is_err(), "Connection should fail when server is off");
        });

        // Makes MeshServiceController::run() finally return.
        drop(power_state_tx);

        // Wait for the controller to finish. It will do so because the power_state channel is closed.
        tokio_runtime.block_on(run_handle).unwrap();
    }

    #[test]
    fn unlocked_mesh_service_controller_power_state() {
        use crate::mesh::async_client::UnsecureMeshConnector;

        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());

        let vvm_config = Arc::new(prepare_test_vm_config().unwrap());

        let env = Arc::new(EnvBuilder::new().build());
        let (new_client_notifier, _) = broadcast::channel(1);

        let local_uds_keys = vec![create_example_key1(), create_example_key2()];

        let mesh_mode = MeshMode::new_unlocked(local_uds_keys.clone()).unwrap();

        let server_factory = Box::new(ServerFactoryImpl {
            local_vm_acl_policy_provider: Arc::new(TestSingleVmAclPolicyProvider::new()),
            service_registry: Arc::new(ServiceRegistry::default()),
            vvm_config: vvm_config.clone(),
            mesh_mode,
            new_client_notifier,
        });

        let (server_port_tx, mut server_port_rx) = watch::channel(None);

        let controller = MeshServiceController {
            server_factory,
            env: env.clone(),
            server_port_tx,
            retry_min_duration: Duration::from_millis(10),
            retry_max_duration: Duration::from_millis(160),
            mesh_mode: ControllerMeshMode::Unlocked,
        };

        let (power_state_tx, power_state_rx) = broadcast::channel(1);

        let run_handle = tokio_runtime.spawn(async move {
            controller.run(power_state_rx).await;
        });

        tokio_runtime.block_on(async {
            // Server should start, as controller assumes power is on before reading power events.
            server_port_rx.wait_for(|p| p.is_some()).await.unwrap();
            let port = server_port_rx.borrow().unwrap();
            let mut server_addr = vvm_config.own_cfg.addr;
            server_addr.set_port(port);

            // Ping the server to confirm it's up.
            let mesh_connector = UnsecureMeshConnector {};
            let client = mesh_connector
                .try_connect(env.clone(), &server_addr, vvm_config.own_cfg.name.as_ref())
                .await
                .unwrap();
            client.ping(&PingRequest::default()).await.unwrap();

            // Check that GetUdsPub() works as expected
            let mut expected_uds_pubs_cbor: Vec<Vec<u8>> =
                local_uds_keys.iter().map(|k| k.clone().to_vec().unwrap()).collect();

            let mut received_uds_pubs = client.get_uds_pub().await.unwrap();

            // Sort to compare since order is not guaranteed.
            expected_uds_pubs_cbor.sort();
            received_uds_pubs.sort();

            assert_eq!(received_uds_pubs, expected_uds_pubs_cbor);

            // Turn power off, server should stop.
            power_state_tx.send(SimplePowerState::Off).unwrap();
            server_port_rx.wait_for(|p| p.is_none()).await.unwrap();

            // Try to connect and expect failure.
            let connection_result = mesh_connector
                .try_connect(env.clone(), &server_addr, vvm_config.own_cfg.name.as_ref())
                .await;
            assert!(connection_result.is_err(), "Connection should fail when server is off");
        });

        // Makes MeshServiceController::run() finally return.
        drop(power_state_tx);

        // Wait for the controller to finish. It will do so because the power_state channel is closed.
        tokio_runtime.block_on(run_handle).unwrap();
    }

    #[tokio::test]
    async fn mesh_service_controller_retries_on_start_failure() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let mut mock_factory = MockServerFactory::new();

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();
        mock_factory.expect_start_unsecure().returning(move |_| {
            let count = call_count_clone.fetch_add(1, Ordering::SeqCst);
            if count < 2 {
                Err(anyhow::anyhow!("Failed to start"))
            } else {
                Ok((Box::new(MockServer::new()), 12345))
            }
        });

        let (server_port_tx, server_port_rx) = watch::channel(None);
        let (power_state_tx, power_state_rx) = broadcast::channel(16);

        let controller = MeshServiceController {
            server_factory: Box::new(mock_factory),
            env: Arc::new(EnvBuilder::new().build()),
            server_port_tx,
            // Keep the sleeps short so that the test runs fast.
            retry_min_duration: Duration::from_millis(10),
            retry_max_duration: Duration::from_millis(160),
            mesh_mode: ControllerMeshMode::Unlocked,
        };

        let run_handle = tokio::spawn(async move {
            controller.run(power_state_rx).await;
        });

        // Wait for 3 calls to start_unsecure. The first two fail, the third succeeds.
        wait_for(move || call_count.load(Ordering::SeqCst) >= 3).await.unwrap();

        // After the successful attempt, the port should be published.
        wait_for(move || *server_port_rx.borrow() == Some(12345)).await.unwrap();

        drop(power_state_tx);
        run_handle.await.unwrap();
    }

    #[tokio::test]
    async fn mesh_service_controller_stops_retrying_on_power_off() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let mut mock_factory = MockServerFactory::new();

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();
        mock_factory.expect_start_unsecure().returning(move |_| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            Err(anyhow::anyhow!("Failed to start"))
        });

        let (server_port_tx, _server_port_rx) = watch::channel(None);
        let (power_state_tx, power_state_rx) = broadcast::channel(16);

        let controller = MeshServiceController {
            server_factory: Box::new(mock_factory),
            env: Arc::new(EnvBuilder::new().build()),
            server_port_tx,
            // Basically don't sleep, so that the test runs fast
            retry_min_duration: Duration::from_millis(1),
            retry_max_duration: Duration::from_millis(2),
            mesh_mode: ControllerMeshMode::Unlocked,
        };

        let run_handle = tokio::spawn(async move {
            controller.run(power_state_rx).await;
        });

        // Wait for the first attempt to start the server.
        let call_count_clone = call_count.clone();
        wait_for(move || call_count_clone.load(Ordering::SeqCst) >= 1).await.unwrap();

        // Send power off event.
        power_state_tx.send(SimplePowerState::Off).unwrap();

        // Give some time to ensure the controller task has read and processed the power state
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(power_state_tx.len(), 0);

        let retries_after_stop = call_count.load(Ordering::SeqCst);

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Check that the number of retries does not grow overtime.
        // Which means that the controller indeed stopped retrying and is thus
        // sitting idle.
        assert_eq!(call_count.load(Ordering::SeqCst), retries_after_stop);

        drop(power_state_tx);
        run_handle.await.unwrap();
    }
}
