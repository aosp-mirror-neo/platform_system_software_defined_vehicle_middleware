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

use crate::kv::{process_identifier::get_calling_process_identifier, ServiceRegistry};
use async_trait::async_trait;
use google_sdv_ca::aidl::google::sdv::ca::{
    IAuthoritiesListener::IAuthoritiesListener,
    ICertificateAuthority::{
        BnCertificateAuthority, BpCertificateAuthority, ICertificateAuthority,
        ICertificateAuthorityAsyncServer,
    },
};
use google_sdv_ca::binder;
use google_sdv_ca::binder::{Status, StatusCode};
use itertools::Itertools;
use openssl::{
    pkey::{PKey, Private, Public},
    x509::{X509Req, X509},
};

use anyhow::{bail, Context};
use crypto_rpc::cert::{build_leaf_certificate, chain_root_to_leaf, verify_request};
use log::{debug, error, info, warn};
use sdv_comms_id::ServiceFqin;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::instrument;

struct ListenerVec(Vec<binder::Strong<dyn IAuthoritiesListener>>);

pub(crate) struct CAService {
    state: Arc<Mutex<CAServiceState>>,
}

struct RootCertEntry {
    /// PEM-encoded X.509 root certificate
    root_cert: String,

    /// How many times this cert was added
    add_count: u16,
}

impl RootCertEntry {
    pub fn new(root_cert: String) -> Self {
        Self { root_cert, add_count: 1 }
    }
    pub fn increment_add_count(&mut self, vm_name: &str) -> anyhow::Result<()> {
        if self.add_count == u16::MAX {
            bail!("Reached add limit for the root cert of {vm_name}");
        }
        self.add_count += 1;
        debug!("add_count of {vm_name} root cert increased to {}", self.add_count);
        Ok(())
    }
    pub fn replace_root_cert(&mut self, new_root_cert: String) {
        self.root_cert = new_root_cert;
        self.add_count = 1;
    }
}

struct CAServiceState {
    root_key: PKey<Private>,
    root_cert: X509,
    root_cert_pem: String,

    // Dynamically populated collection of PEM-encoded certificates for VMs.
    vm_certificates: HashMap<String, RootCertEntry>,

    service_registry: Arc<ServiceRegistry>,
    listeners: ListenerVec,
}

/// A command to add or remove a root certificate to/from the CA service.
#[derive(Debug)]
pub(crate) enum RootCertCommand {
    /// Add the root certificate of the given VM.
    Add { vm_name: String, root_cert: Vec<u8> },
    /// Remove the root certificate of the given VM.
    Remove { vm_name: String, root_cert: Vec<u8> },
}

pub(crate) struct DisabledCAService {}

impl binder::Interface for CAService {}
impl binder::Interface for DisabledCAService {}

#[allow(non_snake_case)] // AIDL-generated names
#[async_trait]
impl ICertificateAuthorityAsyncServer for CAService {
    #[instrument(name = "sd::ICertificateAuthority::requestCertification", skip_all)]
    async fn requestCertification(&self, request: &str) -> binder::Result<String> {
        let state = self.state.lock().unwrap();
        let caller_fqin = state
            .service_registry
            .get_process_fqin(&get_calling_process_identifier())
            .map_err(|e| {
                warn!("{e}");
                Status::from(StatusCode::PERMISSION_DENIED)
            })?;

        Ok(state.process_certificate_request(request, caller_fqin).map_err(|e| {
            warn!("{e}");
            Status::from(StatusCode::BAD_VALUE)
        })?)
    }

    #[instrument(name = "sd::ICertificateAuthority::addAuthoritiesListener", skip_all)]
    async fn addAuthoritiesListener(
        &self,
        listener: &binder::Strong<dyn IAuthoritiesListener>,
    ) -> binder::Result<()> {
        let mut state = self.state.lock().unwrap();

        // Notify about the current state right away. Do not persist the listener in case of the failure.
        listener.onAuthoritiesChanged(&(state.get_vm_certificates()))?;

        state.listeners.0.push(listener.clone());

        Ok(())
    }

