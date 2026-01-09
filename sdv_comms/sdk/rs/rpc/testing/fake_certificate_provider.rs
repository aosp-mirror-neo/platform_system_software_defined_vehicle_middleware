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

#![cfg(test)]

use crate::rpc::tls::TlsData;
use crate::rpc::tls::TlsProvider;

use crypto_rpc::cert::build_ec_private_key;
use crypto_rpc::cert::build_leaf_certificate;
use crypto_rpc::cert::build_root_certificate;
use crypto_rpc::cert::public_from_private;

use openssl::pkey::{PKey, Private, Public};
use openssl::x509::X509;

#[derive(Clone)]
pub(crate) struct FakeTlsProvider {
    all_ca_certs: Vec<X509>, // all the CA certs in the system.
    my_ca_cert: X509,        // the local CA certificate.
    my_cert: X509,
    my_private_key: PKey<Private>, // private key of the service.
}

// We don't process errors within this fake testing certificate provider as
// basically everything here is hard coded. If for some reason there will
// be an error, let's just let it die.
impl FakeTlsProvider {
    pub(crate) fn new(ca: &FakeCertificateAuthority, fqin: &str) -> FakeTlsProvider {
        let my_private_key: PKey<Private> = build_ec_private_key().unwrap().try_into().unwrap();
        let my_public_key = public_from_private(&my_private_key).unwrap();

        let cert_chain = ca.request_certification(&my_public_key, fqin);
        assert_eq!(cert_chain.len(), 2, "Expected service & root certs in chain");
        // The leaf cert is at the top.
        let my_cert = cert_chain[0].clone();
        let my_ca_cert = cert_chain[1].clone();
        let all_ca_certs = ca.get_root_certs();

        FakeTlsProvider { all_ca_certs, my_ca_cert, my_cert, my_private_key }
    }
}

impl TlsProvider for FakeTlsProvider {
    fn get_tls_data(&self) -> TlsData {
        TlsData::new(
            self.all_ca_certs.clone(),
            self.my_ca_cert.clone(),
            self.my_cert.clone(),
            self.my_private_key.clone(),
        )
    }
}

pub(crate) struct FakeCertificateAuthority {
    cert: X509,                 // Certificate of this CA.
    private_key: PKey<Private>, // Private key of this CA.
    all_ca_certs: Vec<X509>,    // All the CA certs in the system.
}

// This is a simplified version of a ICertificateAuthority interface used in prod.
impl FakeCertificateAuthority {
    pub(crate) fn new(ca_name: &str) -> FakeCertificateAuthority {
        let private_key = PKey::from_ec_key(build_ec_private_key().unwrap()).unwrap();
        let cert = build_root_certificate(&private_key, ca_name).unwrap();
        let all_ca_certs = vec![cert.clone()];
        FakeCertificateAuthority { cert, private_key, all_ca_certs }
    }

    fn request_certification(&self, public_key: &PKey<Public>, fqin: &str) -> Vec<X509> {
        let leaf_cert =
            build_leaf_certificate(&self.private_key, &self.cert, public_key, fqin).unwrap();
        let root_cert = self.cert.clone();
        vec![leaf_cert, root_cert]
    }

    fn get_root_certs(&self) -> Vec<X509> {
        self.all_ca_certs.clone()
    }
}
