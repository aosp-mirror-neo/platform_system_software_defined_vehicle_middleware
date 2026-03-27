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

//! SDV service discovery daemon

mod authz;
mod ca;
mod identity;
mod kv;
mod listeners;
mod mesh;
mod power_state;
mod service_discovery_agent;
mod service_registration_agent;
mod sha256;
mod system;
mod transport_agent;
mod unit_type_utils;

use crate::authz::{AuthzBinderService, MeshAclPolicyProvider};
use crate::ca::service::{CAService, DisabledCAService, RootCertCommand};
use crate::identity::service_v2::IdentityAgentV2;
use crate::kv::ServiceRegistry;
use crate::mesh::async_client::{MeshConnector, SecureMeshConnector, UnsecureMeshConnector};
use crate::mesh::fetch_local_vm_uds_pubs;
use crate::mesh::manager::{self, MeshManager, MeshManagerParams};
use crate::mesh::VvmConfig;
use crate::mesh::{get_sdv_boot_mode, SdvBootMode};
use crate::mesh::{sd_certificate, service::MeshServiceController};
use crate::mesh::{
    DisabledUdsPubsProvisioningService, MeshStatusService, UdsPubsProvisioner,
    UdsPubsProvisioningService,
};
use crate::power_state::{run_power_state_reporter, SimplePowerState};
use crate::service_discovery_agent::service::ServiceDiscoveryAgentService;
use crate::service_registration_agent::service::ServiceRegistrationAgent;
use crate::transport_agent::service::TransportSupportAgentService;
use mesh::{
    credentials_checker::CredentialsChecker,
    service::{LockedMeshServiceControllerBuilder, UnlockedMeshServiceControllerBuilder},
};

use crypto_rpc::cert::{build_ec_private_key, build_root_certificate};

use sdv_authz::AuthzService;
use sdv_authz_enforcement_flag::SystemPropertyAuthzEnforcementFlag;
use sdv_authz_local_vm_acl_policy_provider_factory::create_local_vm_acl_policy_provider;

use anyhow::{Context, Result};
use binder::BinderFeatures;
use grpcio::{EnvBuilder, Environment};
use log::{error, info};
use paste::paste;

use std::sync::{Arc, Mutex};

use openssl::{
    pkey::{PKey, Private},
    x509::X509,
};

use tokio::sync::{
    broadcast,
    mpsc::{unbounded_channel, UnboundedReceiver},
    watch,
};

use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceRegistrationAgent::{
    IServiceRegistrationAgent, BpServiceRegistrationAgent, BnServiceRegistrationAgent};

use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent::{
        ITransportSupportAgent, BpTransportSupportAgent, BnTransportSupportAgent};

use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent::{
    IServiceDiscoveryAgent, BpServiceDiscoveryAgent, BnServiceDiscoveryAgent};

use google_sdv_authz::aidl::google::sdv::authz::IAuthzService::{
    BnAuthzService, BpAuthzService, IAuthzService,
};

use google_sdv_privileged_service_discovery::aidl::google::sdv::privileged::service_discovery::IPrivilegedServiceDiscoveryAgent::{
    IPrivilegedServiceDiscoveryAgent, BpPrivilegedServiceDiscoveryAgent, BnPrivilegedServiceDiscoveryAgent};

use google_sdv_privileged_service_discovery::aidl::google::sdv::privileged::service_discovery::IPrivilegedServiceRegistrationAgent::{
    IPrivilegedServiceRegistrationAgent, BpPrivilegedServiceRegistrationAgent, BnPrivilegedServiceRegistrationAgent};

use google_sdv_privileged_identity::aidl::google::sdv::privileged::identity::IPrivilegedIdentityAgent::{
    IPrivilegedIdentityAgent, BpPrivilegedIdentityAgent, BnPrivilegedIdentityAgent};

static LOG_TAG: &str = "SdvServiceManagerServer";

macro_rules! register_binder_ipc_server {
    ($pkg:ident::$name:ident: $server: block) => {
        paste! {{
            let descriptor =
                <[<google_sdv_ $pkg>]::aidl::google::sdv::$pkg::[<I $name>]::[<Bp $name>]
                    as [<google_sdv_ $pkg>]::aidl::google::sdv::$pkg::[<I $name>]::[<I $name>]>
                    ::get_descriptor().to_owned() + "/default";
            let service = [<google_sdv_ $pkg>]::aidl::google::sdv::$pkg::[<I $name>]::[<Bn $name>]::
                new_async_binder($server, binder_rt(), binder_features());
            binder::add_service(&descriptor, service.as_binder())
                .with_context(||
                    format!("Couldn't register Binder Server for SDV SD {}", stringify!($name)))
        }}
    };
}

