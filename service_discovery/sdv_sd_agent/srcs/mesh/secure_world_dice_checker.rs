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

use crate::mesh::dice_chains::parse_and_verify_cbor_csr;
use anyhow::{anyhow, bail};
use crypto_rpc::cert::x509_common_name_to_string;
use hwtrust::{publickey::PublicKey, rkp::Csr};
use openssl::{
    pkey::{PKey, Public},
    x509::X509,
};

use super::sd_certificate_claims::SdCertificateClaims;

/// A checker for the secure world DICE chain in the SD certificate claims which checks
/// 1. The CSR in the SD certificate claims is signed by the leaf CDI of the DICE chain
/// 2. The secure world signer certificate is signed using the attestation key that
///    corresponds to the public key in the CSR.
/// 3. The secure world signer certificate is issued for the subject public key
///    in the SD certificate claims.
pub(crate) fn check_secure_world_dice_chain(claims: &SdCertificateClaims) -> anyhow::Result<()> {
    // The #1 check is done during deserialization.
    let csr = parse_and_verify_cbor_csr(claims.rkp_csr.as_slice())?;

    check(
        csr,
        &claims.secure_world_signer_certificate_der,
        &claims.subject_public_key,
        &claims.subject_name(),
    )
}

fn check(
    csr: Csr,
    signer_certificate_der: &[u8],
    claimed_public_key: &PKey<Public>,
    claimed_subject_name: &str,
) -> anyhow::Result<()> {
    // The #2 check
    let Csr::V3 { csr_payload, .. } = csr else {
        bail!("Only V3 CSR with csr_payload is supported");
    };
    let pub_keys_in_csr = csr_payload.keys_to_sign;
    if pub_keys_in_csr.0.len() != 1 {
        bail!("The CSR should contain exactly one public key");
    }

    let csr_pub_key =
        PublicKey::from_cose_key(pub_keys_in_csr.0.first().ok_or(anyhow!("No public key found"))?)?;
    let signer_certificate = X509::from_der(signer_certificate_der)?;
    if !signer_certificate.verify(csr_pub_key.pkey())? {
        bail!("Secure world signer certificate is not signed by the public key in the CSR.");
    }

    // The #3 check
    if !signer_certificate.public_key()?.public_eq(claimed_public_key) {
        bail!("X.509 subject public key != secure world Signer cert subject public key");
    }

    let signer_certificate_subject = x509_common_name_to_string(signer_certificate.subject_name())?;
    if signer_certificate_subject != claimed_subject_name {
        bail!("X.509 subject != secure world Signer cert subject");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::mesh::sd_certificate_claims::subject_name_from_vm_name;
    use crate::mesh::AuthenticatedCsr;
    use coset::{iana::EllipticCurve, CoseKeyBuilder, CoseSign1};
    use crypto_rpc::cert::{build_ec_private_key, public_from_private};
    use hwtrust::rkp::{CsrPayload, CsrV2Builder, CsrV3Builder, KeysToSign, ProtectedData};
    use openssl::pkey::{PKey, Private};
    use sdv_comms_id::names::VmName;

    use super::*;

    fn create_sd_certificate_claims() -> anyhow::Result<SdCertificateClaims> {
        let sd_private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let subject_public_key = public_from_private(&sd_private_key)?;
        let vm_name: VmName = "neighervm".to_owned().try_into()?;

        let csr = AuthenticatedCsr::create()?;
        let secure_world_signer_certificate_der =
            csr.attest(&sd_private_key, &subject_name_from_vm_name(&vm_name))?;

        Ok(SdCertificateClaims {
            android_sdv: Default::default(),
            android_sdv_signer_certificate: CoseSign1::default(),
            rkp_csr: csr.cbor,
            secure_world_signer_certificate_der,
            subject_public_key,
            uds_certs_cbor: None,
            vm_name,
        })
    }

    fn build_v2_csr() -> anyhow::Result<Csr> {
        let valid_v3_csr = build_good_csr()?;
        let Csr::V3 { dice_chain, challenge, csr_payload, .. } = valid_v3_csr else {
            bail!("Failed to parse V3 CSR");
        };
        let device_info = csr_payload.device_info;
        let protected_data = ProtectedData::new(vec![0; 32], dice_chain, None);

        Ok(CsrV2Builder::default()
            .device_info(device_info)
            .challenge(challenge)
            .protected_data(protected_data)
            .build()?)
    }

    fn build_good_csr() -> anyhow::Result<Csr> {
        parse_and_verify_cbor_csr(AuthenticatedCsr::create()?.cbor.as_slice())
    }

    fn build_csr_without_pubkey() -> anyhow::Result<Csr> {
        let valid_v3_csr = build_good_csr()?;
        let Csr::V3 { dice_chain, challenge, uds_certs, csr_payload } = valid_v3_csr else {
            bail!("Failed to parse V3 CSR");
        };
        let bad_csr_payload = CsrPayload { keys_to_sign: KeysToSign(vec![]), ..csr_payload };

        Ok(CsrV3Builder::default()
            .challenge(challenge)
            .dice_chain(dice_chain)
            .uds_certs(uds_certs)
            .csr_payload(bad_csr_payload)
            .build()?)
    }

    fn build_csr_two_pubkeys() -> anyhow::Result<Csr> {
        let valid_v3_csr = build_good_csr()?;
        let Csr::V3 { dice_chain, challenge, uds_certs, csr_payload } = valid_v3_csr else {
            bail!("Failed to parse V3 CSR");
        };
        let key1 = CoseKeyBuilder::new_ec2_pub_key(
            EllipticCurve::P_256,
            vec![
                34, 113, 65, 21, 5, 175, 166, 212, 223, 238, 76, 85, 237, 149, 239, 4, 185, 13, 6,
                150, 232, 134, 135, 100, 118, 44, 8, 85, 34, 116, 78, 4,
            ],
            vec![
                131, 229, 50, 227, 235, 176, 238, 252, 199, 252, 244, 138, 23, 131, 178, 103, 174,
                240, 52, 176, 240, 130, 25, 36, 192, 180, 99, 37, 35, 169, 195, 147,
            ],
        )
        .build();
        let key2 = CoseKeyBuilder::new_ec2_pub_key(
            EllipticCurve::P_256,
            vec![
                34, 113, 60, 21, 5, 175, 166, 227, 223, 238, 76, 85, 237, 149, 239, 4, 185, 13, 6,
                150, 232, 134, 135, 100, 118, 44, 8, 85, 34, 116, 78, 4,
            ],
            vec![
                131, 229, 51, 227, 235, 176, 238, 252, 199, 252, 244, 138, 23, 131, 178, 103, 174,
                240, 52, 176, 241, 130, 25, 36, 192, 180, 99, 37, 35, 169, 195, 147,
            ],
        )
        .build();
        let bad_csr_payload =
            CsrPayload { keys_to_sign: KeysToSign(vec![key1, key2]), ..csr_payload };

        Ok(CsrV3Builder::default()
            .challenge(challenge)
            .dice_chain(dice_chain)
            .uds_certs(uds_certs)
            .csr_payload(bad_csr_payload)
            .build()?)
    }

    #[test]
    fn check_secure_world_dice_chain_succeeds() -> anyhow::Result<()> {
        let certificate_claims = create_sd_certificate_claims()?;

        check_secure_world_dice_chain(&certificate_claims)?;

        Ok(())
    }

    #[test]
    fn check_v2_csr_fails() -> anyhow::Result<()> {
        let bad_csr = build_v2_csr()?;
        let private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let public_key = public_from_private(&private_key)?;

        let err = check(bad_csr, &[], &public_key, "").expect_err("check() should have failed");

        assert_eq!(err.to_string(), "Only V3 CSR with csr_payload is supported");

        Ok(())
    }

    #[test]
    fn check_csr_without_pubkey_fails() -> anyhow::Result<()> {
        let bad_csr = build_csr_without_pubkey()?;
        let private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let public_key = public_from_private(&private_key)?;

        check(bad_csr, &[], &public_key, "").expect_err("check() should have failed");

        Ok(())
    }

    #[test]
    fn check_csr_two_pubkeys_fails() -> anyhow::Result<()> {
        let bad_csr = build_csr_two_pubkeys()?;
        let private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let public_key = public_from_private(&private_key)?;

        let err = check(bad_csr, &[], &public_key, "").expect_err("check() should have failed");

        assert_eq!(err.to_string(), "The CSR should contain exactly one public key");

        Ok(())
    }

    #[test]
    fn check_csr_pubkey_mismatch_certificate_fails() -> anyhow::Result<()> {
        let good_csr = build_good_csr()?;
        let private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let public_key = public_from_private(&private_key)?;
        let another_csr = AuthenticatedCsr::create()?;
        let subject = "any";
        let signer_certificate_by_another_csr = another_csr.attest(&private_key, subject)?;

        let err = check(good_csr, &signer_certificate_by_another_csr, &public_key, subject)
            .expect_err("check() should have failed");

        assert_eq!(
            err.to_string(),
            "Secure world signer certificate is not signed by the public key in the CSR."
        );

        Ok(())
    }

    #[test]
    fn check_certificate_pubkey_mismatch_claims_fails() -> anyhow::Result<()> {
        let csr = AuthenticatedCsr::create()?;
        let good_csr = parse_and_verify_cbor_csr(csr.cbor.as_slice())?;
        let private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let another_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let public_key = public_from_private(&another_key)?;
        let subject = "any";
        let signer_certificate = csr.attest(&private_key, subject)?;

        let err = check(good_csr, &signer_certificate, &public_key, subject)
            .expect_err("check() should have failed");

        assert_eq!(
            err.to_string(),
            "X.509 subject public key != secure world Signer cert subject public key"
        );

        Ok(())
    }

    #[test]
    fn check_certificate_subject_mismatch_claims_fails() -> anyhow::Result<()> {
        let csr = AuthenticatedCsr::create()?;
        let good_csr = parse_and_verify_cbor_csr(csr.cbor.as_slice())?;
        let private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let public_key = public_from_private(&private_key)?;
        let subject = "any";
        let another_subject = "anotherany";
        let signer_certificate = csr.attest(&private_key, subject)?;

        let err = check(good_csr, &signer_certificate, &public_key, another_subject)
            .expect_err("check() should have failed");

        assert_eq!(err.to_string(), "X.509 subject != secure world Signer cert subject");

        Ok(())
    }
}
