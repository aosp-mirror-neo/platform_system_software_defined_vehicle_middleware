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

use super::dice_chains::{get_android_dice_chain, parse_and_verify_cbor_csr, uds_pub_from_csr};
use super::sd_certificate_claims::{subject_name_from_vm_name, SdCertificateClaims};
use super::vvm_config::VvmConfig;
use super::AuthenticatedCsr;
use anyhow::Context;
use coset::{CborSerializable, CoseSign1};
use crypto_rpc::cert::{build_ec_private_key, public_from_private};
use dice_chain_verifier::VerifiedSdvDiceChain;
use google_sdv_init_open_dice::aidl::google::sdv::init_open_dice::IKeyCertifier::{
    BpKeyCertifier, IKeyCertifier,
};
use hwtrust::publickey::PublicKey;
use openssl::{
    pkey::{PKey, Private, Public},
    x509::X509,
};
use uds_certs::UdsCertificates;
use vvmtruststore::CertifiedUdsPubs;

/// Creates a Service Discovery (SD) Agent key and self-signed X.509 Certificate
///
/// This material is to be used in the mesh, for "SD agent to SD agent" TLS connections.
pub(crate) fn create(vvm_config: &VvmConfig) -> anyhow::Result<(X509, PKey<Private>)> {
    let key_certifier = get_key_certifier()?;
    let android_sdv = get_android_dice_chain()?;
    create_internal(vvm_config, &android_sdv, key_certifier)
}

fn create_internal(
    vvm_config: &VvmConfig,
    android_sdv: &[u8],
    key_certifier: binder::Strong<dyn IKeyCertifier>,
) -> anyhow::Result<(X509, PKey<Private>)> {
    let vm_name = vvm_config.own_cfg.name.clone();
    let android_sdv = android_sdv.to_vec();
    let sd_priv: PKey<Private> = build_ec_private_key()?.try_into()?;
    let sd_pub = public_from_private(&sd_priv)?;
    let subject_name = subject_name_from_vm_name(&vm_name);

    let android_sdv_signer_certificate =
        certify_with_android_sdv_cdi_leaf(key_certifier, &subject_name, &sd_pub)?;

    let csr = AuthenticatedCsr::create()?;
    let secure_world_signer_certificate_der = csr.attest(&sd_priv, &subject_name)?;

    let uds_certs_cbor = match UdsCertificates::load() {
        Ok(uds_certs) => {
            log_whether_uds_pubs_covered_by_certs(
                csr.cbor.as_ref(),
                android_sdv.as_ref(),
                &uds_certs,
                vvm_config,
            )?;
            Some(uds_certs.to_vec()?)
        }
        Err(err) => {
            log::error!("Couldn't load UDS certificates from vvmtruststore: {err}");
            None
        }
    };

    let x509 = SdCertificateClaims {
        android_sdv,
        android_sdv_signer_certificate,
        rkp_csr: csr.cbor,
        secure_world_signer_certificate_der,
        subject_public_key: sd_pub,
        uds_certs_cbor,
        vm_name,
    }
    .to_x509(&sd_priv)?;

    Ok((x509, sd_priv))
}

/// Checks whether the UDS certificates cover all local UDS public keys and logs the outcome.
fn log_whether_uds_pubs_covered_by_certs(
    rkp_csr: &[u8],
    android_sdv: &[u8],
    uds_certs: &UdsCertificates,
    vvm_config: &VvmConfig,
) -> anyhow::Result<()> {
    let csr = parse_and_verify_cbor_csr(rkp_csr)?;
    let secure_world_uds_pub = uds_pub_from_csr(&csr)?;

    let certified_uds_pubs = CertifiedUdsPubs::from_uds_certs(
        uds_certs,
        &vvm_config.uds_ca_pub,
        &vvm_config.uds_ca_rev_list,
    )?;

    let mut all_good = true;

    if !certified_uds_pubs.is_certified(&PublicKey::from_cose_key(&secure_world_uds_pub)?) {
        log::warn!("Secure World UDS_Pub is not covered by the existing uds_certs");
        all_good = false;
    }

    let android_sdv_dice_chain =
        VerifiedSdvDiceChain::from_cbor(android_sdv)?.android_sdv_dice_chain;

    if !certified_uds_pubs.is_certified(android_sdv_dice_chain.root_public_key()) {
        log::warn!("Android SDV UDS_Pub is not covered by the existing uds_certs");
        all_good = false;
    }

    if all_good {
        log::info!("uds_certs covers all local UDS public keys");
    }

    Ok(())
}