fn register_service_registration_agent(
    service_registry: Arc<ServiceRegistry>,
    authz_service: Arc<AuthzService>,
) -> Result<()> {
    let descriptor = <BpServiceRegistrationAgent as IServiceRegistrationAgent>::get_descriptor()
        .to_owned()
        + "/default";
    let service = BnServiceRegistrationAgent::new_async_binder(
        ServiceRegistrationAgent::new(service_registry, authz_service),
        binder_rt(),
        binder_features(),
    );
    binder::add_service(&descriptor, service.as_binder()).with_context(|| {
        "Couldn't register Binder Server for SDV SD IServiceRegistrationAgent".to_string()
    })?;
    Ok(())
}

fn register_service_transport_support_agent(service_registry: Arc<ServiceRegistry>) -> Result<()> {
    let descriptor = <BpTransportSupportAgent as ITransportSupportAgent>::get_descriptor()
        .to_owned()
        + "/default";
    let service = BnTransportSupportAgent::new_async_binder(
        TransportSupportAgentService::new(service_registry),
        binder_rt(),
        binder_features(),
    );
    binder::add_service(&descriptor, service.as_binder()).with_context(|| {
        "Couldn't register Binder Server for SDV SD ITransportSupportAgent".to_string()
    })?;
    Ok(())
}

fn register_service_discovery_agent(
    authz_service: Arc<AuthzService>,
    service_registry: Arc<ServiceRegistry>,
) -> Result<()> {
    let descriptor = <BpServiceDiscoveryAgent as IServiceDiscoveryAgent>::get_descriptor()
        .to_owned()
        + "/default";
    let service = BnServiceDiscoveryAgent::new_async_binder(
        ServiceDiscoveryAgentService::new(service_registry, authz_service),
        binder_rt(),
        binder_features(),
    );
    binder::add_service(&descriptor, service.as_binder()).with_context(|| {
        "Couldn't register Binder Server for SDV SD IServiceDiscoveryAgent".to_string()
    })?;
    Ok(())
}

fn register_authz_service(authz_service: Arc<AuthzService>) -> Result<()> {
    let descriptor = <BpAuthzService as IAuthzService>::get_descriptor().to_owned() + "/default";
    let service =
        BnAuthzService::new_binder(AuthzBinderService::new(authz_service), binder_features());
    binder::add_service(&descriptor, service.as_binder())
        .with_context(|| "Couldn't register Binder Server for SDV SD IAuthzService".to_string())?;
    Ok(())
}

fn register_privileged_identity_agent(identity_agent: IdentityAgentV2) -> Result<()> {
    let descriptor = <BpPrivilegedIdentityAgent as IPrivilegedIdentityAgent>::get_descriptor()
        .to_owned()
        + "/default";
    let service =
        BnPrivilegedIdentityAgent::new_async_binder(identity_agent, binder_rt(), binder_features());
    binder::add_service(&descriptor, service.as_binder()).with_context(|| {
        "Couldn't register Binder Server for SDV SD IPrivilegedIdentityAgent".to_string()
    })?;
    Ok(())
}

fn register_privileged_service_discovery_agent(
    service_registry: Arc<ServiceRegistry>,
    authz_service: Arc<AuthzService>,
) -> Result<()> {
    let descriptor =
        <BpPrivilegedServiceDiscoveryAgent as IPrivilegedServiceDiscoveryAgent>::get_descriptor()
            .to_owned()
            + "/default";
    let service = BnPrivilegedServiceDiscoveryAgent::new_async_binder(
        ServiceDiscoveryAgentService::new(service_registry, authz_service),
        binder_rt(),
        binder_features(),
    );
    binder::add_service(&descriptor, service.as_binder()).with_context(|| {
        "Couldn't register Binder Server for SDV SD IPrivilegedServiceDiscoveryAgent".to_string()
    })?;
    Ok(())
}

