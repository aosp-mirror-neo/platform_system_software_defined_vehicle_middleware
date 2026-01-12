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

use std::pin::Pin;
use std::vec::Vec;

use openssl::pkey::{PKey, Private};
use openssl::ssl::select_next_proto;
use openssl::ssl::{AlpnError, SslAcceptor, SslConnector, SslMethod};
use openssl::x509::{store::X509Store, store::X509StoreBuilder, X509};
use tokio::io::{AsyncRead, AsyncWrite};

use crypto_rpc::cert::base32_x509_common_name_to_string;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};

use crate::id::ServiceFqin;

// A constant containing the HTTP/2 protocol identifier ("h2") in the
// wire format required for ALPN by OpenSSL. This consists of a
// single-byte length prefix (2) followed by the ASCII string "h2".
// The identifier "h2" is per HTTP/2 specification:
// https://datatracker.ietf.org/doc/html/rfc7540#section-3.4.
const ALPN_H2_WIRE: &[u8] = b"\x02h2";

#[derive(Clone)]
pub(crate) struct TlsData {
    all_root_certs: Vec<X509>, // all the CA certs in the system
    my_ca_cert: X509,          // the CA issued by this certificate provider
    my_cert: X509,
    my_private_key: PKey<Private>, // private key of the service
}

pub(crate) trait TlsProvider: Send {
    fn get_tls_data(&self) -> TlsData;
}

impl TlsData {
    pub(crate) fn new(
        all_root_certs: Vec<X509>,
        my_ca_cert: X509,
        my_cert: X509,
        my_private_key: PKey<Private>,
    ) -> Self {
        TlsData { all_root_certs, my_ca_cert, my_cert, my_private_key }
    }

    pub(crate) fn get_root_certs(&self) -> Vec<X509> {
        self.all_root_certs.clone()
    }

    pub(crate) fn get_my_cert(&self) -> X509 {
        self.my_cert.clone()
    }

    pub(crate) fn get_my_ca_cert(&self) -> X509 {
        self.my_ca_cert.clone()
    }

    pub(crate) fn get_my_private_key(&self) -> PKey<Private> {
        self.my_private_key.clone()
    }
}

fn create_unauthenticated_error(
    e: impl std::error::Error + Send + Sync + 'static,
    message: &str,
) -> SdvStatus {
    SdvStatus::with_cause_and_message(SdvStatusCode::Unauthenticated, e, message)
}

pub(crate) trait SecurityDataSetter {
    fn set_security_data(&self, builder: &mut openssl::ssl::SslContextBuilder) -> SdvResult<()>;
}

pub(crate) struct ProdSecurityDataSetter {
    tls_data: TlsData,
}

impl SecurityDataSetter for ProdSecurityDataSetter {
    fn set_security_data(&self, builder: &mut openssl::ssl::SslContextBuilder) -> SdvResult<()> {
        builder
            .set_private_key(&self.tls_data.get_my_private_key())
            .map_err(|e| create_unauthenticated_error(e, "Failed to set private key"))?;
        builder
            .set_certificate(&self.tls_data.get_my_cert())
            .map_err(|e| create_unauthenticated_error(e, "Failed to set a certificate"))?;
        builder
            .add_extra_chain_cert(self.tls_data.get_my_ca_cert().clone())
            .map_err(|e| create_unauthenticated_error(e, "Failed to add an extra chain cert"))?;
        builder
            .check_private_key()
            .map_err(|e| create_unauthenticated_error(e, "Failed private key check"))?;

        let cert_store = build_cert_store(&self.tls_data.get_root_certs())?;
        builder.set_cert_store(cert_store);

        Ok(())
    }
}

fn build_cert_store(ca_certs: &Vec<X509>) -> SdvResult<X509Store> {
    let mut cert_store = X509StoreBuilder::new()
        .map_err(|e| create_unauthenticated_error(e, "Failed to create cert store"))?;

    for ca_cert in ca_certs {
        cert_store
            .add_cert(ca_cert.clone())
            .map_err(|e| create_unauthenticated_error(e, "Failed to add certificate to store"))?;
    }
    let cert_store = cert_store.build();

    Ok(cert_store)
}