    #[instrument(name = "sd::ICertificateAuthority::removeAuthoritiesListener", skip_all)]
    async fn removeAuthoritiesListener(
        &self,
        listener: &binder::Strong<dyn IAuthoritiesListener>,
    ) -> binder::Result<()> {
        let mut state = self.state.lock().unwrap();
        state.listeners.0.retain(|l| l != listener);

        Ok(())
    }

    #[instrument(name = "sd::ICertificateAuthority::isEnabled", skip_all)]
    async fn isEnabled(&self) -> binder::Result<bool> {
        Ok(true)
    }
}

#[allow(non_snake_case)] // AIDL-generated names
#[async_trait]
impl ICertificateAuthorityAsyncServer for DisabledCAService {
    async fn requestCertification(&self, _request: &str) -> binder::Result<String> {
        Err(Status::new_service_specific_error_str(-1, Some("Service is disabled")))
    }
    async fn addAuthoritiesListener(
        &self,
        _listener: &binder::Strong<dyn IAuthoritiesListener>,
    ) -> binder::Result<()> {
        Err(Status::new_service_specific_error_str(-1, Some("Service is disabled")))
    }
    async fn removeAuthoritiesListener(
        &self,
        _listener: &binder::Strong<dyn IAuthoritiesListener>,
    ) -> binder::Result<()> {
        Err(Status::new_service_specific_error_str(-1, Some("Service is disabled")))
    }
    async fn isEnabled(&self) -> binder::Result<bool> {
        Ok(false)
    }
}

impl CAServiceState {
    fn process_certificate_request(
        &self,
        request: &str,
        caller_fqin: ServiceFqin,
    ) -> anyhow::Result<String> {
        let x509_req = X509Req::from_pem(request.as_bytes())
            .context("Failed to parse X509Req from request string")?;

        let caller_fqin_str: String = caller_fqin.into();

        verify_request(&x509_req, &caller_fqin_str)?;

        let cert_chain = self.issue_certificate(&x509_req.public_key()?, &caller_fqin_str)?;

        info!("Issued certificate to {caller_fqin_str}");

        Ok(String::from_utf8(cert_chain)?)
    }

    fn issue_certificate(
        &self,
        subject_public_key: &PKey<Public>,
        subject_name: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let leaf_cert_pem = build_leaf_certificate(
            &self.root_key,
            &self.root_cert,
            subject_public_key,
            subject_name,
        )?
        .to_pem()?;

        Ok(chain_root_to_leaf(self.root_cert_pem.clone().into_bytes(), leaf_cert_pem))
    }

    pub(crate) fn apply_root_cert_command(
        &mut self,
        command: RootCertCommand,
    ) -> anyhow::Result<()> {
        match command {
            RootCertCommand::Add { vm_name, root_cert } => self.add_root_cert(vm_name, root_cert),
            RootCertCommand::Remove { vm_name, root_cert } => {
                self.remove_root_cert(vm_name, root_cert)
            }
        }
    }

    fn add_root_cert(&mut self, vm_name: String, root_cert: Vec<u8>) -> anyhow::Result<()> {
        let pem_certificate = String::from_utf8(root_cert)?;

        match self.vm_certificates.get_mut(&vm_name) {
            Some(entry) => {
                if entry.root_cert == pem_certificate {
                    entry.increment_add_count(&vm_name)?;
                } else {
                    entry.replace_root_cert(pem_certificate);
                    info!("Root cert of {vm_name} has been replaced");
                    self.notify_authorities_changed();
                }
            }
            None => {
                info!("Added root cert of {vm_name} to list of VM certs");
                self.vm_certificates.insert(vm_name, RootCertEntry::new(pem_certificate));
                self.notify_authorities_changed();
            }
        }
        Ok(())
    }

    fn remove_root_cert(&mut self, vm_name: String, root_cert: Vec<u8>) -> anyhow::Result<()> {
        let pem_certificate = String::from_utf8(root_cert)?;

        let should_remove = match self.vm_certificates.get_mut(&vm_name) {
            Some(entry) => {
                if entry.root_cert != pem_certificate {
                    debug!("Ignoring Remove command for a different root cert of {vm_name}");
                    return Ok(());
                }
                if entry.add_count > 0 {
                    entry.add_count -= 1;
                    debug!("add_count of {vm_name} root cert decreased to {}", entry.add_count);
                }
                entry.add_count == 0
            }
            None => {
                bail!("No root cert for {vm_name} to be removed.");
            }
        };

        if should_remove {
            self.vm_certificates.remove(&vm_name);
            info!("Removed root cert of {vm_name} from list of VM certs");
            self.notify_authorities_changed();
        }

        Ok(())
    }