fn register_privileged_service_registration_agent(
    service_registry: Arc<ServiceRegistry>,
    authz_service: Arc<AuthzService>,
) -> Result<()> {
    let descriptor =
        <BpPrivilegedServiceRegistrationAgent as IPrivilegedServiceRegistrationAgent>::get_descriptor()
            .to_owned()
            + "/default";
    let service = BnPrivilegedServiceRegistrationAgent::new_async_binder(
        ServiceRegistrationAgent::new(service_registry, authz_service),
        binder_rt(),
        binder_features(),
    );
    binder::add_service(&descriptor, service.as_binder()).with_context(|| {
        "Couldn't register Binder Server for SDV SD IPrivilegedServiceRegistrationAgent".to_string()
    })?;
    Ok(())
}

#[inline]
fn binder_rt() -> binder_tokio::TokioRuntime<tokio::runtime::Runtime> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    binder_tokio::TokioRuntime(rt)
}

#[inline]
fn binder_features() -> binder::BinderFeatures {
    BinderFeatures::default()
}

enum BootModeParams {
    Unlocked(Arc<Mutex<UdsPubsProvisioner>>),
    Locked(UnboundedReceiver<RootCertCommand>, PKey<Private>, X509),
}

fn run_binder_servers(
    authz_service: Arc<AuthzService>,
    service_registry: Arc<ServiceRegistry>,
    mesh_manager: Arc<MeshManager>,
    vvm_config: Arc<VvmConfig>,
    boot_mode_params: BootModeParams,
    tokio_runtime: Arc<tokio::runtime::Runtime>,
) -> Result<()> {
    match boot_mode_params {
        BootModeParams::Unlocked(uds_pubs_provisioner) => {
            UdsPubsProvisioningService::create_and_register(
                uds_pubs_provisioner,
                binder_rt(),
                binder_features(),
            )?;
            info!("uds_pubs provisioning service running...");

            DisabledCAService::create_and_register(binder_rt(), binder_features())?;
            info!("Disabled ICertificateAuthority service running...");
        }
        BootModeParams::Locked(ca_commands_receiver, root_key, root_cert) => {
            DisabledUdsPubsProvisioningService::create_and_register(
                binder_rt(),
                binder_features(),
            )?;
            info!("Disabled uds_pubs provisioning service running...");

            CAService::create_and_register(
                tokio_runtime,
                binder_features(),
                ca_commands_receiver,
                service_registry.clone(),
                root_key,
                root_cert,
            )?;
            info!("ICertificateAuthority service running...");
        }
    }

    MeshStatusService::create_and_register(mesh_manager, binder_rt(), binder_features())?;
    info!("Mesh status service running...");

    let identity_agent = IdentityAgentV2::new(service_registry.clone(), &vvm_config)?;
    let identity_agent_copy = IdentityAgentV2::new_shallow_copy(&identity_agent);
    register_binder_ipc_server!(identity::IdentityAgent: {
        identity_agent
    })?;
    info!("SDV IdentityAgent running...");

    register_service_registration_agent(service_registry.clone(), authz_service.clone())?;
    info!("SDV IServiceRegistrationAgent running...");

    register_service_transport_support_agent(service_registry.clone())?;
    info!("SDV ITransportSupportAgent running...");

    register_service_discovery_agent(authz_service.clone(), service_registry.clone())?;
    info!("SDV IServiceDiscoveryAgent running...");

    register_authz_service(authz_service.clone())?;
    info!("SDV IAuthzService running...");

    register_privileged_identity_agent(identity_agent_copy)?;
    info!("SDV IPrivilegedIdentityAgent running...");

    register_privileged_service_discovery_agent(service_registry.clone(), authz_service.clone())?;
    info!("SDV IPrivilegedServiceDiscoveryAgent running...");

    register_privileged_service_registration_agent(service_registry.clone(), authz_service)?;
    info!("SDV IPrivilegedServiceRegistrationAgent running...");

    Ok(())
}

/// The private key and certificate of this VM's certificate authority
struct PrivateKeyAndCert {
    /// A private key
    pub root_private_key: PKey<Private>,
    /// The self-signed certificate for root_private_key's public key.
    pub root_cert: openssl::x509::X509,
}

impl PrivateKeyAndCert {
    /// Crates a new root private key and corresponding root certificate
    pub fn create(vm_name: &str) -> anyhow::Result<Self> {
        let root_private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let root_cert = build_root_certificate(&root_private_key, vm_name)?;
        Ok(Self { root_private_key, root_cert })
    }
}

fn main() {
    if let Err(e) = run() {
        // Use the alternate form (#) to ensure anyhow errors are printed with their cause in
        // the same line, making both context and cause visible in logcat.
        error!("{e:#}")
    }
}

