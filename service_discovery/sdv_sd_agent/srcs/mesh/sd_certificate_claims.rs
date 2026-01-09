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
use coset::{CborSerializable, CoseSign1};
use crypto_rpc::cert::{
    set_digital_signature_key_usage, set_indefinite_validity_period, set_issuer_and_subject,
    set_subject_public_key, set_v3, sign_certificate, to_x509_common_name,
    x509_common_name_to_string,
};
use der::Encode;
use foreign_types_shared::{ForeignType, ForeignTypeRef};
use openssl::{
    asn1::{Asn1Object, Asn1OctetString},
    pkey::{PKey, Private, Public},
    x509::{GeneralName, X509Builder, X509},
};
use sdv_comms_id::names::VmName;
use std::cmp::Ordering;
use std::str::FromStr;

/// Randomly chosen OIDs. The "1.3.6.1.4.1" prefix denotes "private enterprise" and is commonly
/// used for custom OIDs (https://oid-rep.orange-labs.fr/get/1.3.6.1.4.1 for details).
///
/// "819149455" is a randomly generated 32 bit integer uniquely identifying "the enterprise", that
/// doesn't correspond to any registered enterprise (https://www.iana.org/assignments/enterprise-numbers/?q=819149455).
/// Note: there are two "Google" entries in that registry (https://www.iana.org/assignments/enterprise-numbers/?q=Google).
const SECURE_WORLD_DICE_CHAIN_OID: &str = "1.3.6.1.4.1.819149455.1";
const ANDROID_SDV_DICE_CHAIN_OID: &str = "1.3.6.1.4.1.819149455.2";
const ANDROID_SDV_SIGNER_CERT_OID: &str = "1.3.6.1.4.1.819149455.3";
const SECURE_WORLD_SIGNER_CERT_OID: &str = "1.3.6.1.4.1.819149455.4";
const UDS_CERTS_OID: &str = "1.3.6.1.4.1.819149455.5";

/// Prefix of a certificate's subject to indicate it identifies a service discovery agent.
const SD_PREFIX: &str = "SD_";

/// This corresponds to GEN_OTHERNAME constant defined in external/boringssl/src/include/openssl/x509.h.
const GEN_OTHERNAME: i32 = 0;

/// This corresponds to V_ASN1_OCTET_STRING constant defined in external/boringssl/src/include/openssl/asn1.h.
const V_ASN1_OCTET_STRING: i32 = 4;

/// The claims of the X.509 self-signed certificate of a service discovery agent.
#[derive(Debug)]
pub(crate) struct SdCertificateClaims {
    /// The Android SDV DICE chain
    pub android_sdv: Vec<u8>,

    /// Certificate for a signing key, signed by the CDI_Leaf of the Android SDV DICE chain
    pub android_sdv_signer_certificate: CoseSign1,

    /// A Certificate Signing Request from KeyMint's IRemotelyProvisionedCompoenent (RKP), which
    /// contains the Secure World DICE chain
    pub rkp_csr: Vec<u8>,

    /// The bytes of a DER-encoded X.509 certificate for the subject public key,
    /// signed by the attestation key in the RKP CSR.
    pub secure_world_signer_certificate_der: Vec<u8>,

    /// The subject public key of the X.509 certificate
    pub subject_public_key: PKey<Public>,

    /// Certificates for UDS (Unique Device Secret) Public Keys.
    pub uds_certs_cbor: Option<Vec<u8>>,

    /// The name of the SDV VM instance where the service discovery agent is running.
    /// As stated in the certificate's subject.
    pub vm_name: VmName,
}