// Establishes client-side TLS connection on a provided stream using
// the provided Security Data Setter.
async fn init_ssl_stream_client_with_data_setter<S>(
    stream: S,
    security_data_setter: &impl SecurityDataSetter,
) -> SdvResult<tokio_openssl::SslStream<S>>
where
    S: AsyncRead + AsyncWrite + std::marker::Unpin,
{
    let mut connector = SslConnector::builder(SslMethod::tls()).unwrap();
    connector.set_verify(openssl::ssl::SslVerifyMode::PEER);
    security_data_setter.set_security_data(&mut connector)?;
    connector.set_alpn_protos(ALPN_H2_WIRE).unwrap();
    let connector = connector.build();

    let ssl = openssl::ssl::Ssl::new(&connector.into_context()).unwrap();
    let mut tokio_ssl = tokio_openssl::SslStream::new(ssl, stream).map_err(|e| {
        SdvStatus::with_cause_and_message(SdvStatusCode::Internal, e, "Failed to create SslStream")
    })?;
    Pin::new(&mut tokio_ssl).connect().await.map_err(|e| {
        SdvStatus::with_cause_and_message(SdvStatusCode::Internal, e, "TLS: failed to connect")
    })?;
    Ok(tokio_ssl)
}

// Establishes client-side TLS connection on a provided stream.
pub(crate) async fn init_ssl_stream_client<S>(
    stream: S,
    tls_data: TlsData,
) -> SdvResult<tokio_openssl::SslStream<S>>
where
    S: AsyncRead + AsyncWrite + std::marker::Unpin,
{
    init_ssl_stream_client_with_data_setter(stream, &ProdSecurityDataSetter { tls_data }).await
}

// Establishes server-side TLS connection on a provided stream using
// the provided Security Data Setter.
async fn init_ssl_stream_server_with_data_setter<S>(
    stream: S,
    security_data_setter: &impl SecurityDataSetter,
) -> std::result::Result<tokio_openssl::SslStream<S>, std::io::Error>
where
    S: AsyncRead + AsyncWrite + std::marker::Unpin,
{
    // Not sure which acceptor builder is suitable for our case.
    // mozilla_intermediate_v5 is described as a 'reasonable default choice' in the doc,
    // so let's keep it like this for now.
    let mut acceptor = SslAcceptor::mozilla_intermediate_v5(SslMethod::tls()).unwrap();
    acceptor.set_verify(openssl::ssl::SslVerifyMode::PEER);
    security_data_setter.set_security_data(&mut acceptor).map_err(|e| {
        log::warn!("Could not set security data: {e}");
        std::io::Error::new(std::io::ErrorKind::InvalidData, "Could not set security data")
    })?;

    acceptor.set_alpn_select_callback(|_ssl, alpn| {
        select_next_proto(ALPN_H2_WIRE, alpn).ok_or(AlpnError::NOACK)
    });

    let acceptor = acceptor.build();

    let ssl = openssl::ssl::Ssl::new(&acceptor.into_context()).unwrap();
    let mut tokio_ssl = tokio_openssl::SslStream::new(ssl, stream).map_err(|e| {
        log::warn!("Could not create SSL Stream");
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
    })?;

    // Initiates a server-side TLS handshake.
    Pin::new(&mut tokio_ssl).accept().await.map_err(|e| {
        log::warn!("TLS handshake failed");
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
    })?;
    Ok(tokio_ssl)
}

// Establishes server-side TLS connection on a provided stream.
pub(crate) async fn init_ssl_stream_server<S>(
    stream: S,
    tls_data: TlsData,
) -> std::result::Result<tokio_openssl::SslStream<S>, std::io::Error>
where
    S: AsyncRead + AsyncWrite + std::marker::Unpin,
{
    init_ssl_stream_server_with_data_setter(stream, &ProdSecurityDataSetter { tls_data }).await
}

