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

use anyhow::{anyhow, bail, Context};
use coset::CoseKey;
use crypto_rpc::cert::to_x509_common_name;
use der::asn1::{OctetStringRef, SetOfVec};
use der::Encode;
use dice_chain_verifier::VerifiedSdvDiceChain;
use hwtrust::dice::ChainForm;
use hwtrust::publickey::{EcKind, SignatureKind};
use hwtrust::rkp::Csr;
use hwtrust::session::{Options, Session};
use keystore_attestation::{AttestationApplicationId, PackageInfoRecord};
use openssl::pkey::{PKey, Private};
use vvmtruststore::pkey_kind;

use android_hardware_security_keymint::aidl::android::hardware::security::keymint::{
    Algorithm::Algorithm, AttestationKey::AttestationKey, Digest::Digest,
    IKeyMintDevice::BpKeyMintDevice, IKeyMintDevice::IKeyMintDevice, KeyFormat::KeyFormat,
    KeyParameter::KeyParameter, KeyParameterValue::KeyParameterValue, KeyPurpose::KeyPurpose,
    Tag::Tag,
};
use android_hardware_security_rkp::aidl::android::hardware::security::keymint::{
    IRemotelyProvisionedComponent::{
        BpRemotelyProvisionedComponent, IRemotelyProvisionedComponent,
    },
    MacedPublicKey::MacedPublicKey,
};

const SYSTEM_PROP_VERIFIED_BOOT_STATE: &str = "ro.boot.verifiedbootstate";
const GREEN: &str = "green";

// As opposed to the Android remote key provisioning procedure, during SDV authentication, the
// remote (within the vehicle) VM does not directly challenge KeyMint, so the challenge is unused.
const DUMMY_CHALLENGE: [u8; 3] = [1, 2, 3];

const SECURE_WORLD_ISSUER: &str = "SecureWorldAttestKey";

// Per RFC 5280 4.1.2.5, an undefined expiration (not-after) field should be set to
// GeneralizedTime 999912312359559, which is 253402300799000 ms from Jan 1, 1970.
const UNDEFINED_NOT_AFTER: i64 = 253402300799000i64;

/// A certificate signing request authenticated by the Secure World DICE chain.
pub(crate) struct AuthenticatedCsr {
    /// The Certificate Signing Request (CSR) in CBOR, as defined in
    /// generateCertificateRequestV2.cddl.
    ///
    /// It contains the Secure World DICE chain and the attestation public
    /// key signed by the CDI_Leaf_Priv of that DICE chain.
    ///
    /// This can be sent to peer VMs in the authentication procedure.
    pub cbor: Vec<u8>,

    /// Blob of the attestation key present in the CSR.
    /// Can be used to import a signing key whose certificate is signed by this attestation key.
    /// That signing key can then be used to sign challenges the SDV VM authentication procedure.
    pub attestation_key_blob: Vec<u8>,
}

/// Fetches the UDS public keys of the device this VM is running on.
pub(crate) fn fetch_local_vm_uds_pubs() -> anyhow::Result<Vec<CoseKey>> {
    let mut vm_uds_pubs = Vec::new();

    vm_uds_pubs.push(fetch_local_android_sdv_uds_pub()?);

    if let Some(key) = fetch_local_secure_world_uds_pub()? {
        if !vm_uds_pubs.contains(&key) {
            vm_uds_pubs.push(key);
        }
    }

    Ok(vm_uds_pubs)
}

fn fetch_local_android_sdv_uds_pub() -> anyhow::Result<CoseKey> {
    let android_sdv = get_android_dice_chain()?;
    uds_pub_from_cbor_android_chain(&android_sdv)
}

fn fetch_local_secure_world_uds_pub() -> anyhow::Result<Option<CoseKey>> {
    let cbor_bytes = AuthenticatedCsr::create()?.cbor;

    let maybe_uds_pub = match parse_and_verify_cbor_csr(&cbor_bytes) {
        Ok(csr) => Some(uds_pub_from_csr(&csr)?),
        Err(e) => {
            if has_locked_avb() {
                return Err(e);
            } else {
                // If AVB is unlocked or absent, parsing of the CSR may fail due to zeroed-out
                // mandatory fields.
                // Downgrade this error to a warning instead to avoid creating unnecessary
                // development friction. Real provisioning only happens with AVB locked.
                log::warn!("{e:#}");
                None
            }
        }
    };

    Ok(maybe_uds_pub)
}

/// Returns true if AVB is locked and false in any other situation,
/// including no AVB.
fn has_locked_avb() -> bool {
    matches!(rustutils::android::system_properties::read(SYSTEM_PROP_VERIFIED_BOOT_STATE),
         Ok(Some(value)) if value == GREEN)
}

pub(crate) fn uds_pub_from_cbor_android_chain(cbor: &[u8]) -> anyhow::Result<CoseKey> {
    let chain = VerifiedSdvDiceChain::from_cbor(cbor)?;

    chain.android_sdv_dice_chain.root_public_key().to_cose_key()
}