    /// Notify authorities listeners and remove dead ones.
    fn notify_authorities_changed(&mut self) {
        let certificates = self.get_vm_certificates();

        self.listeners.0.retain(|listener| {
            if let Err(e) = listener.onAuthoritiesChanged(&certificates) {
                if e.transaction_error() == StatusCode::DEAD_OBJECT {
                    debug!("Dropping IAuthoritiesListeners that have died.");
                    return false;
                }
                error!("Failed to notify listeners: {e:?}");
            }
            true
        });
    }

    fn get_vm_certificates(&self) -> String {
        self.vm_certificates
            .values()
            .map(|entry| &entry.root_cert)
            .chain(std::iter::once(&self.root_cert_pem))
            .join("")
    }
}

async fn process_root_cert_commands(
    ca_state: Arc<Mutex<CAServiceState>>,
    commands_receiver: UnboundedReceiver<RootCertCommand>,
) {
    let mut commands_receiver = commands_receiver;
    while let Some(command) = commands_receiver.recv().await {
        if let Err(error) = ca_state.lock().unwrap().apply_root_cert_command(command) {
            error!("Failed to apply command: {error}");
        }
    }
}

impl CAService {
    pub(crate) fn create_and_register(
        tokio_runtime: Arc<tokio::runtime::Runtime>,
        features: binder::BinderFeatures,
        commands_receiver: UnboundedReceiver<RootCertCommand>,
        service_registry: Arc<ServiceRegistry>,
        root_key: PKey<Private>,
        root_cert: X509,
    ) -> anyhow::Result<()> {
        let service_state = CAServiceState {
            root_key: root_key.clone(),
            root_cert: root_cert.clone(),
            root_cert_pem: String::from_utf8(root_cert.to_pem()?)?,
            vm_certificates: HashMap::new(),
            service_registry: Arc::clone(&service_registry),
            listeners: ListenerVec(vec![]),
        };

        let ca_state = Arc::new(Mutex::new(service_state));

        let service_impl = CAService { state: ca_state.clone() };

        tokio_runtime.spawn(process_root_cert_commands(ca_state, commands_receiver));

        let descriptor = <BpCertificateAuthority as ICertificateAuthority>::get_descriptor()
            .to_owned()
            + "/default";

        let service = BnCertificateAuthority::new_async_binder(
            service_impl,
            binder_tokio::TokioRuntime(tokio_runtime),
            features,
        );

        binder::add_service(&descriptor, service.as_binder())
            .with_context(|| format!("Failed to register {descriptor}"))?;

        Ok(())
    }
}