// Verifies that the issuer of the peer certificate matches the VM specified in the FQIN.
pub(crate) fn verify_issuer_vm_matches_fqin(
    peer_cert: &X509,
    peer_fqin: &ServiceFqin,
) -> SdvResult<()> {
    let peer_vm_name = peer_fqin.get_sdv_vm_name();
    let issuer_name = peer_cert.issuer_name();
    let issuer_name = base32_x509_common_name_to_string(issuer_name).map_err(|err| {
        SdvStatus::with_message(
            sdv_status_rs::SdvStatusCode::Unauthenticated,
            format!("Could not extract issuer name from peer cert {err}"),
        )
    })?;

    if issuer_name != peer_vm_name {
        return Err(SdvStatus::with_message(
            SdvStatusCode::Unauthenticated,
            "Peer certificate issuer is not matching peer vm name",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::testing::fake_certificate_provider::{
        FakeCertificateAuthority, FakeTlsProvider,
    };
    use anyhow::Context;
    use crypto_rpc::cert::{
        build_ec_private_key, build_leaf_certificate, build_root_certificate, public_from_private,
        verify_identity,
    };
    use std::str::FromStr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    #[tokio::test]
    async fn test_tls_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_fqin_str = "vm:com.google.sdv.tls.server.TlsTest/default";
        let client_fqin_str = "vm:com.google.sdv.tls.client.TlsTest/default";

        let fake_ca = FakeCertificateAuthority::new("vm");
        let cert_provider_server = FakeTlsProvider::new(&fake_ca, server_fqin_str);
        let cert_provider_client = FakeTlsProvider::new(&fake_ca, client_fqin_str);

        // Spawn a task to accept connections
        let handle = tokio::spawn(async move {
            match listener.accept().await {
                Ok((stream, _)) => {
                    // Spawn a new task to handle each client connection
                    tokio::spawn(async move {
                        log::info!("Accepted connection from: {:?}", stream.peer_addr());
                        let stream =
                            init_ssl_stream_server(stream, cert_provider_server.get_tls_data())
                                .await;
                        assert!(
                            stream.is_ok(),
                            "Error creating server tls connection: {:?}",
                            stream.err()
                        );
                        let mut stream = stream.unwrap();

                        let peer_cert = stream.ssl().peer_certificate().unwrap();
                        assert!(verify_identity(&peer_cert, client_fqin_str).unwrap());

                        let mut buffer: [u8; 6] = [0; 6];

                        stream.read_exact(&mut buffer).await.expect("Read error");
                        assert_eq!(buffer, "Hello!".as_bytes());
                    })
                    .await
                    .unwrap();
                }
                Err(e) => {
                    println!("Error accepting connection: {e}");
                }
            }
        });

        let stream = TcpStream::connect(addr).await.unwrap();
        let stream = init_ssl_stream_client(stream, cert_provider_client.get_tls_data()).await;
        assert!(stream.is_ok(), "Error creating client tls connection: {:?}", stream.err());
        let mut stream = stream.unwrap();

        assert_eq!(stream.ssl().selected_alpn_protocol(), Some(&b"h2"[..]), "ALPN error");

        let peer_cert = stream.ssl().peer_certificate().unwrap();
        assert!(verify_identity(&peer_cert, server_fqin_str).unwrap());

        let message = "Hello!";
        stream.write_all(message.as_bytes()).await.unwrap();

        handle.await.unwrap();
    }

    #[test]
    fn test_verify_issuer_vm_matches_fqin_ok() {
        let fqin = ServiceFqin::from_str("vm:com.google.Service/default").unwrap();

        let fake_ca = FakeCertificateAuthority::new("vm");
        let tls_provider = FakeTlsProvider::new(&fake_ca, fqin.to_string().as_str());
        let peer_cert = tls_provider.get_tls_data().get_my_cert();

        let result = verify_issuer_vm_matches_fqin(&peer_cert, &fqin);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_issuer_vm_mismatch() {
        let fqin = ServiceFqin::from_str("vm:com.google.Service/default").unwrap();

        // Certificate is issued by a CA on a VM that is different from the VM of a service.
        let fake_ca = FakeCertificateAuthority::new("another-vm");
        let another_vm_tls_provider = FakeTlsProvider::new(&fake_ca, fqin.to_string().as_str());
        let another_vm_service_cert = another_vm_tls_provider.get_tls_data().get_my_cert();

        let result = verify_issuer_vm_matches_fqin(&another_vm_service_cert, &fqin);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), SdvStatusCode::Unauthenticated);
        assert!(err.to_string().contains("Peer certificate issuer is not matching peer vm name"));
    }

    enum SubjectNameEncoding {
        NoEncoding,
        Base32,
    }

    fn build_test_root_cert(
        subject_name_encoding: SubjectNameEncoding,
        subject_name: &str,
    ) -> X509 {
        let issuer = match subject_name_encoding {
            SubjectNameEncoding::Base32 => {
                crypto_rpc::cert::to_base32_x509_common_name(subject_name).unwrap()
            }
            SubjectNameEncoding::NoEncoding => {
                crypto_rpc::cert::to_x509_common_name(subject_name).unwrap()
            }
        };

        let subject = &issuer;
        let private_key =
            PKey::from_ec_key(crypto_rpc::cert::build_ec_private_key().unwrap()).unwrap();
        let public_key = crypto_rpc::cert::public_from_private(&private_key).unwrap();

        openssl::x509::X509Builder::new()
            .context("Failed creating an X509 cert_builder")
            .and_then(crypto_rpc::cert::set_v3)
            .and_then(crypto_rpc::cert::set_cert_sign_key_usage)
            .and_then(crypto_rpc::cert::set_indefinite_validity_period)
            .and_then(|cert_builder| {
                crypto_rpc::cert::set_subject_public_key(cert_builder, &public_key)
            })
            .and_then(|cert_builder| {
                crypto_rpc::cert::set_issuer_and_subject(cert_builder, &issuer, subject)
            })
            .and_then(|cert_builder| {
                crypto_rpc::cert::add_identity_as_subject_alt_name(
                    cert_builder,
                    subject_name,
                    /*No issuer for root cert*/ None,
                )
            })
            .and_then(|cert_builder| crypto_rpc::cert::sign_certificate(cert_builder, &private_key))
            .unwrap()
            .build()
    }

    #[test]
    fn test_verify_issuer_base32_encoding_checked() {
        let fqin = ServiceFqin::from_str("vm:com.google.Service/default").unwrap();

        // Check that Base32 encoded 'vm' works.
        let cert = build_test_root_cert(SubjectNameEncoding::Base32, "vm");
        let result = verify_issuer_vm_matches_fqin(&cert, &fqin);
        assert!(result.is_ok());

        // Now do the same, but don't encode the 'vm' as Base32.
        // This should fail.
        let cert = build_test_root_cert(SubjectNameEncoding::NoEncoding, "vm");
        let result = verify_issuer_vm_matches_fqin(&cert, &fqin);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), SdvStatusCode::Unauthenticated);
        assert!(err.to_string().contains("Could not extract issuer name from peer cert"));
    }

    #[test]
    fn test_create_security_data_error() {
        let error = SdvStatus::new(SdvStatusCode::AlreadyExists);
        let custom_message = "My custom error message";

        let sdv_status = create_unauthenticated_error(error, custom_message);
        assert_eq!(sdv_status.status_code(), SdvStatusCode::Unauthenticated);
        assert!(sdv_status.to_string().contains(custom_message));
    }

    struct TestSecurityDataSetter {
        all_root_certs: Vec<X509>,     // all the CA certs in the system
        my_cert_chain: Vec<X509>,      // certificate chain from leaf to root
        my_private_key: PKey<Private>, // private key of the service
    }

    impl SecurityDataSetter for TestSecurityDataSetter {
        fn set_security_data(
            &self,
            builder: &mut openssl::ssl::SslContextBuilder,
        ) -> SdvResult<()> {
            builder
                .set_private_key(&self.my_private_key)
                .map_err(|e| create_unauthenticated_error(e, "Failed to set private key"))?;
            builder
                .set_certificate(&self.my_cert_chain[0])
                .map_err(|e| create_unauthenticated_error(e, "Failed to set a certificate"))?;
            for cert in &self.my_cert_chain[1..] {
                builder.add_extra_chain_cert(cert.clone()).map_err(|e| {
                    create_unauthenticated_error(e, "Failed to add an extra chain cert")
                })?;
            }
            builder
                .check_private_key()
                .map_err(|e| create_unauthenticated_error(e, "Failed private key check"))?;

            let cert_store = build_cert_store(&self.all_root_certs)?;
            builder.set_cert_store(cert_store);

            Ok(())
        }
    }

    /// Initializes and handles a TLS server connection, reading a message.
    async fn run_server_task(listener: TcpListener, server_tls_setter: TestSecurityDataSetter) {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(async move {
                    log::info!("Accepted connection from: {:?}", stream.peer_addr());
                    let stream_result =
                        init_ssl_stream_server_with_data_setter(stream, &server_tls_setter).await;
                    assert!(
                        stream_result.is_ok(),
                        "Error creating server tls connection: {:?}",
                        stream_result.err()
                    );
                    let mut stream = stream_result.unwrap();

                    let mut buffer: [u8; 6] = [0; 6];
                    stream.read_exact(&mut buffer).await.expect("Read error");
                    assert_eq!(buffer, "Hello!".as_bytes());
                })
                .await
                .unwrap();
            }
            Err(e) => {
                println!("Error accepting connection: {e}");
            }
        }
    }

    /// Initializes and handles a TLS client connection, sending a message.
    async fn run_client_task(
        addr: std::net::SocketAddr,
        client_tls_setter: TestSecurityDataSetter,
    ) -> anyhow::Result<()> {
        let stream = TcpStream::connect(addr).await?;
        let stream_result =
            init_ssl_stream_client_with_data_setter(stream, &client_tls_setter).await;
        assert!(
            stream_result.is_ok(),
            "Error creating client tls connection: {:?}",
            stream_result.err()
        );
        let mut stream = stream_result.unwrap();

        let message = "Hello!";
        stream.write_all(message.as_bytes()).await?;
        Ok(())
    }

    // Tests that TLS connection succeeds also if a root certificate is not specified
    // in a certificate chain explicitly.
    #[tokio::test]
    async fn test_cert_chain_without_root_cert() {
        let server_fqin_str = "vm:com.google.sdv.tls.server.TlsTest/default";
        let client_fqin_str = "vm:com.google.sdv.tls.client.TlsTest/default";

        let ca_private_key = PKey::from_ec_key(build_ec_private_key().unwrap()).unwrap();
        let ca_cert = build_root_certificate(&ca_private_key, "vm").unwrap();
        let all_ca_certs = vec![ca_cert.clone()];
        let client_private_key: PKey<Private> = build_ec_private_key().unwrap().try_into().unwrap();
        let client_public_key = public_from_private(&client_private_key).unwrap();

        let client_cert =
            build_leaf_certificate(&ca_private_key, &ca_cert, &client_public_key, client_fqin_str)
                .unwrap();

        let server_private_key: PKey<Private> = build_ec_private_key().unwrap().try_into().unwrap();
        let server_public_key = public_from_private(&server_private_key).unwrap();

        let server_cert =
            build_leaf_certificate(&ca_private_key, &ca_cert, &server_public_key, server_fqin_str)
                .unwrap();

        let server_tls_setter = TestSecurityDataSetter {
            all_root_certs: all_ca_certs.clone(),
            my_cert_chain: vec![server_cert.clone()],
            my_private_key: server_private_key,
        };
        let client_tls_setter = TestSecurityDataSetter {
            all_root_certs: all_ca_certs.clone(),
            my_cert_chain: vec![client_cert.clone()],
            my_private_key: client_private_key,
        };

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server_handle = tokio::spawn(async move {
            run_server_task(listener, server_tls_setter).await;
        });

        let client_handle_result = run_client_task(addr, client_tls_setter).await;
        assert!(
            client_handle_result.is_ok(),
            "Client task failed: {:?}",
            client_handle_result.err()
        );

        server_handle.await.unwrap();
    }
}
