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

//! Generates X509 UDS certificate chains for testing.

use anyhow::{anyhow, Context};
use crypto_rpc::cert::{
    public_from_private, set_cert_sign_key_usage, set_indefinite_validity_period,
    set_issuer_and_subject, set_subject_public_key, set_v3, sign_certificate, to_x509_common_name,
};
use diced_open_dice::Cdi;
use openssl::pkey::{PKey, Private, Public};
use openssl::x509::{X509Builder, X509Name, X509NameRef, X509};
use test_dice_artifacts_builder::uds_public_key_from_uds;
use vvmtruststore::ec_or_okp_cose_key_to_pkey;

/// This UDS_ID was taken from a sample SDV DICE chain's CDI0 certificate's issuer field.
/// TODO: b/367222266 - derive this value from the provided UDS.
pub const AN_EXAMPLE_UDS_ID: &str = "68bd77a128ddcc51bba9a65ab2582f3281a0a0e5";

/// Generates an EC P256 private key.
pub fn generate_ec_p256_key() -> anyhow::Result<PKey<Private>> {
    use openssl::ec::{EcGroup, EcKey};
    use openssl::nid::Nid;
    let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
    let ec_key = EcKey::generate(&group)?;
    let pkey = PKey::from_ec_key(ec_key)?;
    Ok(pkey)
}

/// Constructs a UDS certificate chain from the provided secrets. Should only be used in test code.
///
/// * ca_private_keys: An array containing the private keys of the UDS CAs starting from the root
///   CA. It must contain at least one element: the UDS root CA private key.
/// * uds: the UDS value that is used to construct the UDS certificate to be certified by the UDS
///   CAs.
pub fn create_x509_uds_cert_chain_from_pkey_secrets(
    ca_private_keys: &[PKey<Private>],
    uds: &Cdi,
) -> anyhow::Result<Vec<X509>> {
    create_x509_uds_cert_chain_for_uds_pub_from_pkey_secrets(
        ca_private_keys,
        &uds_public_pkey_from_uds(uds)?,
    )
}

/// Constructs a UDS certificate chain from the provided secrets. Should only be used in test code.
///
/// * ca_private_keys: An array containing the private keys of the UDS CAs starting from the root
///   CA. It must contain at least one element: the UDS root CA private key.
/// * uds_pub: the UDS public key that is used to construct the UDS certificate to be certified by
///   the UDS CAs.
pub fn create_x509_uds_cert_chain_for_uds_pub_from_pkey_secrets(
    ca_private_keys: &[PKey<Private>],
    uds_pub: &PKey<Public>,
) -> anyhow::Result<Vec<X509>> {
    let (root_ca_private_key, intermediate_private_keys) =
        ca_private_keys.split_first().ok_or(anyhow!("there must be at least a single CA"))?;

    let mut uds_certificate_chain = vec![];

    let ca_name = "Google CA";
    let root_ca_public_key = public_from_private(root_ca_private_key)?;
    let root_cert = UdsRootCaCertificateBuilder::new()
        .set_private_key(root_ca_private_key)
        .set_public_key(&root_ca_public_key)
        .set_ca_name(ca_name)?
        .build()?;
    uds_certificate_chain.push(root_cert.clone());

    let mut previous_private_key = root_ca_private_key;
    let mut previous_cert = root_cert;
    let mut ca_index = 1;
    for intermediate_ca_private_key in intermediate_private_keys {
        let intermediate_ca_public_key = public_from_private(intermediate_ca_private_key)?;
        let intermediate_ca_cert = UdsIntermediateCaCertificateBuilder::new()
            .set_issuer_private_key(previous_private_key)
            .set_issuer_subject_name(previous_cert.subject_name())
            .set_subject_public_key(&intermediate_ca_public_key)
            .set_subject_name(&format!("Intermediate CA {ca_index}"))?
            .build()?;
        uds_certificate_chain.push(intermediate_ca_cert.clone());

        ca_index += 1;
        previous_cert = intermediate_ca_cert;
        previous_private_key = intermediate_ca_private_key;
    }

    let uds_cert = UdsCertificateBuilder::new()
        .set_issuer_private_key(previous_private_key)
        .set_issuer_subject_name(previous_cert.subject_name())
        .set_subject_public_key(uds_pub)
        .set_uds_id(AN_EXAMPLE_UDS_ID)?
        .build()?;

    uds_certificate_chain.push(uds_cert);

    Ok(uds_certificate_chain)
}