fn retrieve_other_name_value(
    names: &openssl::stack::Stack<GeneralName>,
    oid: &str,
) -> anyhow::Result<Vec<u8>> {
    for name in names {
        // SAFETY: The pointer is valid throughout the function,
        // since it's owned by `names`, therefore it's safe to access its fields here.
        // No modifications to the object stored at `name` are made.
        let name = unsafe { *name.as_ptr() };

        if name.type_ != GEN_OTHERNAME {
            continue;
        }

        // SAFETY: otherName field must be populated, as the type is equal to GEN_OTHERNAME.
        let other_name = unsafe { *name.d.otherName };
        // SAFETY: Object created from this pointer is not modified and not dropped.
        let oid_obj = unsafe { Asn1Object::from_ptr(other_name.type_id) };
        let found = oid_obj.to_string() == oid;
        // Call mem::forget to avoid double-free, since this object
        // is owned by `name`.
        std::mem::forget(oid_obj);

        if !found {
            continue;
        }

        // SAFETY: Value field must be populated.
        let value = unsafe { *other_name.value };

        if value.type_ != V_ASN1_OCTET_STRING {
            bail!("The value's type is not OCTET_STRING");
        }

        // SAFETY: octet_string field should be populated, since `type_` is octet string.
        // Object created from this pointer is not modified and not dropped.
        let octet_string = unsafe { Asn1OctetString::from_ptr(value.value.octet_string) };
        let result_vec = octet_string.as_slice().to_vec();
        // Call mem::forget to avoid double-free, since this object
        // is owned by `name`.
        std::mem::forget(octet_string);

        return Ok(result_vec);
    }
    Err(anyhow!("Other name with oid {} not found", oid))
}

impl SdCertificateClaims {
    pub fn from_x509(x509: &X509) -> anyhow::Result<Self> {
        if x509.issuer_name().try_cmp(x509.subject_name())? != Ordering::Equal {
            bail!("issuer name != subject name");
        }

        let subject_public_key = x509.public_key()?;

        if !x509.verify(&subject_public_key)? {
            bail!("Certificate is not self-signed");
        }

        let subject_alt_names = x509.subject_alt_names();
        let subject_alt_names = match subject_alt_names {
            Some(val) => val,
            None => bail!("SANs of the certificate are empty"),
        };

        let android_sdv: Vec<u8> =
            retrieve_other_name_value(&subject_alt_names, ANDROID_SDV_DICE_CHAIN_OID)?;
        let rkp_csr: Vec<u8> =
            retrieve_other_name_value(&subject_alt_names, SECURE_WORLD_DICE_CHAIN_OID)?;
        let secure_world_signer_certificate_der: Vec<u8> =
            retrieve_other_name_value(&subject_alt_names, SECURE_WORLD_SIGNER_CERT_OID)?;

        let android_sdv_signer_cert_bytes: Vec<u8> =
            retrieve_other_name_value(&subject_alt_names, ANDROID_SDV_SIGNER_CERT_OID)?;
        let android_sdv_signer_certificate = CoseSign1::from_slice(&android_sdv_signer_cert_bytes)?;

        let uds_certs_cbor = retrieve_other_name_value(&subject_alt_names, UDS_CERTS_OID);

        let uds_certs_cbor = match uds_certs_cbor {
            Ok(certs) => Some(certs),
            Err(err) => {
                log::warn!("Failed to retrieve UDS certificates from OtherName: {err}");
                None
            }
        };

        let vm_name = vm_name_from_subject_name(&x509_common_name_to_string(x509.subject_name())?)?;

        Ok(Self {
            android_sdv,
            android_sdv_signer_certificate,
            rkp_csr,
            secure_world_signer_certificate_der,
            subject_public_key,
            uds_certs_cbor,
            vm_name,
        })
    }

    pub fn to_x509(&self, private_key: &PKey<Private>) -> anyhow::Result<X509> {
        let subject = to_x509_common_name(&self.subject_name())?;
        let issuer = &subject;

        Ok(X509Builder::new()
            .context("Failed creating an X509 cert_builder")
            .and_then(set_v3)
            .and_then(set_digital_signature_key_usage)
            .and_then(set_indefinite_validity_period)
            .and_then(|cert_builder| set_subject_public_key(cert_builder, &self.subject_public_key))
            .and_then(|cert_builder| set_issuer_and_subject(cert_builder, issuer, &subject))
            .and_then(|cert_builder| self.add_subject_alternative_names(cert_builder))
            .and_then(|cert_builder| sign_certificate(cert_builder, private_key))?
            .build())
    }

