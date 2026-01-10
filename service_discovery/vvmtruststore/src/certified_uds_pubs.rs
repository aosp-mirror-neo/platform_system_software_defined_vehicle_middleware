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

//! A struct that stores the UDS public keys that are cerifified by UDS certificate chains.

use crate::CertificateRevocationList;
use hwtrust::publickey::PublicKey;
use log::warn;
use uds_certs::UdsCertificates;

/// UDS (Unique Device Secret) Public Keys that are certified by UDS certificate chains.
pub struct CertifiedUdsPubs {
    certified_uds_pubs: Vec<PublicKey>,
}

impl CertifiedUdsPubs {
    /// Constructs a list of UDS public keys that have a valid UDS certificate chain stemming from
    /// a trusted UDS root CA.
    pub fn from_uds_certs(
        uds_certs: &UdsCertificates,
        trusted_uds_root_ca_pub: &PublicKey,
        crl: &CertificateRevocationList,
    ) -> anyhow::Result<Self> {
        let mut certified_uds_pubs = vec![];

        for uds_cert_chain in uds_certs.certificate_chains() {
            let root_ca_pub: PublicKey = uds_cert_chain.root_ca_pub()?;
            if &root_ca_pub != trusted_uds_root_ca_pub {
                warn!("UDS certificate chain skipped due to untrusted root CA");
                continue;
            }
            let uds_pub = uds_cert_chain.uds_pub()?;
            if certified_uds_pubs.contains(&uds_pub) {
                warn!("UDS public key is already certified");
                continue;
            }
            if crl.is_revoked_chain(uds_cert_chain.chain())? {
                warn!("UDS certificate chain contains a revoked public key");
                continue;
            }
            certified_uds_pubs.push(uds_pub)
        }
        Ok(CertifiedUdsPubs { certified_uds_pubs })
    }