impl DisabledCAService {
    pub(crate) fn create_and_register<R>(
        binder_runtime: R,
        features: binder::BinderFeatures,
    ) -> anyhow::Result<()>
    where
        R: binder::binder_impl::BinderAsyncRuntime + Send + Sync + 'static,
    {
        let service_impl = DisabledCAService {};

        let descriptor = <BpCertificateAuthority as ICertificateAuthority>::get_descriptor()
            .to_owned()
            + "/default";

        let service =
            BnCertificateAuthority::new_async_binder(service_impl, binder_runtime, features);

        binder::add_service(&descriptor, service.as_binder())
            .with_context(|| format!("Failed to register {descriptor}"))?;

        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use anyhow::{anyhow, ensure, Context};
    use std::time::Duration;

    const TEST_TIMEOUT: Duration = Duration::from_secs(3);

    pub(crate) fn receive_command_add_root_cert(
        ca_command_receiver: &mut UnboundedReceiver<RootCertCommand>,
        expected_vm_name: &str,
        expected_root_cert: &[u8],
        runtime: &tokio::runtime::Runtime,
    ) -> anyhow::Result<()> {
        let ca_command = runtime
            .block_on(async {
                tokio::time::timeout(TEST_TIMEOUT, ca_command_receiver.recv()).await
            })
            .context("Timeout waiting for CA root cert command")?
            .ok_or(anyhow!("Failed to receive CA root cert command: channel closed"))?;

        match ca_command {
            RootCertCommand::Add { vm_name, root_cert } => {
                ensure!(
                    vm_name == expected_vm_name,
                    "Received RootCertCommand::Add for {vm_name} instead of {expected_vm_name}"
                );
                ensure!(
                    root_cert == expected_root_cert,
                    "Received RootCertCommand::Add with unexpected root_cert"
                );
            }
            RootCertCommand::Remove { vm_name, root_cert: _ } => bail!(
                "Received RootCertCommand::Remove({vm_name}) instead of Add({expected_vm_name})"
            ),
        }
        Ok(())
    }

    pub(crate) fn receive_command_remove_root_cert(
        ca_command_receiver: &mut UnboundedReceiver<RootCertCommand>,
        expected_vm_name: &str,
        runtime: &tokio::runtime::Runtime,
    ) -> anyhow::Result<()> {
        let ca_command = runtime
            .block_on(async {
                tokio::time::timeout(TEST_TIMEOUT, ca_command_receiver.recv()).await
            })
            .context("Timeout waiting for CA root cert command")?
            .ok_or(anyhow!("Failed to receive CA root cert command: channel closed"))?;

        match ca_command {
            RootCertCommand::Remove { vm_name, root_cert: _ } => {
                ensure!(
                    vm_name == expected_vm_name,
                    "Received RootCertCommand::Remove for {vm_name} instead of {expected_vm_name}"
                );
            }
            RootCertCommand::Add { vm_name, root_cert: _ } => bail!(
                "Received RootCertCommand::Add({vm_name}) instead of Remove({expected_vm_name})"
            ),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binder::{BinderFeatures, Strong};
    use crypto_rpc::cert::{
        build_certification_request, build_ec_private_key, build_root_certificate, verify_identity,
    };
    use google_sdv_ca::aidl::google::sdv::ca::IAuthoritiesListener::{
        BnAuthoritiesListener, MockIAuthoritiesListener,
    };
    use mockall::predicate;
    use std::sync::atomic::{AtomicU32, Ordering};

    // The largest FQIN possible. 255 human-readable chars (256 if you count a null terminator).
    const LARGEST_FQIN: &str = "vm-has-31-symbols-9012345678901:com.google.sdv.with_127_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234.ServiceWith_47_Symbols0123456789012345678901234/instance-has-47-symbols-90123456789012345678901";

    fn create_ca_service_for_tests(override_root_pem: Option<String>) -> anyhow::Result<CAService> {
        let root_key = PKey::from_ec_key(build_ec_private_key()?)?;
        let root_cert = build_root_certificate(&root_key.clone(), "foobar")?;
        let ca_service_state = CAServiceState {
            root_key: root_key.clone(),
            root_cert: root_cert.clone(),
            root_cert_pem: override_root_pem
                .unwrap_or_else(|| String::from_utf8(root_cert.to_pem().unwrap()).unwrap()),
            vm_certificates: HashMap::new(),
            service_registry: Arc::new(ServiceRegistry::default()),
            listeners: ListenerVec(vec![]),
        };
        Ok(CAService { state: Arc::new(Mutex::new(ca_service_state)) })
    }

    #[test]
    fn request_certification() -> anyhow::Result<()> {
        let service_ec_key = build_ec_private_key()?;
        let service_cert_request = build_certification_request(&service_ec_key, LARGEST_FQIN)?;

        let request = String::from_utf8(service_cert_request.to_pem()?)?;

        let ca_service = create_ca_service_for_tests(/*override_root_pem=*/ None)?;

        let response = ca_service
            .state
            .lock()
            .unwrap()
            .process_certificate_request(&request, ServiceFqin::try_from(LARGEST_FQIN)?)?;

        let cert_chain: Vec<X509> = X509::stack_from_pem(response.as_bytes())?;

        assert_eq!(cert_chain.len(), 2);

        // The leaf cert is at the top
        assert!(verify_identity(&cert_chain[0], LARGEST_FQIN)?);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn request_certification_permission_denied() -> anyhow::Result<()> {
        let service_ec_key = build_ec_private_key()?;
        let service_cert_request = build_certification_request(&service_ec_key, LARGEST_FQIN)?;

        let request = String::from_utf8(service_cert_request.to_pem()?)?;

        let ca_service = create_ca_service_for_tests(/*override_root_pem=*/ None)?;

        assert_eq!(
            ca_service.requestCertification(&request).await,
            Err(Status::from(StatusCode::PERMISSION_DENIED))
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn service_disabled() -> anyhow::Result<()> {
        let ca_service = DisabledCAService {};

        assert!(!ca_service.isEnabled().await?);

        assert!(ca_service.requestCertification("placeholder string").await.is_err());

        let mock_listener = MockIAuthoritiesListener::new();
        let mock_listener_service: Strong<dyn IAuthoritiesListener> =
            BnAuthoritiesListener::new_binder(mock_listener, BinderFeatures::default());

        assert!(ca_service.addAuthoritiesListener(&mock_listener_service).await.is_err());

        assert!(ca_service.removeAuthoritiesListener(&mock_listener_service).await.is_err());

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn add_authorities_listener() -> anyhow::Result<()> {
        let ca_service = create_ca_service_for_tests(/*override_root_pem=*/ None)?;

        let root_cert_pem = ca_service.state.lock().unwrap().root_cert_pem.clone();

        let mut mock_listener = MockIAuthoritiesListener::new();
        mock_listener
            .expect_onAuthoritiesChanged()
            .with(predicate::eq(root_cert_pem))
            .return_once(move |_| Ok(()));

        let mock_listener_service: Strong<dyn IAuthoritiesListener> =
            BnAuthoritiesListener::new_binder(mock_listener, BinderFeatures::default());

        ca_service.addAuthoritiesListener(&mock_listener_service).await?;

        Ok(())
    }

    #[test]
    fn get_vm_certificates_multi_vm() -> anyhow::Result<()> {
        let root_key = PKey::from_ec_key(build_ec_private_key()?)?;
        let mut ca_service_state = CAServiceState {
            root_key: root_key.clone(),
            root_cert: build_root_certificate(&root_key.clone(), "foobar")?,
            root_cert_pem: "self_cert_pem".to_owned(),
            vm_certificates: HashMap::new(),
            service_registry: Arc::new(ServiceRegistry::default()),
            listeners: ListenerVec(vec![]),
        };

        // Add some VM certificates
        ca_service_state
            .vm_certificates
            .insert("vm1".to_string(), RootCertEntry::new("cert1".to_string()));
        ca_service_state
            .vm_certificates
            .insert("vm2".to_string(), RootCertEntry::new("cert2".to_string()));

        let certs = ca_service_state.get_vm_certificates();

        // Since HashMap doesn't guarantee order, we need to check that all certificates are present
        assert!(certs.contains("self_cert_pem"));
        assert!(certs.contains("cert1"));
        assert!(certs.contains("cert2"));
        // Check length to make sure no extra characters are added.
        assert_eq!(certs.len(), "self_cert_pemcert1cert2".len());

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn root_cert_command_add() -> anyhow::Result<()> {
        let ca_service = create_ca_service_for_tests(/*override_root_pem=*/ None)?;

        let root_cert_pem = ca_service.state.lock().unwrap().root_cert_pem.clone();

        let peer_cert_pem = b"peer_vm_cert".to_vec();
        let peer_cert_pem_str = String::from_utf8(peer_cert_pem.clone())?;

        let mut seq = mockall::Sequence::new();
        let mut mock_listener = MockIAuthoritiesListener::new();
        // First onAuthoritiesChanged comes when you call addAuthoritiesListener.
        // It contains the current state.
        mock_listener
            .expect_onAuthoritiesChanged()
            .times(1)
            .in_sequence(&mut seq)
            .with(predicate::eq(root_cert_pem))
            .return_once(move |_| Ok(()));
        // The second onAuthoritiesChanged comes when the RootCertCommand::Add is processed.
        // It now contains the peer VM cert.
        mock_listener
            .expect_onAuthoritiesChanged()
            .times(1)
            .in_sequence(&mut seq)
            .with(predicate::function(move |x: &str| x.contains(peer_cert_pem_str.as_str())))
            .return_once(move |_| Ok(()));

        let mock_listener_service: Strong<dyn IAuthoritiesListener> =
            BnAuthoritiesListener::new_binder(mock_listener, BinderFeatures::default());

        ca_service.addAuthoritiesListener(&mock_listener_service).await?;

        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Add {
            vm_name: "peer_vm".to_string(),
            root_cert: peer_cert_pem.clone(),
        })?;

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn add_authorities_listener_failure_does_not_persist() -> anyhow::Result<()> {
        let ca_service = create_ca_service_for_tests(/*override_root_pem=*/ None)?;

        let mut mock_listener = MockIAuthoritiesListener::new();
        mock_listener
            .expect_onAuthoritiesChanged()
            .times(1)
            // Send back a failure. Should make the CA service discard this listener.
            .returning(|_| Err(Status::from(StatusCode::UNKNOWN_ERROR)));

        let mock_listener_service: Strong<dyn IAuthoritiesListener> =
            BnAuthoritiesListener::new_binder(mock_listener, BinderFeatures::default());

        ca_service
            .addAuthoritiesListener(&mock_listener_service)
            .await
            .expect_err("Adding listener expected to fail if the first notification failed");

        // mock_listener will not be notified about this new root cert given that the
        // first notification sent in addAuthoritiesListener has failed, causing CA service
        // to discard that listener.
        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Add {
            vm_name: "peer_vm".to_string(),
            root_cert: b"peer_vm_cert".to_vec(),
        })?;

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn root_cert_add_listener_drops_on_third_error() -> anyhow::Result<()> {
        let ca_service = create_ca_service_for_tests(/*override_root_pem=*/ None)?;

        let notification_count = Arc::new(AtomicU32::new(0));
        let notification_count_clone = notification_count.clone();
        let mut mock_listener = MockIAuthoritiesListener::new();

        mock_listener.expect_onAuthoritiesChanged().times(3).returning(move |_| {
            let count = notification_count_clone.fetch_add(1, Ordering::SeqCst);
            if count < 2 {
                Ok(())
            } else {
                Err(Status::from(StatusCode::DEAD_OBJECT))
            }
        });

        let mock_listener_service: Strong<dyn IAuthoritiesListener> =
            BnAuthoritiesListener::new_binder(mock_listener, BinderFeatures::default());

        // Notify first time with empty credentials
        ca_service.addAuthoritiesListener(&mock_listener_service).await?;

        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Add {
            vm_name: "instance2".to_string(),
            root_cert: b"instance2_cert".to_vec(),
        })?;

        // Cause a third notification which returns error from mocked listener.
        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Add {
            vm_name: "instance3".to_string(),
            root_cert: b"instance3_cert".to_vec(),
        })?;

        // Cause a fourth notification to check listener has been removed.
        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Add {
            vm_name: "instance4".to_string(),
            root_cert: b"instance4_cert".to_vec(),
        })?;

        // Check that the listener has been removed by checking length of listeners
        assert_eq!(ca_service.state.lock().unwrap().listeners.0.len(), 0);

        Ok(())
    }

    #[test]
    fn root_cert_command_remove() -> anyhow::Result<()> {
        let ca_service = create_ca_service_for_tests(
            /*override_root_pem=*/ Some("self_cert_pem".to_owned()),
        )?;
        let remote_vm_name = "test_vm".to_string();
        let remote_vm_root_cert = b"test_certificate".to_vec();
        ca_service.state.lock().unwrap().vm_certificates.insert(
            remote_vm_name.clone(),
            RootCertEntry::new(String::from_utf8(remote_vm_root_cert.clone())?),
        );

        let mut mock_listener = MockIAuthoritiesListener::new();
        mock_listener
            .expect_onAuthoritiesChanged()
            .with(predicate::eq("self_cert_pem".to_string()))
            .return_once(move |_| Ok(()));

        let mock_listener_service: Strong<dyn IAuthoritiesListener> =
            BnAuthoritiesListener::new_binder(mock_listener, BinderFeatures::default());
        ca_service.state.lock().unwrap().listeners.0.push(mock_listener_service);

        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Remove {
            vm_name: remote_vm_name.clone(),
            root_cert: remote_vm_root_cert.clone(),
        })?;

        // Key should be removed from the map.
        assert!(!ca_service.state.lock().unwrap().vm_certificates.contains_key(&remote_vm_name));

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn root_cert_command_2_add_1_remove_cert_stays() -> anyhow::Result<()> {
        let ca_service =
            create_ca_service_for_tests(/*override_root_pem=*/ Some("A".to_owned()))?;

        let mut mock_listener = MockIAuthoritiesListener::new();
        let mut seq = mockall::Sequence::new();
        mock_listener
            .expect_onAuthoritiesChanged()
            .times(1)
            .in_sequence(&mut seq)
            .with(predicate::eq("A".to_string()))
            .return_once(move |_| Ok(()));
        mock_listener
            .expect_onAuthoritiesChanged()
            .times(1)
            .in_sequence(&mut seq)
            .with(predicate::function(move |x: &str| {
                x.contains("A") && x.contains("B") && x.len() == 2
            }))
            .return_once(move |_| Ok(()));

        let mock_listener_service: Strong<dyn IAuthoritiesListener> =
            BnAuthoritiesListener::new_binder(mock_listener, BinderFeatures::default());

        ca_service.addAuthoritiesListener(&mock_listener_service).await?;

        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Add {
            vm_name: "peer_vm".to_string(),
            root_cert: b"B".to_vec(),
        })?;

        assert_eq!(
            ca_service
                .state
                .lock()
                .unwrap()
                .vm_certificates
                .get("peer_vm")
                .expect("root cert for peer_vm must still exist")
                .add_count,
            1
        );

        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Add {
            vm_name: "peer_vm".to_string(),
            root_cert: b"B".to_vec(),
        })?;

        assert_eq!(
            ca_service
                .state
                .lock()
                .unwrap()
                .vm_certificates
                .get("peer_vm")
                .expect("root cert for peer_vm must still exist")
                .add_count,
            2
        );

        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Remove {
            vm_name: "peer_vm".to_string(),
            root_cert: b"B".to_vec(),
        })?;

        assert_eq!(
            ca_service
                .state
                .lock()
                .unwrap()
                .vm_certificates
                .get("peer_vm")
                .expect("root cert for peer_vm must still exist")
                .add_count,
            1
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn root_cert_command_2_add_2_remove_cert_removed() -> anyhow::Result<()> {
        let ca_service =
            create_ca_service_for_tests(/*override_root_pem=*/ Some("A".to_owned()))?;

        let mut mock_listener = MockIAuthoritiesListener::new();
        let mut seq = mockall::Sequence::new();
        mock_listener
            .expect_onAuthoritiesChanged()
            .times(1)
            .in_sequence(&mut seq)
            .with(predicate::eq("A".to_string()))
            .return_once(move |_| Ok(()));
        mock_listener
            .expect_onAuthoritiesChanged()
            .times(1)
            .in_sequence(&mut seq)
            .with(predicate::function(move |x: &str| {
                x.contains("A") && x.contains("B") && x.len() == 2
            }))
            .return_once(move |_| Ok(()));
        mock_listener
            .expect_onAuthoritiesChanged()
            .times(1)
            .in_sequence(&mut seq)
            .with(predicate::eq("A".to_string()))
            .return_once(move |_| Ok(()));

        let mock_listener_service: Strong<dyn IAuthoritiesListener> =
            BnAuthoritiesListener::new_binder(mock_listener, BinderFeatures::default());

        ca_service.addAuthoritiesListener(&mock_listener_service).await?;

        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Add {
            vm_name: "peer_vm".to_string(),
            root_cert: b"B".to_vec(),
        })?;

        assert_eq!(
            ca_service
                .state
                .lock()
                .unwrap()
                .vm_certificates
                .get("peer_vm")
                .expect("root cert for peer_vm must still exist")
                .add_count,
            1
        );

        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Add {
            vm_name: "peer_vm".to_string(),
            root_cert: b"B".to_vec(),
        })?;

        assert_eq!(
            ca_service
                .state
                .lock()
                .unwrap()
                .vm_certificates
                .get("peer_vm")
                .expect("root cert for peer_vm must still exist")
                .add_count,
            2
        );

        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Remove {
            vm_name: "peer_vm".to_string(),
            root_cert: b"B".to_vec(),
        })?;

        assert_eq!(
            ca_service
                .state
                .lock()
                .unwrap()
                .vm_certificates
                .get("peer_vm")
                .expect("root cert for peer_vm must still exist")
                .add_count,
            1
        );

        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Remove {
            vm_name: "peer_vm".to_string(),
            root_cert: b"B".to_vec(),
        })?;

        assert!(!ca_service.state.lock().unwrap().vm_certificates.contains_key("peer_vm"));

        Ok(())
    }