    pub fn subject_name(&self) -> String {
        subject_name_from_vm_name(&self.vm_name)
    }

    fn add_subject_alternative_names(
        &self,
        mut cert_builder: X509Builder,
    ) -> anyhow::Result<X509Builder> {
        let ctx = cert_builder.x509v3_context(None, None);
        let mut san = openssl::x509::extension::SubjectAlternativeName::new();
        san.other_name2(
            Asn1Object::from_str(SECURE_WORLD_DICE_CHAIN_OID)?,
            der_encode(self.rkp_csr.as_slice())?.as_slice(),
        )
        .other_name2(
            Asn1Object::from_str(ANDROID_SDV_DICE_CHAIN_OID)?,
            der_encode(self.android_sdv.as_slice())?.as_slice(),
        )
        .other_name2(
            Asn1Object::from_str(ANDROID_SDV_SIGNER_CERT_OID)?,
            der_encode(&self.android_sdv_signer_certificate.clone().to_vec()?)?.as_slice(),
        )
        .other_name2(
            Asn1Object::from_str(SECURE_WORLD_SIGNER_CERT_OID)?,
            der_encode(self.secure_world_signer_certificate_der.as_slice())?.as_slice(),
        );

        if let Some(certs) = &self.uds_certs_cbor {
            san.other_name2(
                Asn1Object::from_str(UDS_CERTS_OID)?,
                der_encode(certs.as_slice())?.as_slice(),
            );
        }

        let san = san.build(&ctx)?;
        cert_builder.append_extension(san)?;
        Ok(cert_builder)
    }
}

fn vm_name_from_subject_name(subject_name: &str) -> anyhow::Result<VmName> {
    let vm_name_str = subject_name
        .strip_prefix(SD_PREFIX)
        .ok_or(anyhow!("Subject name \"{subject_name}\" does not start with \"{SD_PREFIX}\""))?;

    Ok(VmName::from_str(vm_name_str)?)
}

pub(crate) fn subject_name_from_vm_name(vm_name: &VmName) -> String {
    format!("{SD_PREFIX}{vm_name}")
}

fn der_encode(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    der::asn1::OctetString::new(bytes)
        .and_then(|octet_string| octet_string.to_der())
        .or_else(|_| bail!("Creating OctetString"))
}