fn keymint_hwtrust_session() -> Session {
    use hwtrust::dice::ProfileVersion;
    use hwtrust::session::DiceProfileRange;
    // TODO: b/404916116 - require Android15 Profile version for the KeyMint DICE chain, and a
    //       non-degenerate DICE chain.
    Session {
        options: Options {
            dice_profile_range: DiceProfileRange::new(
                ProfileVersion::Android14,
                ProfileVersion::Android16,
            ),
            allow_any_mode: true,
            ..Default::default()
        },
    }
}

/// Parses and verifies the Certificate Signing Request (CSR) encoded in CBOR.
pub(crate) fn parse_and_verify_cbor_csr(cbor: &[u8]) -> anyhow::Result<Csr> {
    use hwtrust::rkp::Csr;
    // from_cbor() not just parses, but also verifies the CSR.
    Csr::from_cbor(&keymint_hwtrust_session(), cbor).context("Failed to parse the CSR from KeyMint")
}

/// Extracts the UDS_Public from a Certificate Signing Request (CSR).
pub(crate) fn uds_pub_from_csr(csr: &Csr) -> anyhow::Result<CoseKey> {
    match csr {
        Csr::V3 { dice_chain, .. } => match dice_chain {
            ChainForm::Proper(chain) => chain.root_public_key().to_cose_key(),
            ChainForm::Degenerate(deg_chain) => deg_chain.public_key().to_cose_key(),
        },
        _ => bail!("CSR V2 is not supported!"),
    }
}

pub(crate) fn get_android_dice_chain() -> anyhow::Result<Vec<u8>> {
    use google_sdv_init_open_dice::aidl::google::sdv::init_open_dice::IDiceChainProvider::{
        BpDiceChainProvider, IDiceChainProvider,
    };

    let dice_chain_provider = binder::wait_for_interface::<dyn IDiceChainProvider>(
        &(<BpDiceChainProvider as IDiceChainProvider>::get_descriptor().to_owned() + "/default"),
    )
    .context("Failed to connect to IDiceChainProvider service")?;

    dice_chain_provider
        .getAndroidDiceChain()
        .context("Failed to get the Android SDV DICE chain from the provider service")
}

fn get_default_keymint_device_service() -> anyhow::Result<binder::Strong<dyn IKeyMintDevice>> {
    let keymint_device_descriptor: &str = <BpKeyMintDevice as IKeyMintDevice>::get_descriptor();
    let default_keymint_device_service_name = format!("{keymint_device_descriptor}/default");

    binder::wait_for_interface(&default_keymint_device_service_name)
        .context("Failed to wait for IKeyMintDevice interface")
}

fn get_default_remotely_provisioned_component_service(
) -> anyhow::Result<binder::Strong<dyn IRemotelyProvisionedComponent>> {
    let remotely_provisioned_component_descriptor: &str =
        <BpRemotelyProvisionedComponent as IRemotelyProvisionedComponent>::get_descriptor();
    let default_remotely_provisioned_component_service_name =
        format!("{remotely_provisioned_component_descriptor}/default");

    binder::wait_for_interface(&default_remotely_provisioned_component_service_name)
        .context("Failed to wait for IRemotelyProvisionedComponent interface")
}

fn is_key_type_supported(private_key: &PKey<Private>) -> anyhow::Result<()> {
    match pkey_kind(private_key) {
        Some(SignatureKind::Ec(EcKind::P256)) => Ok(()),
        _ => Err(anyhow!("Only ECDSA P-256 private key is supported")),
    }
}

fn construct_attestation_app_id(subject: &str) -> anyhow::Result<AttestationApplicationId> {
    let package_info = PackageInfoRecord {
        package_name: OctetStringRef::new(subject.as_bytes()).map_err(|e| {
            anyhow!("Failed to create ASN.1 OCTET STRING from subject bytes: {e:?}")
        })?,
        version: 0,
    };
    let mut package_info_records = SetOfVec::new();
    package_info_records
        .insert(package_info)
        .map_err(|e| anyhow!("Failed to add PackageInfoRecord: {e:?}"))?;
    let signature_digests = SetOfVec::new();
    Ok(AttestationApplicationId { package_info_records, signature_digests })
}

