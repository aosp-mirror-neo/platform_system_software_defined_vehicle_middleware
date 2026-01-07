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

use super::dice_chains::{parse_and_verify_cbor_csr, uds_pub_from_csr};
use super::dice_policy_checker::check_dice_policy;
use super::sd_certificate_claims::SdCertificateClaims;
use super::secure_world_dice_checker::check_secure_world_dice_chain;
use super::uds_pubs::UdsPubs;
use super::vvm_config::VvmConfig;

use anyhow::{anyhow, bail, ensure, Context};
use coset::{CborSerializable, CoseKey};
use dice_chain_verifier::{AndroidVerifiedBootState, SdvBootMode, VerifiedSdvDiceChain};
use hwtrust::dice;
use hwtrust::publickey::PublicKey;
use sdv_comms_id::names::VmName;
use uds_certs::UdsCertificates;
use vvmtruststore::{CertifiedUdsPubs, SignerCertificateClaims};

/// Checks the credentials of SDV VMs
#[derive(Clone)]
pub(crate) struct CredentialsChecker {
    uds_pubs: UdsPubs,
    vvm_config: VvmConfig,
}

impl CredentialsChecker {
    pub fn load(vvm_config: VvmConfig) -> anyhow::Result<Self> {
        let uds_pubs = UdsPubs::load()?;
        log::info!(
            "Loaded UdsPubs with {} {} keys",
            uds_pubs.keys().len(),
            if uds_pubs.trusted() { "trusted" } else { "untrusted" }
        );

        Ok(CredentialsChecker { uds_pubs, vvm_config })
    }

    fn check_uds_pub(
        &self,
        vm_name: &VmName,
        uds_pub: &CoseKey,
        certified_uds_pubs: Option<&CertifiedUdsPubs>,
    ) -> anyhow::Result<()> {
        ensure!(
            self.uds_pubs.keys().contains(uds_pub),
            "DICE chain of {vm_name} has an unknown UDS Pub."
        );

        if self.uds_pubs.trusted() {
            return Ok(());
        }

        if let Some(certified_uds_pubs) = certified_uds_pubs {
            ensure!(
                certified_uds_pubs.is_certified(&PublicKey::from_cose_key(uds_pub)?),
                "DICE chain of {vm_name} has a known, but untrusted, UDS Pub without a matching \
                uds_cert."
            );
        } else {
            bail!(
                "DICE chain of {vm_name} has a known, but untrusted, UDS Pub without any uds_certs"
            );
        }

        Ok(())
    }

    pub fn check(&self, claims: &SdCertificateClaims) -> anyhow::Result<()> {
        let VerifiedSdvDiceChain { android_sdv_dice_chain, sdv_config_values, vm_instance_name } =
            VerifiedSdvDiceChain::from_cbor(&claims.android_sdv)?;
        let android_uds_pub = android_sdv_dice_chain.root_public_key().to_cose_key()?;

        self.vvm_config
            .find_remote_config(&claims.vm_name)
            .ok_or(anyhow!("Unknown VM instance name: {}", claims.vm_name))?;

        let android_sdv_cdi_leaf = get_cdi_leaf(&android_sdv_dice_chain)?;

        if vm_instance_name != claims.vm_name.to_string() {
            bail!(
                "Android SDV config descriptor has incorrect Component Instance Name: {vm_instance_name}"
            )
        }

        // Checks that android_sdv_signer_cert is indeed signed by the CDI_Leaf
        // of the Android SDV DICE chain.
        let android_sdv_signer_cert_claims = SignerCertificateClaims::verify(
            &claims.android_sdv_signer_certificate,
            android_sdv_cdi_leaf.subject(),
            android_sdv_cdi_leaf.subject_public_key(),
        )?;

        // The X.509 subject public key must be signed by the CDI_Leaf of the Android SDV DICE
        // chain.
        // This is verified by checking that the X.509 cert pub key is the same as the one in the
        // Android SDV Signer Certificate.
        let sd_pub: PublicKey = claims.subject_public_key.clone().try_into()?;
        if android_sdv_signer_cert_claims.subject_public_key != sd_pub {
            bail!("X.509 subject public key != Android SDV Signer cert subject public key");
        }
        if android_sdv_signer_cert_claims.subject != claims.subject_name() {
            bail!("X.509 subject != Android SDV Signer cert subject");
        }

        let certified_uds_pubs = self.certified_uds_pubs_in_claims(claims)?;

        self.check_uds_pub(&claims.vm_name, &android_uds_pub, certified_uds_pubs.as_ref())
            .context("Invalid Android SDV UDS public key")?;
        let csr = parse_and_verify_cbor_csr(claims.rkp_csr.as_slice())?;
        self.check_uds_pub(&claims.vm_name, &uds_pub_from_csr(&csr)?, certified_uds_pubs.as_ref())
            .context("Invalid secure world UDS public key")?;

        ensure!(
            sdv_config_values.sdv_boot_mode == SdvBootMode::Locked,
            "VM must be SDV Boot Mode locked"
        );
        ensure!(
            sdv_config_values.verified_boot_state == AndroidVerifiedBootState::Green,
            "VM must have a green Android Verified Boot State, got {}",
            sdv_config_values.verified_boot_state
        );

        check_dice_policy(claims, &self.vvm_config)?;

        check_secure_world_dice_chain(claims)?;

        log::info!("Verification of {} successful", claims.vm_name);

        Ok(())
    }