    /// Tests the following sequence of RootCertCommands and corresponding expected results:
    ///
    /// Add("foo", b"A")
    ///    - vm_certificates["foo"] == RootCertEntry{ root_cert: b"A", add_count: 1 }
    ///    - Listeners are notified about the change
    /// Add("foo", b"B")
    ///    - vm_certificates["foo"] == RootCertEntry{ root_cert: b"B", add_count: 1 }
    ///    - Listeners are notified about the change
    /// Remove("foo", b"A")
    ///    - No changes in CAServiceState
    ///    - Listeners don't receive any notification.
    ///
    /// This can happen if VM "foo" is rebooted and gets back online before the removal
    /// command for its old instance is received by the CA service.
    #[tokio::test(flavor = "multi_thread")]
    async fn interleaved_add_remove_for_diff_root_certs_and_same_vm_name() -> anyhow::Result<()> {
        let ca_service = create_ca_service_for_tests(Some("X".to_owned()))?;

        let mut mock_listener = MockIAuthoritiesListener::new();
        let mut seq = mockall::Sequence::new();

        // 1. Initial state notification upon adding a listener.
        mock_listener
            .expect_onAuthoritiesChanged()
            .times(1)
            .in_sequence(&mut seq)
            .with(predicate::eq("X"))
            .return_once(|_| Ok(()));

        // 2. Notification after Add("foo", "A").
        mock_listener
            .expect_onAuthoritiesChanged()
            .times(1)
            .in_sequence(&mut seq)
            .with(predicate::function(|certs: &str| {
                certs.contains('X') && certs.contains('A') && (certs.len() == 2)
            }))
            .return_once(|_| Ok(()));

        // 3. Notification after Add("foo", "B"), which replaces "A".
        mock_listener
            .expect_onAuthoritiesChanged()
            .times(1)
            .in_sequence(&mut seq)
            .with(predicate::function(|certs: &str| {
                certs.contains('X') && certs.contains('B') && (certs.len() == 2)
            }))
            .return_once(|_| Ok(()));

        // No more notifications expected after this.

        let mock_listener_service: Strong<dyn IAuthoritiesListener> =
            BnAuthoritiesListener::new_binder(mock_listener, BinderFeatures::default());

        ca_service.addAuthoritiesListener(&mock_listener_service).await?;

        {
            let state = ca_service.state.lock().unwrap();
            assert!(state.vm_certificates.is_empty());
        }

        // Add("foo", "A")
        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Add {
            vm_name: "foo".to_string(),
            root_cert: b"A".to_vec(),
        })?;

        {
            let state = ca_service.state.lock().unwrap();
            let entry = state.vm_certificates.get("foo").unwrap();
            assert_eq!(entry.root_cert, "A");
            assert_eq!(entry.add_count, 1);
        }

        // Add("foo", "B")
        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Add {
            vm_name: "foo".to_string(),
            root_cert: b"B".to_vec(),
        })?;

        {
            let state = ca_service.state.lock().unwrap();
            let entry = state.vm_certificates.get("foo").unwrap();
            assert_eq!(entry.root_cert, "B");
            assert_eq!(entry.add_count, 1);
        }

        // Remove("foo", "A")
        ca_service.state.lock().unwrap().apply_root_cert_command(RootCertCommand::Remove {
            vm_name: "foo".to_string(),
            root_cert: b"A".to_vec(),
        })?;

        {
            let state = ca_service.state.lock().unwrap();
            let entry = state.vm_certificates.get("foo").unwrap();
            assert_eq!(entry.root_cert, "B");
            assert_eq!(entry.add_count, 1);
        }

        Ok(())
    }
}