// Constructs the key parameters array for the IKeyMintDevice::importKey() call,
// including the minimum required set and Tag::CERTIFICATE_SUBJECT.
fn construct_key_parameters(subject: &str) -> anyhow::Result<[KeyParameter; 8]> {
    Ok([
        KeyParameter { tag: Tag::ALGORITHM, value: KeyParameterValue::Algorithm(Algorithm::EC) },
        KeyParameter { tag: Tag::PURPOSE, value: KeyParameterValue::KeyPurpose(KeyPurpose::SIGN) },
        KeyParameter { tag: Tag::DIGEST, value: KeyParameterValue::Digest(Digest::SHA_2_256) },
        KeyParameter {
            tag: Tag::ATTESTATION_CHALLENGE,
            value: KeyParameterValue::Blob(Vec::from(DUMMY_CHALLENGE)),
        },
        KeyParameter {
            tag: Tag::ATTESTATION_APPLICATION_ID,
            value: KeyParameterValue::Blob(
                construct_attestation_app_id(subject)?
                    .to_der()
                    .map_err(|e| anyhow!("Failed to encode AttestationApplicationId: {e:?}"))?,
            ),
        },
        KeyParameter { tag: Tag::CERTIFICATE_NOT_BEFORE, value: KeyParameterValue::DateTime(0) },
        KeyParameter {
            tag: Tag::CERTIFICATE_NOT_AFTER,
            value: KeyParameterValue::DateTime(UNDEFINED_NOT_AFTER),
        },
        KeyParameter {
            tag: Tag::CERTIFICATE_SUBJECT,
            value: KeyParameterValue::Blob(to_x509_common_name(subject)?.to_der()?),
        },
    ])
}

impl AuthenticatedCsr {
    pub fn create() -> anyhow::Result<Self> {
        let remotely_provisioned_component = get_default_remotely_provisioned_component_service()?;

        let mut maced_attestation_pub = MacedPublicKey { macedKey: vec![] };

        let is_for_testing = false;
        let attestation_key_blob = remotely_provisioned_component
            .generateEcdsaP256KeyPair(is_for_testing, &mut maced_attestation_pub)?;

        // Ask KeyMint to generate a Certificate Signing Request (CSR) containing the attestation
        // public key we just created. This CSR is authenticated (signed) by the leaf CDI of the
        // Secure World DICE chain.
        let cbor = remotely_provisioned_component
            .generateCertificateRequestV2(&[maced_attestation_pub], &DUMMY_CHALLENGE)?;

        Ok(AuthenticatedCsr { cbor, attestation_key_blob })
    }

    /// Returns a DER encoded X.509 attestation certificate issued by KeyMint for the provided
    /// private_key. Only ECDSA P-256 private key is supported.
    pub fn attest(&self, private_key: &PKey<Private>, subject: &str) -> anyhow::Result<Vec<u8>> {
        is_key_type_supported(private_key)?;
        let key_parameters = construct_key_parameters(subject)?;
        let attestation_key = AttestationKey {
            keyBlob: self.attestation_key_blob.clone(),
            attestKeyParams: vec![],
            issuerSubjectName: to_x509_common_name(SECURE_WORLD_ISSUER)?.to_der()?,
        };

        let certificates = get_default_keymint_device_service()?
            .importKey(
                &key_parameters,
                KeyFormat::PKCS8,
                &private_key.private_key_to_pkcs8()?,
                Some(&attestation_key),
            )?
            .certificateChain;
        if certificates.len() != 1 {
            bail!("Exactly one certificate should be returned when importing the key.");
        }
        Ok(certificates
            .into_iter()
            .next()
            .ok_or(anyhow!("Failed to get the attestation certificate"))?
            .encodedCertificate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crypto_rpc::cert::{build_ec_private_key, public_from_private};
    use openssl::{asn1::Asn1Time, pkey::PKey, pkey::Private, x509::X509};

    const SUBJECT: &str = "test subject";

    #[test]
    fn test_authenticated_csr_attest_ec_p256_succeeds() -> anyhow::Result<()> {
        let csr = AuthenticatedCsr::create()?;
        let private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let public_key = public_from_private(&private_key)?;

        let attestation_cert = csr.attest(&private_key, SUBJECT)?;

        let attestation_x509 = X509::from_der(&attestation_cert)?;
        assert!(attestation_x509.public_key()?.public_eq(&public_key));
        assert!(format!("{:?}", attestation_x509.subject_name()).contains(SUBJECT));
        assert!(format!("{:?}", attestation_x509.issuer_name()).contains(SECURE_WORLD_ISSUER));
        assert!(attestation_x509.not_before().eq(&Asn1Time::from_unix(0)?));
        assert!(attestation_x509
            .not_after()
            .eq(&Asn1Time::from_unix((UNDEFINED_NOT_AFTER / 1000) as libc::time_t)?));

        Ok(())
    }

    #[test]
    fn test_authenticated_csr_attest_unsupported_key_fails() -> anyhow::Result<()> {
        let csr = AuthenticatedCsr::create()?;
        let private_key: PKey<Private> = PKey::generate_ed25519()?;

        let err = csr.attest(&private_key, SUBJECT).expect_err("csr.attest() should have failed");

        assert_eq!(err.to_string(), "Only ECDSA P-256 private key is supported");

        Ok(())
    }

    #[test]
    fn test_authenticated_csr_attest_empty_subject_fails() -> anyhow::Result<()> {
        let csr = AuthenticatedCsr::create()?;
        let private_key: PKey<Private> = build_ec_private_key()?.try_into()?;

        let err = csr.attest(&private_key, "").expect_err("csr.attest() should have failed");

        assert!(err.to_string().contains("STRING_TOO_SHORT"));

        Ok(())
    }
}
