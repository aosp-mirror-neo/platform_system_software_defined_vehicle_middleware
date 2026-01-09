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
use crate::rpc::tls::TlsData;
use crate::rpc::tls::TlsProvider;

use google_sdv_ca::aidl::google::sdv::ca::IAuthoritiesListener::{
    BnAuthoritiesListener, IAuthoritiesListener, IAuthoritiesListenerAsyncServer,
};
use google_sdv_ca::aidl::google::sdv::ca::ICertificateAuthority::{
    BpCertificateAuthority, ICertificateAuthority, ICertificateAuthorityAsync,
};

use async_trait::async_trait;
use binder::Strong;
use binder_tokio::Tokio;
use binder_tokio::TokioRuntime;
use crypto_rpc::cert::build_certification_request;
use crypto_rpc::cert::build_ec_private_key;
use crypto_rpc::cert::csr_to_pem;
use crypto_rpc::cert::pems_to_x509;
use openssl::ec::EcKey;
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::sync::Arc;
use std::sync::Mutex;
use tokio::runtime::Handle;
use tokio::sync::oneshot;

async fn get_certificate_authority() -> SdvResult<binder::Strong<dyn ICertificateAuthority>> {
    let descriptor =
        <BpCertificateAuthority as ICertificateAuthority>::get_descriptor().to_owned() + "/default";
    let binder_service = binder_tokio::wait_for_interface::<dyn ICertificateAuthority>(&descriptor);
    let certificate_authority = binder_service.await?;
    Ok(certificate_authority)
}

pub(crate) struct AuthoritiesListener {
    all_root_certs: Arc<Mutex<Vec<X509>>>,
    init_complete: Mutex<Option<oneshot::Sender<()>>>,
}

fn anyhow_to_sdv_status(err: anyhow::Error) -> SdvStatus {
    SdvStatus::with_message(sdv_status_rs::SdvStatusCode::Internal, err.to_string())
}

impl binder::Interface for AuthoritiesListener {}

#[allow(non_snake_case)] // AIDL generated
#[async_trait]
impl IAuthoritiesListenerAsyncServer for AuthoritiesListener {
    async fn onAuthoritiesChanged(&self, authoritiesListPem: &str) -> binder::Result<()> {
        log::info!("New root certs received");
        let new_root_certs = pems_to_x509(authoritiesListPem).map_err(|e| {
            binder::Status::new_service_specific_error_str(
                SdvStatusCode::Internal as i32,
                Some(e.to_string()),
            )
        })?;

        if new_root_certs.is_empty() {
            log::warn!("Root certs are empty");
        }

        *(self.all_root_certs.lock().unwrap()) = new_root_certs;

        if let Some(tx) = self.init_complete.lock().unwrap().take() {
            let _ = tx.send(());
        }

        Ok(())
    }
}

impl AuthoritiesListener {
    pub fn new(
        all_root_certs: Arc<Mutex<Vec<X509>>>,
        init_complete_sender: oneshot::Sender<()>,
    ) -> AuthoritiesListener {
        AuthoritiesListener {
            all_root_certs,
            init_complete: Mutex::new(Some(init_complete_sender)),
        }
    }
}

struct Certs {
    pub(crate) my_cert: X509,
    pub(crate) my_ca_cert: X509,
}

async fn obtain_certs(
    cert_authority_async: &binder::Strong<dyn ICertificateAuthorityAsync<Tokio>>,
    ec_private_key: &EcKey<Private>,
    fqin: &str,
) -> SdvResult<Certs> {
    let cert_signing_request =
        build_certification_request(ec_private_key, fqin).map_err(anyhow_to_sdv_status)?;
    let cert_signing_request = csr_to_pem(&cert_signing_request).map_err(anyhow_to_sdv_status)?;

    let cert_chain_pem = cert_authority_async.requestCertification(&cert_signing_request).await?;
    let cert_chain: Vec<X509> = pems_to_x509(&cert_chain_pem).map_err(anyhow_to_sdv_status)?;

    if cert_chain.len() != 2 {
        return Err(SdvStatus::with_message(
            SdvStatusCode::Internal,
            "There should be exactly 2 certs in the chain: the cert of the service & root cert",
        ));
    }

    // The leaf cert is at the top
    let my_cert = cert_chain[0].clone();
    let my_ca_cert = cert_chain[1].clone();

    SdvResult::Ok(Certs { my_cert, my_ca_cert })
}