    fn certified_uds_pubs_in_claims(
        &self,
        claims: &SdCertificateClaims,
    ) -> anyhow::Result<Option<CertifiedUdsPubs>> {
        if let Some(uds_certs_cbor) = &claims.uds_certs_cbor {
            let uds_certs = UdsCertificates::from_slice(uds_certs_cbor)?;
            let certified_uds_pubs = CertifiedUdsPubs::from_uds_certs(
                &uds_certs,
                &self.vvm_config.uds_ca_pub,
                &self.vvm_config.uds_ca_rev_list,
            )?;
            Ok(Some(certified_uds_pubs))
        } else {
            Ok(None)
        }
    }
}

fn get_cdi_leaf(chain: &dice::Chain) -> anyhow::Result<&dice::Payload> {
    // This shall never happen since dice::Chain::validate() already covers
    // this from within dice::Chain::from_cbor().
    // But since the structure, a Vec<Payload>, actually allows for an empty vector
    // to be represented, we are forced to check that again ourselves.
    chain.payloads().last().ok_or(anyhow!("DICE chain has no CDI certs"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::mesh::dice_policy_checker::tests::{
        create_good_android_sdv_policy, create_good_secure_world_policy,
    };
    use crate::mesh::sd_certificate_claims;
    use crate::mesh::vvm_config::VmConfig;
    use crate::mesh::AuthenticatedCsr;
    use crypto_rpc::cert::{build_ec_private_key, public_from_private};
    use dice_chain_verifier::SdvConfigValues;
    use dice_policy::DicePolicy;
    use diced_open_dice::{Cdi, KeyAlgorithm};
    use openssl::ec::{EcGroup, EcKey};
    use openssl::nid::Nid;
    use openssl::pkey::{PKey, Private, Public};
    use sample_dice_handover::{create_random_uds, OwnedSdvDiceArtifacts, SdvDiceArtifactsBuilder};
    use sdv_dice::CertificateIssuer;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use test_dice_artifacts_builder::uds_public_key_from_uds;
    use test_uds_certs_generator::{
        create_x509_uds_cert_chain_for_uds_pub_from_pkey_secrets,
        create_x509_uds_cert_chain_from_pkey_secrets,
    };
    use uds_certs::UdsCertChain;
    use vvmtruststore::{ec_or_okp_cose_key_to_pkey, CertificateRevocationList};

    fn create_vmconfig_foo() -> anyhow::Result<VmConfig> {
        let name_str = "foo";
        let name: VmName = name_str.to_owned().try_into()?;
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let android = 0u32;
        let secure_world = 1u32;

        Ok(VmConfig { name, addr, android, secure_world })
    }

    fn create_vmconfig_bar() -> anyhow::Result<VmConfig> {
        let name_str = "bar";
        let name: VmName = name_str.to_owned().try_into()?;
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 8080);
        let android = 0u32;
        let secure_world = 1u32;

        Ok(VmConfig { name, addr, android, secure_world })
    }

    pub(crate) fn create_vvmconfig() -> anyhow::Result<VvmConfig> {
        create_vvmconfig_with_uds_ca_pub(public_from_private(&generate_ec_p256_key()?)?)
    }

    fn create_vvmconfig_with_uds_ca_pub(pub_key: PKey<Public>) -> anyhow::Result<VvmConfig> {
        let uds_ca_pub = PublicKey::try_from(pub_key.clone())?;
        let uds_ca_rev_list = CertificateRevocationList::default();
        let policies: Vec<DicePolicy> =
            vec![create_good_android_sdv_policy()?, create_good_secure_world_policy()?];
        let remote_cfgs = vec![create_vmconfig_foo()?];
        let own_cfg = create_vmconfig_bar()?;

        Ok(VvmConfig { uds_ca_pub, uds_ca_rev_list, policies, remote_cfgs, own_cfg })
    }

    fn create_uds_pubs(uds: &Cdi, csr_cbor: &[u8]) -> anyhow::Result<UdsPubs> {
        let uds_pub = uds_public_key_from_uds(uds)?;
        let csr_uds_pub = uds_pub_from_csr_cbor(csr_cbor)?;
        let trusted_keys: Vec<CoseKey> = vec![uds_pub, csr_uds_pub];
        Ok(UdsPubs::new_trusted(trusted_keys))
    }

    fn create_untrusted_uds_pubs(uds: &Cdi, csr_cbor: &[u8]) -> anyhow::Result<UdsPubs> {
        let uds_pub = uds_public_key_from_uds(uds)?;
        let csr_uds_pub = uds_pub_from_csr_cbor(csr_cbor)?;
        let trusted_keys: Vec<CoseKey> = vec![uds_pub, csr_uds_pub];
        Ok(UdsPubs::new_untrusted(trusted_keys))
    }

    fn create_only_csr_uds_pubs(csr_cbor: &[u8]) -> anyhow::Result<UdsPubs> {
        let csr_uds_pub = uds_pub_from_csr_cbor(csr_cbor)?;
        let trusted_keys: Vec<CoseKey> = vec![csr_uds_pub];
        Ok(UdsPubs::new_trusted(trusted_keys))
    }

    fn create_only_android_uds_pubs(uds: &Cdi) -> anyhow::Result<UdsPubs> {
        let uds_pub = uds_public_key_from_uds(uds)?;
        let trusted_keys: Vec<CoseKey> = vec![uds_pub];
        Ok(UdsPubs::new_trusted(trusted_keys))
    }

    fn uds_pub_from_csr_cbor(csr_cbor: &[u8]) -> anyhow::Result<CoseKey> {
        let csr = parse_and_verify_cbor_csr(csr_cbor)?;
        uds_pub_from_csr(&csr)
    }

    fn create_dice_artifacts(vm_name: &VmName, uds: &Cdi) -> anyhow::Result<OwnedSdvDiceArtifacts> {
        SdvDiceArtifactsBuilder {
            uds: *uds,
            sdv_config_values: SdvConfigValues::new_with_sample_values(),
            vm_instance_name: vm_name.to_string(),
            key_algorithm: KeyAlgorithm::Ed25519,
        }
        .build()
    }

    fn generate_ec_p256_key() -> anyhow::Result<PKey<Private>> {
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
        let ec_key = EcKey::generate(&group)?;
        let pkey = PKey::from_ec_key(ec_key)?;
        Ok(pkey)
    }

    fn generate_uds_cert_chain_for_uds(
        root_ca_priv: &PKey<Private>,
        uds: &Cdi,
    ) -> anyhow::Result<UdsCertChain> {
        let keys = vec![root_ca_priv.clone(), generate_ec_p256_key()?, generate_ec_p256_key()?];
        let uds_cert_chain = create_x509_uds_cert_chain_from_pkey_secrets(&keys, uds)?;
        UdsCertChain::try_from(uds_cert_chain)
    }

    fn generate_uds_cert_chain_for_uds_pub(
        root_ca_priv: &PKey<Private>,
        uds_pub: &PKey<Public>,
    ) -> anyhow::Result<UdsCertChain> {
        let keys = vec![root_ca_priv.clone(), generate_ec_p256_key()?, generate_ec_p256_key()?];
        let uds_cert_chain =
            create_x509_uds_cert_chain_for_uds_pub_from_pkey_secrets(&keys, uds_pub)?;
        UdsCertChain::try_from(uds_cert_chain)
    }

    fn generate_uds_certificates(
        root_ca_priv: &PKey<Private>,
        uds: &Cdi,
    ) -> anyhow::Result<UdsCertificates> {
        Ok(UdsCertificates::from(vec![generate_uds_cert_chain_for_uds(root_ca_priv, uds)?]))
    }

    pub(crate) struct SdClaimsBuilder<'a> {
        vm_name: &'a VmName,
        uds: &'a Cdi,
        auth_csr: &'a AuthenticatedCsr,
        sd_private_key: Option<&'a PKey<Private>>,
    }
    impl<'a> SdClaimsBuilder<'a> {
        pub(crate) fn new(
            vm_name: &'a VmName,
            uds: &'a Cdi,
            auth_csr: &'a AuthenticatedCsr,
        ) -> SdClaimsBuilder<'a> {
            SdClaimsBuilder { vm_name, uds, auth_csr, sd_private_key: None }
        }

        pub(crate) fn with_sd_private_key(
            self,
            sd_private_key: &'a PKey<Private>,
        ) -> SdClaimsBuilder<'a> {
            let mut builder = self;
            builder.sd_private_key = Some(sd_private_key);
            builder
        }
        pub(crate) fn build(self) -> anyhow::Result<SdCertificateClaims> {
            let dice_handover = create_dice_artifacts(self.vm_name, self.uds)?;

            let android_sdv = dice_handover.dice_chain().to_vec();

            let android_sdv_signer_cert_issuer =
                CertificateIssuer::from_dice_artifacts(dice_handover.cdi_attest(), &android_sdv)?;

            let random_sd_private_key = build_ec_private_key()?.try_into()?;

            let sd_private_key: &PKey<Private> = match self.sd_private_key {
                Some(key) => key,
                None => &random_sd_private_key,
            };

            let sd_pub = public_from_private(sd_private_key)?;

            let subject = sd_certificate_claims::subject_name_from_vm_name(self.vm_name);

            let secure_world_signer_certificate_der =
                self.auth_csr.attest(sd_private_key, &subject)?;

            let android_sdv_signer_certificate = SignerCertificateClaims {
                issuer: android_sdv_signer_cert_issuer.name.clone(),
                subject,
                subject_public_key: sd_pub.clone().try_into()?,
            }
            .sign(&android_sdv_signer_cert_issuer.private_key)?;

            Ok(SdCertificateClaims {
                android_sdv,
                android_sdv_signer_certificate,
                rkp_csr: self.auth_csr.cbor.clone(),
                secure_world_signer_certificate_der,
                subject_public_key: sd_pub,
                uds_certs_cbor: None,
                vm_name: self.vm_name.clone(),
            })
        }
    }

    pub(crate) fn create_custom_credentials_checker(
        random_uds: &Cdi,
        vvm_config: VvmConfig,
        csr_cbor: &[u8],
    ) -> anyhow::Result<CredentialsChecker> {
        Ok(CredentialsChecker { uds_pubs: create_uds_pubs(random_uds, csr_cbor)?, vvm_config })
    }
    pub(crate) fn create_credentials_checker() -> anyhow::Result<CredentialsChecker> {
        create_custom_credentials_checker(
            &create_random_uds(),
            create_vvmconfig()?,
            AuthenticatedCsr::create()?.cbor.as_slice(),
        )
    }

    pub(crate) fn create_secure_world_signer_cert_der() -> anyhow::Result<Vec<u8>> {
        let private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let csr = AuthenticatedCsr::create()?;
        csr.attest(&private_key, "subject")
    }

    #[test]
    fn check_unknown_vm_name_err() -> anyhow::Result<()> {
        let uds = create_random_uds();
        let vvm_config = create_vvmconfig()?;
        let auth_csr = AuthenticatedCsr::create()?;

        let mut claims =
            SdClaimsBuilder::new(&vvm_config.remote_cfgs[0].name, &uds, &auth_csr).build()?;

        // There's no "foobar" VM instance in our vvmconfig
        claims.vm_name = "foobar".to_owned().try_into()?;

        let checker =
            create_custom_credentials_checker(&uds, vvm_config, claims.rkp_csr.as_slice())?;

        let checker_err = checker.check(&claims).expect_err("check() should have failed");

        assert_eq!(checker_err.to_string(), "Unknown VM instance name: foobar");

        Ok(())
    }

    #[test]
    fn check_ok() -> anyhow::Result<()> {
        let uds = create_random_uds();
        let vvm_config = create_vvmconfig()?;
        let auth_csr = AuthenticatedCsr::create()?;
        let claims =
            SdClaimsBuilder::new(&vvm_config.remote_cfgs[0].name, &uds, &auth_csr).build()?;

        let checker =
            create_custom_credentials_checker(&uds, vvm_config, claims.rkp_csr.as_slice())?;

        checker.check(&claims)?;

        Ok(())
    }

    // Tests that an Android SDV signer certificate signed by an unknown key causes check() to fail.
    #[test]
    fn check_bad_android_sdv_signer_cert_err() -> anyhow::Result<()> {
        let uds = create_random_uds();
        let vvm_config = create_vvmconfig()?;

        let dice_handover = create_dice_artifacts(&vvm_config.remote_cfgs[0].name, &uds)?;

        let android_sdv = dice_handover.dice_chain().to_vec();

        let android_sdv_signer_cert_issuer =
            CertificateIssuer::from_dice_artifacts(dice_handover.cdi_attest(), &android_sdv)?;

        let sd_private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let sd_pub = public_from_private(&sd_private_key)?;

        let another_private_key: PKey<Private> = build_ec_private_key()?.try_into()?;

        let remote_vm_name = vvm_config.remote_cfgs[0].name.clone();

        let android_sdv_signer_certificate = SignerCertificateClaims {
            issuer: android_sdv_signer_cert_issuer.name.clone(),
            subject: sd_certificate_claims::subject_name_from_vm_name(&remote_vm_name),
            subject_public_key: sd_pub.clone().try_into()?,
        }
        // Signing with a key that is not CDI_Leaf's attestation key.
        .sign(&another_private_key)?;

        let auth_csr = AuthenticatedCsr::create()?;

        let claims = SdCertificateClaims {
            android_sdv,
            android_sdv_signer_certificate,
            rkp_csr: auth_csr.cbor.clone(),
            secure_world_signer_certificate_der: create_secure_world_signer_cert_der()?,
            subject_public_key: sd_pub,
            uds_certs_cbor: None,
            vm_name: remote_vm_name,
        };

        let checker =
            create_custom_credentials_checker(&uds, vvm_config, auth_csr.cbor.as_slice())?;

        let err = checker.check(&claims).expect_err("check() should have failed");

        assert_eq!(err.to_string(), "Android SDV Signer Certificate signature verification failed");

        Ok(())
    }

    // Tests that an Android SDV signer certificate with a subject public key different
    // from the subject public key in the container X.509 cert causes check() to fail.
    #[test]
    fn check_unknown_android_sdv_signer_subject_public_key_err() -> anyhow::Result<()> {
        let uds = create_random_uds();
        let vvm_config = create_vvmconfig()?;

        let dice_handover = create_dice_artifacts(&vvm_config.remote_cfgs[0].name, &uds)?;

        let android_sdv = dice_handover.dice_chain().to_vec();

        let android_sdv_signer_cert_issuer =
            CertificateIssuer::from_dice_artifacts(dice_handover.cdi_attest(), &android_sdv)?;

        let sd_private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let sd_pub = public_from_private(&sd_private_key)?;

        let another_public_key = public_from_private(&build_ec_private_key()?.try_into()?)?;

        let remote_vm_name = vvm_config.remote_cfgs[0].name.clone();

        let android_sdv_signer_certificate = SignerCertificateClaims {
            issuer: android_sdv_signer_cert_issuer.name.clone(),
            subject: sd_certificate_claims::subject_name_from_vm_name(&remote_vm_name),
            // In a correct cert, this should have been sd_pub
            subject_public_key: another_public_key.try_into()?,
        }
        .sign(&android_sdv_signer_cert_issuer.private_key)?;

        let auth_csr = AuthenticatedCsr::create()?;

        let claims = SdCertificateClaims {
            android_sdv,
            android_sdv_signer_certificate,
            rkp_csr: auth_csr.cbor.clone(),
            secure_world_signer_certificate_der: create_secure_world_signer_cert_der()?,
            subject_public_key: sd_pub,
            uds_certs_cbor: None,
            vm_name: remote_vm_name,
        };

        let checker =
            create_custom_credentials_checker(&uds, vvm_config, auth_csr.cbor.as_slice())?;

        let err = checker.check(&claims).expect_err("check() should have failed");

        assert_eq!(
            err.to_string(),
            "X.509 subject public key != Android SDV Signer cert subject public key"
        );

        Ok(())
    }

    // If the component instance name in the Android SDV CDI certificate doesn't match the VM name
    // stated in the container X.509 certificate subject, check() must fail.
    #[test]
    fn check_android_sdv_component_instance_name_mismatch_err() -> anyhow::Result<()> {
        let uds = create_random_uds();
        let vvm_config = create_vvmconfig()?;

        let remote_vm_name = vvm_config.remote_cfgs[0].name.clone();

        let dice_artifacts = SdvDiceArtifactsBuilder {
            uds,
            sdv_config_values: SdvConfigValues::new_with_sample_values(),
            // The component instance name of the Android SDV CDI should have been
            // remote_vm_name.to_string().
            //
            // There's now a mismatch between what the Android SDV CDI, created by the
            // bootloader/hypervisor of the remote VM, claims and what the container X.509
            // certificate created by the sdv_sd_agent of that remote VM claims.
            vm_instance_name: "something_else".to_string(),
            key_algorithm: KeyAlgorithm::Ed25519,
        }
        .build()?;

        let android_sdv = dice_artifacts.dice_chain().to_vec();

        let android_sdv_signer_cert_issuer =
            CertificateIssuer::from_dice_artifacts(dice_artifacts.cdi_attest(), &android_sdv)?;

        let sd_private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let sd_pub = public_from_private(&sd_private_key)?;

        let android_sdv_signer_certificate = SignerCertificateClaims {
            issuer: android_sdv_signer_cert_issuer.name.clone(),
            subject: sd_certificate_claims::subject_name_from_vm_name(&remote_vm_name),
            subject_public_key: sd_pub.clone().try_into()?,
        }
        .sign(&android_sdv_signer_cert_issuer.private_key)?;

        let auth_csr = AuthenticatedCsr::create()?;

        let sd_cert_claims = SdCertificateClaims {
            android_sdv,
            android_sdv_signer_certificate,
            rkp_csr: auth_csr.cbor.clone(),
            secure_world_signer_certificate_der: create_secure_world_signer_cert_der()?,
            subject_public_key: sd_pub,
            uds_certs_cbor: None,
            vm_name: remote_vm_name,
        };

        let checker =
            create_custom_credentials_checker(&uds, vvm_config, auth_csr.cbor.as_slice())?;

        let checker_err = checker.check(&sd_cert_claims).expect_err("check() should have failed");

        assert_eq!(
            checker_err.to_string(),
            "Android SDV config descriptor has incorrect Component Instance Name: something_else"
        );

        Ok(())
    }

    #[test]
    fn check_trusted_uds_pubs_and_missing_uds_certs_is_ok() -> anyhow::Result<()> {
        let uds = create_random_uds();
        let vvm_config = create_vvmconfig()?;
        let auth_csr = AuthenticatedCsr::create()?;
        let mut claims =
            SdClaimsBuilder::new(&vvm_config.remote_cfgs[0].name, &uds, &auth_csr).build()?;
        claims.uds_certs_cbor = None;

        let checker = CredentialsChecker {
            uds_pubs: create_uds_pubs(&uds, claims.rkp_csr.as_slice())?,
            vvm_config,
        };

        checker.check(&claims)?;

        Ok(())
    }

    #[test]
    fn check_untrusted_uds_pubs_and_missing_uds_certs_err() -> anyhow::Result<()> {
        let uds = create_random_uds();
        let vvm_config = create_vvmconfig()?;
        let auth_csr = AuthenticatedCsr::create()?;
        let mut claims =
            SdClaimsBuilder::new(&vvm_config.remote_cfgs[0].name, &uds, &auth_csr).build()?;
        claims.uds_certs_cbor = None;

        let checker = CredentialsChecker {
            uds_pubs: create_untrusted_uds_pubs(&uds, claims.rkp_csr.as_slice())?,
            vvm_config,
        };

        let checker_err = checker.check(&claims).expect_err("check() should have failed");

        assert_eq!(checker_err.to_string(), "Invalid Android SDV UDS public key");
        assert_eq!(
            checker_err.root_cause().to_string(),
            "DICE chain of foo has a known, but untrusted, UDS Pub without any uds_certs"
        );

        Ok(())
    }

    #[test]
    fn check_untrusted_uds_pubs_but_with_uds_cert_is_ok() -> anyhow::Result<()> {
        let uds_android_sdv = create_random_uds();
        let uds_root_ca_priv = generate_ec_p256_key()?;
        let vvm_config = create_vvmconfig_with_uds_ca_pub(public_from_private(&uds_root_ca_priv)?)?;
        let auth_csr = AuthenticatedCsr::create()?;
        let mut claims =
            SdClaimsBuilder::new(&vvm_config.remote_cfgs[0].name, &uds_android_sdv, &auth_csr)
                .build()?;
        let secure_world_uds_pub =
            ec_or_okp_cose_key_to_pkey(&uds_pub_from_csr_cbor(&claims.rkp_csr)?)?;
        claims.uds_certs_cbor = Some(
            UdsCertificates::from(vec![
                generate_uds_cert_chain_for_uds_pub(&uds_root_ca_priv, &secure_world_uds_pub)?,
                generate_uds_cert_chain_for_uds(&uds_root_ca_priv, &uds_android_sdv)?,
            ])
            .to_vec()?,
        );

        let checker = CredentialsChecker {
            uds_pubs: create_untrusted_uds_pubs(&uds_android_sdv, claims.rkp_csr.as_slice())?,
            vvm_config,
        };

        checker.check(&claims)?;

        Ok(())
    }

    #[test]
    fn check_untrusted_uds_pubs_but_with_android_uds_cert_and_missing_secure_uds_cert_is_err(
    ) -> anyhow::Result<()> {
        let uds = create_random_uds();
        let uds_root_ca_priv = generate_ec_p256_key()?;
        let vvm_config = create_vvmconfig_with_uds_ca_pub(public_from_private(&uds_root_ca_priv)?)?;
        let auth_csr = AuthenticatedCsr::create()?;
        let mut claims =
            SdClaimsBuilder::new(&vvm_config.remote_cfgs[0].name, &uds, &auth_csr).build()?;
        claims.uds_certs_cbor = Some(generate_uds_certificates(&uds_root_ca_priv, &uds)?.to_vec()?);

        let checker = CredentialsChecker {
            uds_pubs: create_untrusted_uds_pubs(&uds, claims.rkp_csr.as_slice())?,
            vvm_config,
        };

        let checker_err = checker.check(&claims).expect_err("check() should have failed");
        assert_eq!(checker_err.to_string(), "Invalid secure world UDS public key");
        assert_eq!(
            checker_err.root_cause().to_string(),
            "DICE chain of foo has a known, but untrusted, UDS Pub without a matching uds_cert."
        );

        Ok(())
    }

    #[test]
    fn check_untrusted_uds_pubs_and_uds_cert_for_different_uds_is_err() -> anyhow::Result<()> {
        let uds = create_random_uds();
        let uds_root_ca_priv = generate_ec_p256_key()?;
        let vvm_config = create_vvmconfig_with_uds_ca_pub(public_from_private(&uds_root_ca_priv)?)?;
        let auth_csr = AuthenticatedCsr::create()?;
        let mut claims =
            SdClaimsBuilder::new(&vvm_config.remote_cfgs[0].name, &uds, &auth_csr).build()?;
        let some_other_uds = create_random_uds();
        claims.uds_certs_cbor =
            Some(generate_uds_certificates(&uds_root_ca_priv, &some_other_uds)?.to_vec()?);

        let checker = CredentialsChecker {
            uds_pubs: create_untrusted_uds_pubs(&uds, claims.rkp_csr.as_slice())?,
            vvm_config,
        };

        let checker_err = checker.check(&claims).expect_err("check() should have failed");
        assert_eq!(
            checker_err.root_cause().to_string(),
            "DICE chain of foo has a known, but untrusted, UDS Pub without a matching uds_cert."
        );

        Ok(())
    }

    #[test]
    fn check_untrusted_uds_pubs_and_uds_cert_with_untrusted_root_ca_is_err() -> anyhow::Result<()> {
        let uds = create_random_uds();
        let uds_root_ca_priv = generate_ec_p256_key()?;
        let vvm_config = create_vvmconfig_with_uds_ca_pub(public_from_private(&uds_root_ca_priv)?)?;
        let auth_csr = AuthenticatedCsr::create()?;
        let mut claims =
            SdClaimsBuilder::new(&vvm_config.remote_cfgs[0].name, &uds, &auth_csr).build()?;
        let untrusted_root_ca_priv = generate_ec_p256_key()?;
        claims.uds_certs_cbor =
            Some(generate_uds_certificates(&untrusted_root_ca_priv, &uds)?.to_vec()?);

        let checker = CredentialsChecker {
            uds_pubs: create_untrusted_uds_pubs(&uds, claims.rkp_csr.as_slice())?,
            vvm_config,
        };

        let checker_err = checker.check(&claims).expect_err("check() should have failed");
        assert_eq!(
            checker_err.root_cause().to_string(),
            "DICE chain of foo has a known, but untrusted, UDS Pub without a matching uds_cert."
        );

        Ok(())
    }

    /// Tests that CredentialsChecker::check fails if the received SdCertificateClaims contains a
    /// UDS certificate chain with an intermediate CA that has been revoked.
    #[test]
    fn check_untrusted_uds_pubs_and_uds_cert_with_revoked_intermediate_ca_is_err(
    ) -> anyhow::Result<()> {
        let uds_android_sdv = create_random_uds();
        let uds_root_ca_priv = generate_ec_p256_key()?;
        let mut vvm_config =
            create_vvmconfig_with_uds_ca_pub(public_from_private(&uds_root_ca_priv)?)?;
        let auth_csr = AuthenticatedCsr::create()?;
        let mut claims =
            SdClaimsBuilder::new(&vvm_config.remote_cfgs[0].name, &uds_android_sdv, &auth_csr)
                .build()?;
        let secure_world_uds_pub =
            ec_or_okp_cose_key_to_pkey(&uds_pub_from_csr_cbor(&claims.rkp_csr)?)?;

        let revoked_intermediate_ca_priv_key = generate_ec_p256_key()?;
        let revoked_intermediate_ca_pub_key =
            public_from_private(&revoked_intermediate_ca_priv_key)?;

        let uds_certs_for_android_sdv =
            UdsCertChain::try_from(create_x509_uds_cert_chain_from_pkey_secrets(
                &[
                    uds_root_ca_priv.clone(),
                    revoked_intermediate_ca_priv_key.clone(),
                    generate_ec_p256_key()?,
                ],
                &uds_android_sdv,
            )?)?;

        // Add certificates for the UDS public keys of both the Secure World and the Android SDV
        // DICE chains of the remote VM.
        claims.uds_certs_cbor = Some(
            UdsCertificates::from(vec![
                generate_uds_cert_chain_for_uds_pub(&uds_root_ca_priv, &secure_world_uds_pub)?,
                uds_certs_for_android_sdv,
            ])
            .to_vec()?,
        );

        // Create an untrusted UdsPubs for the local VM, containing the UDS public keys of the
        // remote VM.
        //
        // This means that the remote VM can join the secure mesh if it provides certificates for
        // its UDS public keys.
        let untrusted_uds_pubs =
            create_untrusted_uds_pubs(&uds_android_sdv, claims.rkp_csr.as_slice())?;

        let checker = CredentialsChecker {
            uds_pubs: untrusted_uds_pubs.clone(),
            vvm_config: vvm_config.clone(),
        };

        checker.check(&claims).expect("Claims with valid UDS cert chains must pass");

        // Now the certificate revocation list of the local VM contains an intermediate CA
        // present in the certificate chain of the remove VM's Android SDV UDS public key.
        vvm_config.uds_ca_rev_list.add_public_key(revoked_intermediate_ca_pub_key);

        let checker = CredentialsChecker { uds_pubs: untrusted_uds_pubs, vvm_config };

        // The remote VM's Android SDV UDS public key must be rejected by the local VM, given that
        // its certificate is no longer valid.
        // That UDS public key certificate is invalid due to the revoked intermediate CA present in
        // its chain.
        let checker_err = checker
            .check(&claims)
            .expect_err("Claims with revoked UDS cert chain and untrusted UDS pub must fail");
        assert_eq!(checker_err.to_string(), "Invalid Android SDV UDS public key");
        assert_eq!(
            checker_err.root_cause().to_string(),
            "DICE chain of foo has a known, but untrusted, UDS Pub without a matching uds_cert."
        );

        Ok(())
    }

    #[test]
    fn check_unknown_android_sdv_uds_pubs_err() -> anyhow::Result<()> {
        let uds = create_random_uds();
        let vvm_config = create_vvmconfig()?;
        let auth_csr = AuthenticatedCsr::create()?;
        let claims =
            SdClaimsBuilder::new(&vvm_config.remote_cfgs[0].name, &uds, &auth_csr).build()?;

        let checker = CredentialsChecker {
            uds_pubs: create_only_csr_uds_pubs(claims.rkp_csr.as_slice())?,
            vvm_config,
        };

        let checker_err = checker.check(&claims).expect_err("check() should have failed");

        assert_eq!(checker_err.to_string(), "Invalid Android SDV UDS public key");
        assert_eq!(
            checker_err.root_cause().to_string(),
            "DICE chain of foo has an unknown UDS Pub."
        );

        Ok(())
    }

    #[test]
    fn check_unknown_secure_world_uds_pubs_err() -> anyhow::Result<()> {
        let uds = create_random_uds();
        let vvm_config = create_vvmconfig()?;
        let auth_csr = AuthenticatedCsr::create()?;
        let claims =
            SdClaimsBuilder::new(&vvm_config.remote_cfgs[0].name, &uds, &auth_csr).build()?;

        let checker =
            CredentialsChecker { uds_pubs: create_only_android_uds_pubs(&uds)?, vvm_config };

        let checker_err = checker.check(&claims).expect_err("check() should have failed");

        assert_eq!(checker_err.to_string(), "Invalid secure world UDS public key");
        assert_eq!(
            checker_err.root_cause().to_string(),
            "DICE chain of foo has an unknown UDS Pub."
        );

        Ok(())
    }
}