fn get_key_certifier() -> anyhow::Result<binder::Strong<dyn IKeyCertifier>> {
    let key_certifier = binder::wait_for_interface::<dyn IKeyCertifier>(
        &(<BpKeyCertifier as IKeyCertifier>::get_descriptor().to_owned() + "/default"),
    )
    .context("Failed to connect to init_open_dice::IKeyCertifier service")?;

    Ok(key_certifier)
}

fn certify_with_android_sdv_cdi_leaf(
    key_certifier: binder::Strong<dyn IKeyCertifier>,
    subject_name: &str,
    sd_pub: &PKey<Public>,
) -> anyhow::Result<CoseSign1> {
    let sd_pub_cbor = pkey_to_cbor(sd_pub.clone())?;

    let cbor_bytes = key_certifier
        .certifySigningKey(subject_name, &sd_pub_cbor)
        .context("Failed to certify SD_Pub with Android SDV CDI Leaf")?;

    Ok(CoseSign1::from_slice(&cbor_bytes)?)
}

fn pkey_to_cbor(pkey: PKey<Public>) -> anyhow::Result<Vec<u8>> {
    let public_key = PublicKey::try_from(pkey)?;
    let cbor: Vec<u8> = public_key.to_cose_key()?.to_vec()?;
    Ok(cbor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::vvm_config::{create_test_uds_ca_pub, VmConfig};
    use binder::BinderFeatures;
    use coset::{CborSerializable, CoseKey};
    use crypto_rpc::cert::build_ec_private_key;
    use google_sdv_init_open_dice::aidl::google::sdv::init_open_dice::IKeyCertifier::{
        BnKeyCertifier, MockIKeyCertifier,
    };
    use sample_dice_handover::SdvDiceArtifactsBuilder;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use vvmtruststore::{CertificateRevocationList, SignerCertificateClaims};

    fn fake_certify_signing_key(subject: &str, pub_key_bytes: &[u8]) -> binder::Result<Vec<u8>> {
        // Not that I'm unwrapping everywhere in this function as it returns a binder::Result
        // and it's not worth converting other errors to it in these tests.
        let cose_key = CoseKey::from_slice(pub_key_bytes).unwrap();

        let subject_public_key = PublicKey::from_cose_key(&cose_key).unwrap();

        let issuer_private_key: PKey<Private> = build_ec_private_key().unwrap().try_into().unwrap();

        let cert = SignerCertificateClaims {
            issuer: "issuer".to_owned(),
            subject: subject.to_owned(),
            subject_public_key,
        }
        .sign(&issuer_private_key)
        .unwrap();

        Ok(cert.to_vec().unwrap())
    }

    fn create_fake_vvmconfig() -> anyhow::Result<VvmConfig> {
        Ok(VvmConfig {
            uds_ca_pub: create_test_uds_ca_pub()?,
            uds_ca_rev_list: CertificateRevocationList::default(),
            policies: vec![],
            remote_cfgs: vec![],
            own_cfg: VmConfig::new_for_testing(
                "instance1".to_owned().try_into()?,
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 98, 3)), 2001),
            ),
        })
    }

    fn create_android_sdv_cbor_chain() -> anyhow::Result<Vec<u8>> {
        Ok(SdvDiceArtifactsBuilder::new_with_sample_values().build()?.dice_chain().to_vec())
    }

    #[test]
    fn create_ok() -> anyhow::Result<()> {
        let mut mock = MockIKeyCertifier::new();
        mock.expect_certifySigningKey().returning(fake_certify_signing_key);

        let key_certifier: binder::Strong<dyn IKeyCertifier> =
            BnKeyCertifier::new_binder(mock, BinderFeatures::default());

        let android_sdv = create_android_sdv_cbor_chain()?;

        let (_x509, _sd_private) =
            create_internal(&create_fake_vvmconfig()?, &android_sdv, key_certifier)?;

        Ok(())
    }
}
