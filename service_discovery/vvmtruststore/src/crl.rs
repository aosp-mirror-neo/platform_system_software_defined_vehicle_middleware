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

use crate::ec_or_okp_cose_key_to_pkey;

use coset::CoseKey;
use openssl::pkey::PKey;
use openssl::pkey::Public;
use openssl::x509::X509;

/// Represents a list of the public keys that are revoked.
#[derive(Default, Clone, Debug)]
pub struct CertificateRevocationList {
    revoked_keys: Vec<PKey<Public>>,
}

impl CertificateRevocationList {
    /// Creates new empty revokation list.
    pub fn new() -> CertificateRevocationList {
        Self::default()
    }

    /// Adds the public key to the revocation list.
    pub fn add_public_key(&mut self, public_key: PKey<Public>) {
        self.revoked_keys.push(public_key);
    }

    fn is_revoked(&self, public_key: &PKey<Public>) -> bool {
        self.revoked_keys.iter().any(|key| key.public_eq(public_key.as_ref()))
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.revoked_keys.len()
    }

    /// Checks if the chain contains certificates with revoked public keys.
    pub fn is_revoked_chain(&self, chain: &[X509]) -> anyhow::Result<bool> {
        // This should not happen. But anyways self signed leaf cert or empty chain
        // is not revoked.
        if chain.is_empty() || chain.len() == 1 {
            return Ok(false);
        }

        // We ignore the verification of the leaf cert because it is not possible
        // that public key of the leaf would appear in the CRL.
        for cert in &chain[..chain.len() - 1] {
            if self.is_revoked(&cert.public_key()?) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl TryFrom<&Vec<CoseKey>> for CertificateRevocationList {
    type Error = anyhow::Error;

    fn try_from(key_list: &Vec<CoseKey>) -> anyhow::Result<Self, Self::Error> {
        let mut crl = CertificateRevocationList::default();
        for rev_key in key_list {
            let revoked_key = ec_or_okp_cose_key_to_pkey(rev_key)?;
            crl.add_public_key(revoked_key);
        }
        Ok(crl)
    }
}

#[cfg(test)]
mod tests {
    use crate::crl::CertificateRevocationList;
    use anyhow::Context;
    use crypto_rpc::cert::{build_ec_private_key, public_from_private};
    use openssl::pkey::PKey;
    use openssl::pkey::Private;
    use openssl::pkey::Public;
    use openssl::x509::X509;
    use test_uds_certs_generator::create_x509_uds_cert_chain_for_uds_pub_from_pkey_secrets;

    fn random_private_key() -> anyhow::Result<PKey<Private>> {
        PKey::from_ec_key(build_ec_private_key()?).context("Failed to generate test private key")
    }

    fn generate_public_key() -> anyhow::Result<PKey<Public>> {
        let private_key = build_ec_private_key()?.try_into()?;
        let public_key = public_from_private(&private_key)?;
        Ok(public_key)
    }

    fn generate_chain(private_keys: &[PKey<Private>]) -> anyhow::Result<Vec<X509>> {
        let leaf_public_key = generate_public_key()?;

        create_x509_uds_cert_chain_for_uds_pub_from_pkey_secrets(private_keys, &leaf_public_key)
    }

    #[test]
    fn revoked() -> anyhow::Result<()> {
        let private_keys =
            vec![random_private_key()?, random_private_key()?, random_private_key()?];
        let chain = generate_chain(&private_keys)?;

        for pr_key in &private_keys[..private_keys.len()] {
            let mut crl = CertificateRevocationList::new();
            crl.add_public_key(public_from_private(pr_key)?);
            assert!(crl.is_revoked_chain(&chain)?);
        }
        Ok(())
    }

    #[test]
    fn not_revoked() -> anyhow::Result<()> {
        let mut crl = CertificateRevocationList::new();
        let public_key = generate_public_key()?;
        crl.add_public_key(public_key);

        let private_keys =
            vec![random_private_key()?, random_private_key()?, random_private_key()?];
        let chain = generate_chain(&private_keys)?;

        assert!(!crl.is_revoked_chain(&chain)?);

        Ok(())
    }

    #[test]
    // Check that 2 public keys are equal after serialization.
    // This is important to check as comparison of the keys is
    // the key operation in CRL.
    fn check_public_eq() -> anyhow::Result<()> {
        let private_key = build_ec_private_key()?.try_into()?;
        let public_key = public_from_private(&private_key)?;
        let public_key_pem = public_key.public_key_to_pem()?;
        let public_key_restored = openssl::pkey::PKey::public_key_from_pem(&public_key_pem)?;
        assert!(public_key.public_eq(&public_key_restored));

        Ok(())
    }

    #[test]
    fn try_from_vec() -> anyhow::Result<()> {
        let cose_key = coset::CoseKeyBuilder::new_ec2_pub_key(
            coset::iana::EllipticCurve::P_256,
            vec![
                34, 113, 60, 21, 5, 175, 166, 227, 223, 238, 76, 85, 237, 149, 239, 4, 185, 13, 6,
                150, 232, 134, 135, 100, 118, 44, 8, 85, 34, 116, 78, 4,
            ],
            vec![
                131, 229, 51, 227, 235, 176, 238, 252, 199, 252, 244, 138, 23, 131, 178, 103, 174,
                240, 52, 176, 241, 130, 25, 36, 192, 180, 99, 37, 35, 169, 195, 147,
            ],
        )
        .algorithm(coset::iana::Algorithm::ES256)
        .build();

        let cose_keys_vec = vec![cose_key];
        let crl = CertificateRevocationList::try_from(&cose_keys_vec)?;
        assert_eq!(crl.len(), 1);

        Ok(())
    }
}