/// Extracts and converts the UDS public key to an OpenSSL PKey.
///
/// * `uds` - Compound Device Identifier.
pub fn uds_public_pkey_from_uds(uds: &Cdi) -> anyhow::Result<PKey<Public>> {
    ec_or_okp_cose_key_to_pkey(&uds_public_key_from_uds(uds)?)
}

/// Represents a validity period for a certificate.
///
/// The `not_before` and `not_after` fields should be in the ASN1 UTCTIME format, e.g.,
/// "YYMMDDHHMMSSZ".
pub struct ValidityPeriod {
    /// The start of the validity period.
    pub not_before: String,
    /// The end of the validity period.
    pub not_after: String,
}

/// A builder for creating UDS root CA certificates.
#[derive(Default)]
pub struct UdsRootCaCertificateBuilder<'a> {
    private_key: Option<&'a PKey<Private>>,
    public_key: Option<&'a PKey<Public>>,
    ca_name: Option<X509Name>,
    validity: Option<ValidityPeriod>,
}

impl<'a> UdsRootCaCertificateBuilder<'a> {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the private key that will sign this certificate.
    pub fn set_private_key(mut self, private_key: &'a PKey<Private>) -> Self {
        self.private_key = Some(private_key);
        self
    }

    /// Set the subject public key.
    pub fn set_public_key(mut self, public_key: &'a PKey<Public>) -> Self {
        self.public_key = Some(public_key);
        self
    }

    /// Set the subject name and issuer name for the certificate.
    pub fn set_ca_name(mut self, ca_name: &str) -> anyhow::Result<Self> {
        self.ca_name = Some(to_x509_common_name(ca_name)?);
        Ok(self)
    }

    /// Set the validity period for the certificate.
    /// If not set then an indefinite validity period is used.
    pub fn set_validity_period(mut self, validity: ValidityPeriod) -> Self {
        self.validity = Some(validity);
        self
    }

    /// Build a self-signed (aka root) certificate for a root CA of UDS certificates.
    pub fn build(&self) -> anyhow::Result<X509> {
        let issuer = self.ca_name.as_ref().ok_or(anyhow!("ca name must be set"))?;
        let subject = issuer;
        let public_key = self.public_key.ok_or(anyhow!("public key must be set"))?;
        let private_key = self.private_key.ok_or(anyhow!("private key must be set"))?;

        Ok(X509Builder::new()
            .context("Failed to create an X509Builder")
            .and_then(set_v3)
            .and_then(set_cert_sign_key_usage)
            .and_then(|cert_builder| set_validity_period(cert_builder, &self.validity))
            .and_then(|cert_builder| set_subject_public_key(cert_builder, public_key))
            .and_then(|cert_builder| set_issuer_and_subject(cert_builder, issuer, subject))
            .and_then(|cert_builder| sign_certificate(cert_builder, private_key))?
            .build())
    }
}

/// A builder for creating UDS intermediate CA certificates.
#[derive(Default)]
pub struct UdsIntermediateCaCertificateBuilder<'a> {
    issuer_private_key: Option<&'a PKey<Private>>,
    issuer_subject_name: Option<&'a X509NameRef>,
    subject_public_key: Option<&'a PKey<Public>>,
    subject_name: Option<X509Name>,
}

impl<'a> UdsIntermediateCaCertificateBuilder<'a> {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the issuer's private key.
    pub fn set_issuer_private_key(mut self, issuer_private_key: &'a PKey<Private>) -> Self {
        self.issuer_private_key = Some(issuer_private_key);
        self
    }

    /// Set the issuer's subject name.
    pub fn set_issuer_subject_name(mut self, issuer_subject_name: &'a X509NameRef) -> Self {
        self.issuer_subject_name = Some(issuer_subject_name);
        self
    }

    /// Set the subject public key of the intermediate certificate.
    pub fn set_subject_public_key(mut self, subject_public_key: &'a PKey<Public>) -> Self {
        self.subject_public_key = Some(subject_public_key);
        self
    }

    /// Set the subject name of the intermediate certificate.
    pub fn set_subject_name(mut self, subject_name: &str) -> anyhow::Result<Self> {
        self.subject_name = Some(to_x509_common_name(subject_name)?);
        Ok(self)
    }