fn run() -> Result<()> {
    sdv_log::init_logger(LOG_TAG)?;

    sdv_tracing::init_tracing();

    let vvm_config = Arc::new(VvmConfig::load_from_file()?);
    info!("Loaded VvmConfig successfully!");

    let tokio_runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap(),
    );

    binder::ProcessState::start_thread_pool();

    let boot_mode = get_sdv_boot_mode()?;
    info!("SDV Boot Mode is {boot_mode:?}");

    let local_vm_acl_policy_provider = create_local_vm_acl_policy_provider()?;

    let mesh_acl_policy_provider = Arc::new(MeshAclPolicyProvider::new(
        vvm_config.own_cfg.name.clone(),
        local_vm_acl_policy_provider.clone(),
    ));

    #[cfg(sdv_sd_agent_for_a15)]
    let authz_flag = Arc::new(SystemPropertyAuthzEnforcementFlag::new_for_a15());

    #[cfg(not(sdv_sd_agent_for_a15))]
    let authz_flag = Arc::new(SystemPropertyAuthzEnforcementFlag::new());

    let service_registry = Arc::new(ServiceRegistry::default());

    let authz_service = Arc::new(AuthzService::new(
        vvm_config.own_cfg.name.clone(),
        mesh_acl_policy_provider.clone(),
        authz_flag,
        service_registry.clone(),
    ));

    let env = Arc::new(EnvBuilder::new().build());
    // We use a broadcast channel to ensure that no power state changes are missed.
    // Capacity is 16, which should be more than enough to buffer state changes if a
    // consumer is slow.
    let power_state_tx = broadcast::Sender::new(16);

    // We only pass clones of this sender to both producers and consumers. Consumers
    // will get a fresh broadcast::Receiver out of the sender by calling sender.subscribe()
    //
    // Capacity is 1 since we don't care about the value that is sent, an empty tuple. It will just
    // get overwritten with yet another empty tuple if the receiver doesn't read it in time.
    let mesh_manager_wake_notifier: broadcast::Sender<()> = broadcast::Sender::new(1);

    let (mesh_manager, mesh_service_controller, boot_mode_params) = match boot_mode {
        SdvBootMode::Locked => setup_locked_mode(
            Arc::clone(&vvm_config),
            mesh_acl_policy_provider,
            Arc::clone(&service_registry),
            Arc::clone(&authz_service),
            Arc::clone(&env),
            tokio_runtime.clone(),
            power_state_tx.clone(),
            mesh_manager_wake_notifier.clone(),
        )?,
        SdvBootMode::Unlocked => setup_unlocked_mode(
            Arc::clone(&vvm_config),
            mesh_acl_policy_provider,
            Arc::clone(&service_registry),
            Arc::clone(&authz_service),
            Arc::clone(&env),
            tokio_runtime.clone(),
            power_state_tx.clone(),
            mesh_manager_wake_notifier.clone(),
        )?,
    };

    run_binder_servers(
        authz_service,
        service_registry,
        Arc::clone(&mesh_manager),
        vvm_config,
        boot_mode_params,
        tokio_runtime.clone(),
    )?;

    let power_state_rx = power_state_tx.subscribe();
    tokio_runtime.spawn(async move {
        mesh_service_controller.run(power_state_rx).await;
        // This must not happen and is not recoverable. Can only be caused by a closed
        // SimplePowerState channel, which would be a programming error.
        panic!("MeshService permanently stopped.");
    });

    let tokio_runtime_clone = tokio_runtime.clone();
    let watcher_state_rx = mesh_manager.subscribe_to_watcher_thread_state();
    tokio_runtime.spawn(async {
        run_power_state_reporter(tokio_runtime_clone, power_state_tx, watcher_state_rx).await;
    });

    let tokio_runtime_clone = tokio_runtime.clone();
    tokio_runtime.block_on(async {
        let all_tasks_joined = mesh_manager.spawn_watcher_tasks(tokio_runtime_clone);
        info!("SD Agent has started successfully");
        all_tasks_joined.await;
    });

    // This should actually never happen as the watcher tasks loop forever.
    info!("Mesh tasks stopped. Exiting...");

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn setup_locked_mode(
    vvm_config: Arc<VvmConfig>,
    mesh_acl_policy_provider: Arc<MeshAclPolicyProvider>,
    service_registry: Arc<ServiceRegistry>,
    authz_service: Arc<AuthzService>,
    env: Arc<Environment>,
    tokio_runtime: Arc<tokio::runtime::Runtime>,
    power_state_tx: broadcast::Sender<SimplePowerState>,
    mesh_manager_wake_notifier: broadcast::Sender<()>,
) -> Result<(Arc<MeshManager>, MeshServiceController, BootModeParams)> {
    let pkey_and_cert = PrivateKeyAndCert::create(vvm_config.own_cfg.name.as_ref())?;
    let (ca_command_sender, ca_command_receiver) = unbounded_channel();
    let credentials_checker = CredentialsChecker::load((*vvm_config).clone())?;
    let (sd_cert, sd_priv) = sd_certificate::create(&vvm_config)?;
    let connector =
        Box::new(SecureMeshConnector::new(credentials_checker.clone(), &sd_cert, &sd_priv)?)
            as Box<dyn MeshConnector>;

    let root_cert_pem = pkey_and_cert.root_cert.to_pem()?;

    let mesh_manager_params = MeshManagerParams {
        mesh_acl_policy_provider,
        service_registry: service_registry.clone(),
        authz_service: authz_service.clone(),
        vvm_config: vvm_config.clone(),
        connector,
        mesh_mode: manager::MeshMode::Locked(manager::LockedMesh {
            local_root_cert: root_cert_pem.clone(),
            ca_command_sender: ca_command_sender.clone(),
        }),
        env: Arc::clone(&env),
        wake_notifier: mesh_manager_wake_notifier.clone(),
        power_state_tx,
    };
    let mesh_manager = Arc::new(MeshManager::new(mesh_manager_params));

    let mesh_service_controller = LockedMeshServiceControllerBuilder {
        server_port_tx: watch::channel(None).0,
        service_registry,
        vvm_config,
        root_cert_pem,
        ca_command_sender,
        tokio_runtime,
        mesh_manager_wake_notifier,
        env,
        sd_cert,
        sd_priv,
        credentials_checker,
    }
    .build()?;

    Ok((
        mesh_manager,
        mesh_service_controller,
        BootModeParams::Locked(
            ca_command_receiver,
            pkey_and_cert.root_private_key,
            pkey_and_cert.root_cert,
        ),
    ))
}