// Prod TLS Provider creates instances of TlsData that reflects
// the state of the system at the moment of creation. In particular
// the root certificates in TlsData correspond to the VMs in mesh.
// Don't make it 'clone' as it will cause problems with listener
// subscription/unsubscription.
pub struct ProdTlsProvider {
    private_key: PKey<Private>,
    certs: Certs,
    #[allow(dead_code)] // this variable should be kept alive to receive updates on root certs
    authorities_listener: Strong<dyn IAuthoritiesListener>,
    cert_authority: Strong<dyn ICertificateAuthority>,
    is_subscribed: bool,
    // Authorities listener (authorities_listener) and Prod TLS Provider
    // (this struct) share vector of root certs (all_root_certs).
    // As soon as authorities listener receives updates, the value of this
    // field will be updated and will be used to create new
    // instances of TlsData in get_tls_data().
    all_root_certs: Arc<Mutex<Vec<X509>>>,
}

impl ProdTlsProvider {
    pub async fn new(fqin: &str) -> SdvResult<Option<ProdTlsProvider>> {
        let cert_authority = get_certificate_authority().await?;
        Self::new_impl(fqin, cert_authority).await
    }

    async fn new_impl(
        fqin: &str,
        cert_authority: Strong<dyn ICertificateAuthority>,
    ) -> SdvResult<Option<ProdTlsProvider>> {
        // The sync version of the interface preserved to be used in the destructor
        let cert_authority_async: binder::Strong<dyn ICertificateAuthorityAsync<Tokio>> =
            cert_authority.clone().into_async();

        if !cert_authority_async.isEnabled().await? {
            log::info!("TLS is disabled");
            return Ok(None);
        }

        log::info!("TLS is enabled");

        let my_ec_private_key = build_ec_private_key().map_err(anyhow_to_sdv_status)?;
        let my_certs = obtain_certs(&cert_authority_async, &my_ec_private_key, fqin).await?;

        let my_private_key: PKey<Private> = my_ec_private_key.try_into().map_err(|_e| {
            SdvStatus::with_message(SdvStatusCode::Internal, "Could not create a private key")
        })?;

        let all_root_certs: Arc<Mutex<Vec<X509>>> = Arc::new(Mutex::new(Vec::new()));
        // Signal to fetch root cert
        let (init_complete_tx, init_complete_rx) = oneshot::channel();
        binder::ProcessState::start_thread_pool();
        let authorities_listener = BnAuthoritiesListener::new_async_binder(
            AuthoritiesListener::new(all_root_certs.clone(), init_complete_tx),
            TokioRuntime(Handle::current()),
            binder::BinderFeatures::default(),
        );

        // Immediately create TlsProvider to guarantee unsubscription in drop().
        let mut tls_provider = ProdTlsProvider {
            private_key: my_private_key,
            certs: my_certs,
            authorities_listener,
            cert_authority,
            is_subscribed: false,
            all_root_certs: all_root_certs.clone(),
        };
        tls_provider.subscribe_root_certs(&cert_authority_async).await?;

        // Wait for the initial population of all root certs to complete.
        init_complete_rx.await.map_err(|_| {
            SdvStatus::with_message(
                SdvStatusCode::Internal,
                "Failed to receive initial root certificates",
            )
        })?;
        {
            // TLS can't function without root certs.
            let all_root_certs = all_root_certs.lock().unwrap();
            if all_root_certs.is_empty() {
                return Err(SdvStatus::with_message(
                    SdvStatusCode::Internal,
                    "Root certs are empty or failed to be fetched",
                ));
            }
        }

        Ok(Some(tls_provider))
    }

    async fn subscribe_root_certs(
        &mut self,
        cert_authority_async: &binder::Strong<dyn ICertificateAuthorityAsync<Tokio>>,
    ) -> SdvResult<()> {
        // As a result of this call initial root certs should be fetched.
        cert_authority_async.addAuthoritiesListener(&self.authorities_listener).await?;
        self.is_subscribed = true;
        SdvResult::Ok(())
    }
}

impl TlsProvider for ProdTlsProvider {
    fn get_tls_data(&self) -> TlsData {
        TlsData::new(
            self.all_root_certs.lock().unwrap().clone(),
            self.certs.my_ca_cert.clone(),
            self.certs.my_cert.clone(),
            self.private_key.clone(),
        )
    }
}