    /// Build a certificate for an intermediate CA of UDS certificates.
    pub fn build(&self) -> anyhow::Result<X509> {
        let subject = self.subject_name.as_ref().ok_or(anyhow!("subject name must be set"))?;
        let issuer_subject_name =
            self.issuer_subject_name.ok_or(anyhow!("issuer subject name must be set"))?;
        let subject_public_key =
            self.subject_public_key.ok_or(anyhow!("subject public key must be set"))?;
        let issuer_private_key =
            self.issuer_private_key.ok_or(anyhow!("issuer private key must be set"))?;

        Ok(X509Builder::new()
            .context("Failed to create an X509Builder")
            .and_then(set_v3)
            .and_then(set_cert_sign_key_usage)
            .and_then(set_ca_basicconstraint)
            .and_then(set_indefinite_validity_period)
            .and_then(|cert_builder| set_subject_public_key(cert_builder, subject_public_key))
            .and_then(|cert_builder| {
                set_issuer_and_subject(cert_builder, issuer_subject_name, subject)
            })
            .and_then(|cert_builder| sign_certificate(cert_builder, issuer_private_key))?
            .build())
    }
}

/// A builder for creating leaf UDS certificates.
#[derive(Default)]
pub struct UdsCertificateBuilder<'a> {
    issuer_private_key: Option<&'a PKey<Private>>,
    issuer_subject_name: Option<&'a X509NameRef>,
    subject_public_key: Option<&'a PKey<Public>>,
    subject_name: Option<X509Name>,
    validity: Option<ValidityPeriod>,
}

impl<'a> UdsCertificateBuilder<'a> {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the issuer's private key.
    pub fn set_issuer_private_key(mut self, issuer_private_key: &'a PKey<Private>) -> Self {
        self.issuer_private_key = Some(issuer_private_key);
        self
    }

    /// Set the issuer's subject name.
    pub fn set_issuer_subject_name(mut self, issuer_subject_name: &'a X509NameRef) -> Self {
        self.issuer_subject_name = Some(issuer_subject_name);
        self
    }

    /// Set the UDS public key.
    pub fn set_subject_public_key(mut self, subject_public_key: &'a PKey<Public>) -> Self {
        self.subject_public_key = Some(subject_public_key);
        self
    }

    /// Set the UDS ID.
    pub fn set_uds_id(mut self, uds_id: &str) -> anyhow::Result<Self> {
        self.subject_name = Some(to_x509_common_name(format!("SERIALNUMBER={uds_id}").as_str())?);
        Ok(self)
    }

    /// Set the validity period for the certificate.
    /// If not set then an indefinite validity period is used.
    pub fn set_validity_period(mut self, validity: ValidityPeriod) -> Self {
        self.validity = Some(validity);
        self
    }

    /// Builds a leaf UDS certificate.
    pub fn build(&self) -> anyhow::Result<X509> {
        let issuer = self.issuer_subject_name.ok_or(anyhow!("issuer subject name must be set"))?;
        let subject = self.subject_name.as_ref().ok_or(anyhow!("subject name must be set"))?;
        let subject_public_key =
            self.subject_public_key.ok_or(anyhow!("subject public key must be set"))?;
        let issuer_private_key =
            self.issuer_private_key.ok_or(anyhow!("issuer private key must be set"))?;

        Ok(X509Builder::new()
            .context("Failed to create an X509Builder")
            .and_then(set_v3)
            .and_then(set_indefinite_validity_period)
            .and_then(|cert_builder| set_subject_public_key(cert_builder, subject_public_key))
            .and_then(|cert_builder| set_issuer_and_subject(cert_builder, issuer, subject))
            .and_then(|cert_builder| set_validity_period(cert_builder, &self.validity))
            .and_then(|cert_builder| sign_certificate(cert_builder, issuer_private_key))?
            .build())
    }
}

fn set_ca_basicconstraint(mut cert_builder: X509Builder) -> anyhow::Result<X509Builder> {
    use openssl::x509::extension::BasicConstraints;
    let basicconstraints_extension =
        BasicConstraints::new().ca().build().context("Create BasicConstraints extension")?;
    cert_builder
        .append_extension(basicconstraints_extension)
        .context("Append BasicConstraints extension")?;
    Ok(cert_builder)
}

fn set_validity_period(
    mut cert_builder: X509Builder,
    validity: &Option<ValidityPeriod>,
) -> anyhow::Result<X509Builder> {
    use openssl::asn1::Asn1Time;
    use std::borrow::Borrow;
    match validity {
        Some(validity) => {
            cert_builder
                .set_not_before(
                    Asn1Time::from_str(&validity.not_before)
                        .context("Asn1 time creation")?
                        .borrow(),
                )
                .context("Valid not before")?;
            cert_builder
                .set_not_after(
                    Asn1Time::from_str(&validity.not_after).context("Asn1 time creation")?.borrow(),
                )
                .context("Valid not after")?;
            Ok(cert_builder)
        }
        None => set_indefinite_validity_period(cert_builder),
    }
}