    /// Returns whether the provided UDS public key is certified by an UDS certificate chain.
    pub fn is_certified(&self, uds_pub: &PublicKey) -> bool {
        self.certified_uds_pubs.contains(uds_pub)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use crypto_rpc::cert::{build_ec_private_key, public_from_private};
    use diced_open_dice::Cdi;
    use openssl::pkey::{PKey, Private};
    use sample_dice_handover::create_random_uds;
    use test_uds_certs_generator::create_x509_uds_cert_chain_from_pkey_secrets;
    use uds_certs::UdsCertChain;

    fn random_private_key() -> anyhow::Result<PKey<Private>> {
        PKey::from_ec_key(build_ec_private_key()?).context("Failed to generate test private key")
    }

    fn random_public_key() -> anyhow::Result<PublicKey> {
        let public_key: PublicKey = public_from_private(&random_private_key()?)?.try_into()?;
        Ok(public_key)
    }

    fn valid_random_uds_cert_chain() -> anyhow::Result<UdsCertChain> {
        let ca_private_keys =
            vec![random_private_key()?, random_private_key()?, random_private_key()?];
        UdsCertChain::try_from(create_x509_uds_cert_chain_from_pkey_secrets(
            &ca_private_keys,
            &create_random_uds(),
        )?)
    }

    fn valid_random_uds_cert_chain_for_uds(uds: &Cdi) -> anyhow::Result<UdsCertChain> {
        let ca_private_keys =
            vec![random_private_key()?, random_private_key()?, random_private_key()?];
        UdsCertChain::try_from(create_x509_uds_cert_chain_from_pkey_secrets(&ca_private_keys, uds)?)
    }

    fn valid_random_uds_cert_chain_for_uds_from_root(
        uds: &Cdi,
        root_private_key: &PKey<Private>,
    ) -> anyhow::Result<UdsCertChain> {
        let ca_private_keys =
            vec![root_private_key.clone(), random_private_key()?, random_private_key()?];
        UdsCertChain::try_from(create_x509_uds_cert_chain_from_pkey_secrets(&ca_private_keys, uds)?)
    }

    #[test]
    fn uds_pub_with_valid_chain_is_certified() -> anyhow::Result<()> {
        let uds_cert_chain = valid_random_uds_cert_chain()?;
        let root_ca_pub = uds_cert_chain.root_ca_pub()?;
        let uds_pub = uds_cert_chain.uds_pub()?;
        let uds_certs = UdsCertificates::from(vec![uds_cert_chain]);

        let certified_uds_pubs = CertifiedUdsPubs::from_uds_certs(
            &uds_certs,
            &root_ca_pub,
            &CertificateRevocationList::default(),
        )?;

        assert!(certified_uds_pubs.is_certified(&uds_pub));
        Ok(())
    }

    #[test]
    fn uds_pub_with_certified_by_unknown_root_ca_is_not_certified() -> anyhow::Result<()> {
        let root_ca_pub = random_public_key()?;
        let uds_cert_chain_with_unknown_root_ca = valid_random_uds_cert_chain()?;
        let uds_pub = uds_cert_chain_with_unknown_root_ca.uds_pub()?;

        let uds_certs_with_unknown_root_ca =
            UdsCertificates::from(vec![uds_cert_chain_with_unknown_root_ca]);

        let certified_uds_pubs = CertifiedUdsPubs::from_uds_certs(
            &uds_certs_with_unknown_root_ca,
            &root_ca_pub,
            &CertificateRevocationList::default(),
        )?;

        assert!(!certified_uds_pubs.is_certified(&uds_pub));
        Ok(())
    }

    #[test]
    fn uds_pub_without_uds_cert_chain_is_not_certified() -> anyhow::Result<()> {
        let uds_cert_chain = valid_random_uds_cert_chain()?;
        let root_ca_pub = uds_cert_chain.root_ca_pub()?;
        let uds_certs = UdsCertificates::from(vec![uds_cert_chain]);
        let certified_uds_pubs = CertifiedUdsPubs::from_uds_certs(
            &uds_certs,
            &root_ca_pub,
            &CertificateRevocationList::default(),
        )?;

        let uds_pub_not_in_cert_chain = random_public_key()?;
        assert!(!certified_uds_pubs.is_certified(&uds_pub_not_in_cert_chain));
        Ok(())
    }

    #[test]
    fn uds_pub_certified_by_both_unknown_and_known_ca_is_certified() -> anyhow::Result<()> {
        let uds = create_random_uds();
        let uds_cert_chain = valid_random_uds_cert_chain_for_uds(&uds)?;
        let root_ca_pub = uds_cert_chain.root_ca_pub()?;
        let uds_pub = uds_cert_chain.uds_pub()?;
        let uds_cert_chain_unknown_ca = valid_random_uds_cert_chain_for_uds(&uds)?;

        // Known first in list
        {
            let uds_certs = UdsCertificates::from(vec![
                uds_cert_chain.clone(),
                uds_cert_chain_unknown_ca.clone(),
            ]);

            let certified_uds_pubs = CertifiedUdsPubs::from_uds_certs(
                &uds_certs,
                &root_ca_pub,
                &CertificateRevocationList::default(),
            )?;

            assert!(certified_uds_pubs.is_certified(&uds_pub));
        }

        // Unknown first in list
        {
            let uds_certs = UdsCertificates::from(vec![
                uds_cert_chain_unknown_ca.clone(),
                uds_cert_chain.clone(),
            ]);

            let certified_uds_pubs = CertifiedUdsPubs::from_uds_certs(
                &uds_certs,
                &root_ca_pub,
                &CertificateRevocationList::default(),
            )?;

            assert!(certified_uds_pubs.is_certified(&uds_pub));
        }

        Ok(())
    }

    #[test]
    fn uds_pub_certified_by_two_chains_is_certified() -> anyhow::Result<()> {
        let uds = create_random_uds();
        let root_ca_private = random_private_key()?;
        let uds_cert_chain = valid_random_uds_cert_chain_for_uds_from_root(&uds, &root_ca_private)?;
        let root_ca_pub = uds_cert_chain.root_ca_pub()?;
        let uds_pub = uds_cert_chain.uds_pub()?;
        let uds_cert_chain_2 =
            valid_random_uds_cert_chain_for_uds_from_root(&uds, &root_ca_private)?;

        let uds_certs = UdsCertificates::from(vec![uds_cert_chain, uds_cert_chain_2]);

        let certified_uds_pubs = CertifiedUdsPubs::from_uds_certs(
            &uds_certs,
            &root_ca_pub,
            &CertificateRevocationList::default(),
        )?;

        assert!(certified_uds_pubs.is_certified(&uds_pub));
        assert_eq!(certified_uds_pubs.certified_uds_pubs.len(), 1);

        Ok(())
    }

    #[test]
    fn certified_uds_pub_certifies_multiple_different_uds_pubs() -> anyhow::Result<()> {
        let uds_1 = create_random_uds();
        let root_ca_private = random_private_key()?;
        let uds_1_cert_chain =
            valid_random_uds_cert_chain_for_uds_from_root(&uds_1, &root_ca_private)?;
        let root_ca_pub = uds_1_cert_chain.root_ca_pub()?;
        let uds_1_pub = uds_1_cert_chain.uds_pub()?;

        let uds_2 = create_random_uds();
        let uds_2_cert_chain =
            valid_random_uds_cert_chain_for_uds_from_root(&uds_2, &root_ca_private)?;
        let uds_2_pub = uds_2_cert_chain.uds_pub()?;

        let uds_certs = UdsCertificates::from(vec![uds_1_cert_chain, uds_2_cert_chain]);
        let certified_uds_pubs = CertifiedUdsPubs::from_uds_certs(
            &uds_certs,
            &root_ca_pub,
            &CertificateRevocationList::default(),
        )?;

        assert!(certified_uds_pubs.is_certified(&uds_1_pub));
        assert!(certified_uds_pubs.is_certified(&uds_2_pub));

        Ok(())
    }

    #[test]
    fn uds_pub_with_valid_chain_revoked() -> anyhow::Result<()> {
        let uds_cert_chain = valid_random_uds_cert_chain()?;
        let root_ca_pub = uds_cert_chain.root_ca_pub()?;
        let uds_pub = uds_cert_chain.uds_pub()?;

        let mut crl = CertificateRevocationList::new();
        let revoked_cert = uds_cert_chain
            .chain()
            .get(1)
            .expect("We want to revoke an intermediate certificate")
            .clone();
        crl.add_public_key(revoked_cert.public_key()?);

        let uds_certs = UdsCertificates::from(vec![uds_cert_chain]);

        let certified_uds_pubs = CertifiedUdsPubs::from_uds_certs(&uds_certs, &root_ca_pub, &crl)?;

        assert!(!certified_uds_pubs.is_certified(&uds_pub));
        Ok(())
    }
}