/// Creates an SdCertificateClaims filled with default values.
#[cfg(test)]
pub(crate) fn create_default() -> anyhow::Result<SdCertificateClaims> {
    use crypto_rpc::cert::{build_ec_private_key, public_from_private};

    let private_key = build_ec_private_key()?.try_into()?;
    let subject_public_key = public_from_private(&private_key)?;

    Ok(SdCertificateClaims {
        android_sdv: Default::default(),
        android_sdv_signer_certificate: Default::default(),
        rkp_csr: Default::default(),
        secure_world_signer_certificate_der: Default::default(),
        subject_public_key,
        uds_certs_cbor: None,
        vm_name: "instance1".to_owned().try_into()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto_rpc::cert::{build_ec_private_key, public_from_private};
    use vvmtruststore::SignerCertificateClaims;

    fn check_same_cose_sign1(one: &CoseSign1, other: &CoseSign1) -> anyhow::Result<()> {
        // A deserialized CoseSign1 will never match a CoseSign1 that is created
        // by signing a payload at runtime due to CoseSign1::protected::original_data.
        // the former will have Some(CBOR bytes) and the latter None.
        // Hence the need for our own comparison function.

        if one.payload != other.payload {
            bail!("Mismatching payloads");
        }

        if one.signature != other.signature {
            bail!("Mismatching signatures");
        }

        if one.protected.header != other.protected.header {
            bail!("Mismatching protected headers");
        }

        if one.unprotected != other.unprotected {
            bail!("Mismatching unprotected headers");
        }

        Ok(())
    }

    fn check_same_sd_claims(
        one: &SdCertificateClaims,
        other: &SdCertificateClaims,
    ) -> anyhow::Result<()> {
        if one.android_sdv != other.android_sdv {
            bail!("Mismatching android_sdv");
        }

        check_same_cose_sign1(
            &one.android_sdv_signer_certificate,
            &other.android_sdv_signer_certificate,
        )
        .context("Mismatching android_sdv_signer_certificate")?;

        if one.rkp_csr != other.rkp_csr {
            bail!("Mismatching rkp_csr");
        }

        if one.secure_world_signer_certificate_der != other.secure_world_signer_certificate_der {
            bail!("Mismatching secure_world_signer_certificate_der");
        }

        let one_pub_key_pem = one.subject_public_key.public_key_to_pem()?;
        let other_pub_key_pem = other.subject_public_key.public_key_to_pem()?;
        if one_pub_key_pem != other_pub_key_pem {
            bail!("Mismatching subject_public_key");
        }

        if one.uds_certs_cbor != other.uds_certs_cbor {
            bail!("Mismatching UDS certificates");
        }

        if one.vm_name != other.vm_name {
            bail!("Mismatching vm_name: {} != {}", one.vm_name, other.vm_name);
        }

        Ok(())
    }

    fn create_x509_with_add_sans_fn(
        add_sans_fn: impl Fn(X509Builder, &SdCertificateClaims) -> anyhow::Result<X509Builder>,
    ) -> anyhow::Result<X509> {
        let private_key = build_ec_private_key()?.try_into()?;
        let subject_public_key = public_from_private(&private_key)?;

        let sd_claims = SdCertificateClaims {
            android_sdv: vec![43u8; 20 * 1024],
            android_sdv_signer_certificate: CoseSign1::default(),
            rkp_csr: vec![42u8; 20 * 1024],
            secure_world_signer_certificate_der: vec![42u8; 20 * 1024],
            subject_public_key,
            uds_certs_cbor: Some(vec![44u8; 20 * 1024]),
            vm_name: "instance1".to_owned().try_into()?,
        };

        let subject = to_x509_common_name(&sd_claims.subject_name())?;
        let issuer = &subject;

        Ok(X509Builder::new()
            .context("Failed creating an X509 cert_builder")
            .and_then(set_v3)
            .and_then(set_digital_signature_key_usage)
            .and_then(set_indefinite_validity_period)
            .and_then(|cert_builder| {
                set_subject_public_key(cert_builder, &sd_claims.subject_public_key)
            })
            .and_then(|cert_builder| set_issuer_and_subject(cert_builder, issuer, &subject))
            .and_then(|cert_builder| add_sans_fn(cert_builder, &sd_claims))
            .and_then(|cert_builder| sign_certificate(cert_builder, &private_key))?
            .build())
    }

    /// Creates a well formed certificate.
    /// It's not tied to the Android SDV DICE chain. But it's enough for our tests.
    fn create_test_signer_cert(subject_public_key: &PKey<Public>) -> anyhow::Result<CoseSign1> {
        let issuer_private_key: PKey<Private> = build_ec_private_key()?.try_into()?;

        SignerCertificateClaims {
            issuer: "issuer".to_owned(),
            subject: "subject".to_owned(),
            subject_public_key: subject_public_key.clone().try_into()?,
        }
        .sign(&issuer_private_key)
    }

    #[test]
    fn vm_name_from_subject_name_ok() -> anyhow::Result<()> {
        let vm_name = vm_name_from_subject_name(&format!("{SD_PREFIX}foobar"))?;
        assert_eq!(vm_name, VmName::from_str("foobar")?);
        Ok(())
    }

    #[test]
    fn vm_name_from_subject_name_missing_prefix_err() -> anyhow::Result<()> {
        let result = vm_name_from_subject_name("foobar");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn to_and_from_x509_ok() -> anyhow::Result<()> {
        let private_key = build_ec_private_key()?.try_into()?;
        let subject_public_key = public_from_private(&private_key)?;

        let sd_claims = SdCertificateClaims {
            android_sdv: vec![43u8; 20 * 1024],
            android_sdv_signer_certificate: create_test_signer_cert(&subject_public_key)?,
            rkp_csr: vec![42u8; 20 * 1024],
            secure_world_signer_certificate_der: vec![42u8; 20 * 1024],
            subject_public_key,
            uds_certs_cbor: Some(vec![44u8; 20 * 1024]),
            vm_name: "instance1".to_owned().try_into()?,
        };

        let sd_certificate = sd_claims.to_x509(&private_key)?;

        let deserialized_sd_claims = SdCertificateClaims::from_x509(&sd_certificate)?;

        check_same_sd_claims(&sd_claims, &deserialized_sd_claims)?;

        Ok(())
    }

    #[test]
    // Check that empty string gets processed correctly without panic.
    fn to_and_from_x509_empty_string_ok() -> anyhow::Result<()> {
        let private_key = build_ec_private_key()?.try_into()?;
        let subject_public_key = public_from_private(&private_key)?;

        let sd_claims = SdCertificateClaims {
            android_sdv: vec![43u8; 0],
            android_sdv_signer_certificate: create_test_signer_cert(&subject_public_key)?,
            rkp_csr: vec![42u8; 0],
            secure_world_signer_certificate_der: vec![42u8; 20 * 1024],
            subject_public_key,
            uds_certs_cbor: Some(vec![44u8; 0]),
            vm_name: "instance1".to_owned().try_into()?,
        };

        let sd_certificate = sd_claims.to_x509(&private_key)?;

        let deserialized_sd_claims = SdCertificateClaims::from_x509(&sd_certificate)?;

        check_same_sd_claims(&sd_claims, &deserialized_sd_claims)?;

        Ok(())
    }

    #[test]
    fn secure_world_dice_chain_missing_err() -> anyhow::Result<()> {
        let x509 = create_x509_with_add_sans_fn(|mut cert_builder, sd_claims| {
            let ctx = cert_builder.x509v3_context(None, None);
            let san = openssl::x509::extension::SubjectAlternativeName::new()
                .other_name2(
                    Asn1Object::from_str(ANDROID_SDV_DICE_CHAIN_OID)?,
                    der_encode(sd_claims.android_sdv.as_slice())?.as_slice(),
                )
                .build(&ctx)?;
            cert_builder.append_extension(san)?;
            Ok(cert_builder)
        })?;

        let err =
            SdCertificateClaims::from_x509(&x509).expect_err("from_x509() should have failed");

        assert_eq!(
            err.to_string(),
            format!("Other name with oid {SECURE_WORLD_DICE_CHAIN_OID} not found")
        );

        Ok(())
    }

    #[test]
    fn san_wrong_extension_type_err() -> anyhow::Result<()> {
        let asn_int_bytes = der::asn1::Int::new(&vec![42u8; 20 * 1024])
            .and_then(|asn_int| asn_int.to_der())
            .or_else(|_| bail!("Creating AsnInt"))?;

        let x509 = create_x509_with_add_sans_fn(|mut cert_builder, sd_claims| {
            let ctx = cert_builder.x509v3_context(None, None);
            let san = openssl::x509::extension::SubjectAlternativeName::new()
                .other_name2(
                    Asn1Object::from_str(ANDROID_SDV_DICE_CHAIN_OID)?,
                    der_encode(sd_claims.android_sdv.as_slice())?.as_slice(),
                )
                .other_name2(
                    Asn1Object::from_str(SECURE_WORLD_DICE_CHAIN_OID)?,
                    // Set asn1::Int object as value instead of OctetString.
                    // This should cause an error, since expected value for
                    // the DICE chain is OctetString.
                    asn_int_bytes.as_slice(),
                )
                .build(&ctx)?;
            cert_builder.append_extension(san)?;
            Ok(cert_builder)
        })?;

        let err =
            SdCertificateClaims::from_x509(&x509).expect_err("from_x509() should have failed");

        assert_eq!(err.to_string(), "The value's type is not OCTET_STRING");

        Ok(())
    }

    #[test]
    fn from_x509_not_self_signed_err() -> anyhow::Result<()> {
        let subject_private_key = build_ec_private_key()?.try_into()?;

        let sd_claims = SdCertificateClaims {
            android_sdv: vec![43u8; 20 * 1024],
            android_sdv_signer_certificate: CoseSign1::default(),
            rkp_csr: vec![42u8; 20 * 1024],
            secure_world_signer_certificate_der: vec![42u8; 20 * 1024],
            subject_public_key: public_from_private(&subject_private_key)?,
            uds_certs_cbor: Some(vec![44u8; 20 * 1024]),
            vm_name: "foobar".to_owned().try_into()?,
        };

        let subject = to_x509_common_name(&sd_claims.subject_name())?;
        let issuer = &subject;

        let other_private_key: PKey<Private> = build_ec_private_key()?.try_into()?;

        let x509 = X509Builder::new()
            .context("Failed creating an X509 cert_builder")
            .and_then(set_v3)
            .and_then(set_digital_signature_key_usage)
            .and_then(set_indefinite_validity_period)
            .and_then(|cert_builder| {
                set_subject_public_key(cert_builder, &sd_claims.subject_public_key)
            })
            .and_then(|cert_builder| set_issuer_and_subject(cert_builder, issuer, &subject))
            .and_then(|cert_builder| sd_claims.add_subject_alternative_names(cert_builder))
            // Signing with other_private_key instead of subject_private_key
            .and_then(|cert_builder| sign_certificate(cert_builder, &other_private_key))?
            .build();

        let err =
            SdCertificateClaims::from_x509(&x509).expect_err("from_x509() should have failed");

        assert_eq!(err.to_string(), "Certificate is not self-signed");

        Ok(())
    }

    #[test]
    fn test_der_encode() -> anyhow::Result<()> {
        // https://letsencrypt.org/docs/a-warm-welcome-to-asn1-and-der/#octet-string-encoding
        let bytes = [0x03, 0x02, 0x06, 0xA0];
        let der_encoded = der_encode(&bytes)?;
        assert_eq!(der_encoded, [0x04, 0x04, 0x03, 0x02, 0x06, 0xA0]);
        Ok(())
    }

    #[test]
    fn to_x509_ok() -> anyhow::Result<()> {
        let private_key = build_ec_private_key()?.try_into()?;
        let subject_public_key = public_from_private(&private_key)?;

        let sd_certificate = SdCertificateClaims {
            android_sdv: vec![43u8; 20 * 1024],
            android_sdv_signer_certificate: CoseSign1::default(),
            rkp_csr: vec![42u8; 20 * 1024],
            secure_world_signer_certificate_der: vec![42u8; 20 * 1024],
            subject_public_key,
            uds_certs_cbor: Some(vec![44u8; 20 * 1024]),
            vm_name: "instance1".to_owned().try_into()?,
        }
        .to_x509(&private_key)?;

        let sd_cert_pem = String::from_utf8(sd_certificate.to_pem()?)?;
        assert!(sd_cert_pem.starts_with("-----BEGIN CERTIFICATE-----\n"));
        assert!(sd_cert_pem.ends_with("-----END CERTIFICATE-----\n"));

        Ok(())
    }
}