impl Drop for ProdTlsProvider {
    fn drop(&mut self) {
        if !self.is_subscribed {
            return;
        }

        let status = self.cert_authority.removeAuthoritiesListener(&self.authorities_listener);
        if status.is_err() {
            log::error!("Error while unregistering listener: {status:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::rpc::certificate_provider::ProdTlsProvider;
    use binder::BinderFeatures;
    use binder::Status;
    use crypto_rpc::cert::build_ec_private_key;
    use crypto_rpc::cert::build_leaf_certificate;
    use crypto_rpc::cert::build_root_certificate;
    use google_sdv_ca::aidl::google::sdv::ca::ICertificateAuthority::{
        BnCertificateAuthority, MockICertificateAuthority,
    };
    use openssl::pkey::PKey;
    use openssl::x509::X509Req;

    const SERVICE_FQIN: &str = "instance1:com.google.sdv.Service/default";
    const VM_NAME: &str = "vm1";

    // This function is similar to what SD does while processing a CSR.
    fn process_csr(request: &str) -> Result<String, Status> {
        let x509_req = X509Req::from_pem(request.as_bytes()).unwrap();

        let private_key = PKey::from_ec_key(build_ec_private_key().unwrap()).unwrap();
        let root_cert = build_root_certificate(&private_key, VM_NAME).unwrap();

        let service_public_key = x509_req.public_key().unwrap();
        let service_fqin = SERVICE_FQIN.to_string();
        let leaf_cert =
            build_leaf_certificate(&private_key, &root_cert, &service_public_key, &service_fqin)
                .unwrap();

        let leaf_cert_pem = String::from_utf8(leaf_cert.to_pem().unwrap()).unwrap();
        let root_cert_pem = String::from_utf8(root_cert.to_pem().unwrap()).unwrap();

        Ok(leaf_cert_pem + "\n" + &root_cert_pem)
    }

    fn setup_mock_for_unsubscribe() -> MockICertificateAuthority {
        let mut mock_cert_authority = MockICertificateAuthority::new();
        mock_cert_authority.expect_isEnabled().returning(|| Ok(true));

        mock_cert_authority
            .expect_requestCertification()
            .returning(|request: &str| process_csr(request));

        mock_cert_authority.expect_removeAuthoritiesListener().times(1).returning(|_| Ok(()));
        mock_cert_authority
    }

    // Even if the initial response after subscription to root cert contained error
    // (e.g. no root certs), the listener should unsubscribe.
    #[tokio::test]
    async fn remove_listener_error() -> anyhow::Result<()> {
        let mut mock_cert_authority = setup_mock_for_unsubscribe();
        mock_cert_authority.expect_addAuthoritiesListener().times(1).returning(|listener| {
            // Return empty root cert list on subscription. This is an error case
            // and the listener should be destroyed and unsubscribe.
            listener.onAuthoritiesChanged("")?;
            Ok(())
        });
        let cert_authority_strong =
            BnCertificateAuthority::new_binder(mock_cert_authority, BinderFeatures::default());

        let provider = ProdTlsProvider::new_impl(SERVICE_FQIN, cert_authority_strong).await;

        assert!(provider.is_err());

        Ok(())
    }

    // Happy path: unsubscribe in drop() should take place.
    #[tokio::test]
    async fn remove_listener_ok() -> anyhow::Result<()> {
        let mut mock_cert_authority = setup_mock_for_unsubscribe();

        mock_cert_authority.expect_addAuthoritiesListener().times(1).returning(|listener| {
            let private_key = PKey::from_ec_key(build_ec_private_key().unwrap()).unwrap();
            let root_cert = build_root_certificate(&private_key, VM_NAME).unwrap();
            let root_cert_pem = String::from_utf8(root_cert.to_pem().unwrap()).unwrap();
            listener.onAuthoritiesChanged(&root_cert_pem)?;
            Ok(())
        });

        let cert_authority_strong =
            BnCertificateAuthority::new_binder(mock_cert_authority, BinderFeatures::default());

        let provider =
            ProdTlsProvider::new_impl(SERVICE_FQIN, cert_authority_strong).await?.unwrap();

        // drop() implementation should call removeAuthoritiesListener().
        drop(provider);

        Ok(())
    }
}