/// Mode used in a factory settings and development.
#[allow(clippy::too_many_arguments)]
fn setup_unlocked_mode(
    vvm_config: Arc<VvmConfig>,
    mesh_acl_policy_provider: Arc<MeshAclPolicyProvider>,
    service_registry: Arc<ServiceRegistry>,
    authz_service: Arc<AuthzService>,
    env: Arc<Environment>,
    tokio_runtime: Arc<tokio::runtime::Runtime>,
    power_state_tx: broadcast::Sender<SimplePowerState>,
    mesh_manager_wake_notifier: broadcast::Sender<()>,
) -> Result<(Arc<MeshManager>, MeshServiceController, BootModeParams)> {
    let local_vm_uds_pubs = fetch_local_vm_uds_pubs()?;

    let uds_pubs_provisioner = Arc::new(Mutex::new(UdsPubsProvisioner::try_new(
        &vvm_config,
        local_vm_uds_pubs.clone(),
        tokio_runtime,
    )?));

    let mesh_manager_params = MeshManagerParams {
        mesh_acl_policy_provider,
        service_registry: service_registry.clone(),
        authz_service: authz_service.clone(),
        vvm_config: vvm_config.clone(),
        connector: Box::new(UnsecureMeshConnector {}),
        mesh_mode: manager::MeshMode::Unlocked(manager::UnlockedMesh {
            uds_pubs_provisioner: uds_pubs_provisioner.clone(),
        }),
        env: Arc::clone(&env),
        wake_notifier: mesh_manager_wake_notifier.clone(),
        power_state_tx,
    };
    let mesh_manager = Arc::new(MeshManager::new(mesh_manager_params));

    let (server_port_tx, _) = watch::channel(None);
    let mesh_service_controller = UnlockedMeshServiceControllerBuilder {
        server_port_tx,
        service_registry,
        vvm_config,
        local_vm_uds_pubs,
        mesh_manager_wake_notifier,
        env,
    }
    .build()?;

    Ok((mesh_manager, mesh_service_controller, BootModeParams::Unlocked(uds_pubs_provisioner)))
}
