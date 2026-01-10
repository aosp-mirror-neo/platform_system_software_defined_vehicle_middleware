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

use crate::{Claims, KeyUsage};
use anyhow::{anyhow, bail, Context};
use coset::{CborSerializable, CoseSign1};
use hwtrust::publickey::PublicKey;
use hwtrust_private_key::private_key::PrivateKey;
use openssl::pkey::{PKey, Private};

/// Claims of a certificate for an elliptic-curve (EC) key that can be used for verifying digital
/// signatures, other than signatures on certificates.
///
/// Such an EC key can participate in TLS handshakes.
#[derive(Clone, Debug, PartialEq)]
pub struct SignerCertificateClaims {
    /// Issuer
    pub issuer: String,

    /// Subject
    pub subject: String,

    /// Subject public key
    pub subject_public_key: PublicKey,
}

impl SignerCertificateClaims {
    /// Signs the claims, returning a signer certificate as a CoseSign1.
    pub fn sign(self, issuer_private_key: &PKey<Private>) -> anyhow::Result<CoseSign1> {
        let issuer_private_key = PrivateKey::from_pkey(issuer_private_key.clone())?;

        if self.issuer.is_empty() || self.subject.is_empty() {
            bail!("Both issuer and subject must be non-empty strings.");
        }

        let cert = issuer_private_key.sign_cose_sign1(
            Claims {
                issuer: self.issuer,
                subject: self.subject,
                subject_public_key: self.subject_public_key.to_cose_key()?,
                key_usage: KeyUsage { digital_signature: true, key_cert_sign: false },
            }
            .to_vec()?,
        );

        Ok(cert)
    }

    /// Verifies that the given CoseSign1 is a valid signer certificate
    /// issued by the given issuer, returning the verified claims.
    pub fn verify(
        signer_certificate: &CoseSign1,
        issuer: &str,
        issuer_public_key: &PublicKey,
    ) -> anyhow::Result<Self> {
        let aad = b""; // Additional Authenticated Data
        issuer_public_key
            .verify_cose_sign1(signer_certificate, aad)
            .context("Android SDV Signer Certificate signature verification failed")?;

        let cert_claims = parse_cert_claims(signer_certificate)?;

        if issuer != cert_claims.issuer {
            bail!("Certificate issuer ({}) is not the expected one ({issuer})", cert_claims.issuer);
        }

        if !cert_claims.key_usage.digital_signature || cert_claims.key_usage.key_cert_sign {
            bail!("Certificate has invalid key usage: {:?}", cert_claims.key_usage);
        }

        let public_key = PublicKey::from_cose_key(&cert_claims.subject_public_key)?;

        Ok(SignerCertificateClaims {
            issuer: cert_claims.issuer,
            subject: cert_claims.subject,
            subject_public_key: public_key,
        })
    }
}

fn parse_cert_claims(cert: &CoseSign1) -> anyhow::Result<Claims> {
    let cbor_bytes: &[u8] =
        cert.payload.as_deref().ok_or(anyhow!("Signer certificate has no payload."))?;

    Ok(Claims::from_slice(cbor_bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto_rpc::cert::{build_ec_private_key, public_from_private};

    /// An ED25519 private key.
    /// Generated with
    /// $ openssl genpkey -algorithm ed25519
    const ED25519_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
        MC4CAQAwBQYDK2VwBCIEILKW0KEeuieFxhDAzigQPE4XRTiQx+0/AlAjJqHmUWE6\n\
        -----END PRIVATE KEY-----\n";

    #[test]
    fn create_and_verify_signer_certificate_ok() -> anyhow::Result<()> {
        let cdi_leaf_private_key = PrivateKey::from_pem(ED25519_KEY_PEM)?;
        let cdi_leaf_public_key = cdi_leaf_private_key.public_key();

        let subject_private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let subject_public_key: PublicKey =
            public_from_private(&subject_private_key)?.try_into()?;

        let original_claims = SignerCertificateClaims {
            issuer: "foo".to_owned(),
            subject: "service_discovery.instance1".to_owned(),
            subject_public_key,
        };

        let signer_certificate = original_claims.clone().sign(cdi_leaf_private_key.pkey())?;

        let verified_claims =
            SignerCertificateClaims::verify(&signer_certificate, "foo", &cdi_leaf_public_key)?;

        assert_eq!(original_claims, verified_claims);

        Ok(())
    }
}
