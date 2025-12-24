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

//! Certificate utils for RPC

use anyhow::{anyhow, bail, Context, Result};
use openssl::{
    asn1::Asn1Time,
    ec::{EcGroup, EcKey},
    hash::{hash, MessageDigest},
    nid::Nid,
    pkey::{self, HasParams, PKey, PKeyRef, Private, Public},
    stack::Stack,
    x509::{
        extension::KeyUsage, X509Builder, X509Extension, X509Name, X509NameEntryRef, X509NameRef,
        X509Req, X509ReqBuilder, X509,
    },
};

use data_encoding::BASE32_DNSSEC;
use std::borrow::Borrow;

// The key of the "common name" standard attribute of x509 certificate's Subject field.
const COMMON_NAME_KEY: &str = "CN";

/// A very early point in time to be used as the "Not before" time of the generated certs to ensure
/// that they are effectively always usable.
const INDEFINITE_NOT_BEFORE_TIME: &str = "19700101000000Z";

/// The latest allowed "Not after" time to be used as the "Not after" time of the generated certs
/// to ensure that they effectively never expire.
/// See https://www.rfc-editor.org/rfc/rfc5280.html#section-4.1.2.5.
const INDEFINITE_NOT_AFTER_TIME: &str = "99991231235959Z";

/// Generates an EC private key and returns it PEM-encoded.
///
/// Note that its public key counterpart can be extracted from it.
pub fn generate_ec_private_key() -> Result<Vec<u8>> {
    build_ec_private_key()?.private_key_to_pem().context("Private key to PEM")
}

/// Builds a typed EC private key.
///
/// Note that its public key counterpart can be extracted from it.
pub fn build_ec_private_key() -> Result<EcKey<Private>> {
    let ec_group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).context("Init EC group")?;
    EcKey::generate(&ec_group).context("Generate EC private key")
}

/// Generates a self-signed (aka root) certificate for a given key.
///
/// * `private_key_pem` - The PEM-encoded private key that will sign this certificate.
///   The subject public key is also extracted from it.
/// * `subject_name` - The certificate's subject common name.
pub fn generate_root_certificate(private_key_pem: &[u8], subject_name: &str) -> Result<X509> {
    let private_key = PKey::private_key_from_pem(private_key_pem).context("Invalid PEM")?;

    build_root_certificate(&private_key, subject_name)
}

/// Builds a self-signed (aka root) certificate for a given key.
///
/// * `private_key` - The private key that will sign this certificate.
///   The subject public key is also extracted from it.
/// * `subject_name` - The certificate's subject common name.
pub fn build_root_certificate(private_key: &PKey<Private>, subject_name: &str) -> Result<X509> {
    let issuer = to_base32_x509_common_name(subject_name)?;
    let subject = &issuer;
    let public_key = public_from_private(private_key)?;

    Ok(X509Builder::new()
        .context("Failed creating an X509 cert_builder")
        .and_then(set_v3)
        .and_then(set_cert_sign_key_usage)
        .and_then(set_indefinite_validity_period)
        .and_then(|cert_builder| set_subject_public_key(cert_builder, &public_key))
        .and_then(|cert_builder| set_issuer_and_subject(cert_builder, &issuer, subject))
        .and_then(|cert_builder| {
            add_identity_as_subject_alt_name(
                cert_builder,
                subject_name,
                /*No issuer for root cert*/ None,
            )
        })
        .and_then(|cert_builder| sign_certificate(cert_builder, private_key))?
        .build())
}

/// Builds a leaf certificate that is signed by the given issuer.
/// Is a type safe function.
///
/// * `issuer_private_key` - The issuer's private key.
/// * `issuer_certificate` - The issuer's certificate.
/// * `subject_public_key` - The subject public key of the leaf certificate.
/// * `subject_name` - The subject name of the leaf certificate.
pub fn build_leaf_certificate(
    issuer_private_key: &PKey<Private>,
    issuer_certificate: &X509,
    subject_public_key: &PKey<Public>,
    subject_name: &str,
) -> Result<X509> {
    let issuer = issuer_certificate.subject_name();
    // TODO: Check if we need subject at all (b/381406776).
    let subject = to_base32_x509_common_name("FakeSubjectName")?;

    Ok(X509Builder::new()
        .context("Failed creating an X509 cert_builder")
        .and_then(set_v3)
        .and_then(set_digital_signature_key_usage)
        .and_then(set_indefinite_validity_period)
        .and_then(|cert_builder| set_subject_public_key(cert_builder, subject_public_key))
        .and_then(|cert_builder| set_issuer_and_subject(cert_builder, issuer, &subject))
        .and_then(|cert_builder| {
            add_identity_as_subject_alt_name(cert_builder, subject_name, Some(issuer_certificate))
        })
        .and_then(|cert_builder| sign_certificate(cert_builder, issuer_private_key))?
        .build())
}

/// Gets the public key of a given private key.
pub fn public_from_private(private_key: &PKey<Private>) -> Result<PKey<Public>> {
    let ec_key = private_key.ec_key()?;
    let public_key = PKey::from_ec_key(
        EcKey::from_public_key(
            EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).context("Init EC group")?.borrow(),
            ec_key.public_key(),
        )
        .context("EcKey from EcPoint")?,
    )
    .context("PKey from EcKey")?;
    Ok(public_key)
}

/// Creates a PEM certificate chain with the given root and leaf certs.
pub fn chain_root_to_leaf(root_cert_pem: Vec<u8>, leaf_cert_pem: Vec<u8>) -> Vec<u8> {
    let mut root_cert_pem = root_cert_pem;
    // In a PEM file, the order is from leaf to root.
    let mut cert_chain_pem = leaf_cert_pem;
    cert_chain_pem.push(b'\n');
    cert_chain_pem.append(&mut root_cert_pem);

    cert_chain_pem
}

/// Sets issuer and subject "CommonName"s.
pub fn set_issuer_and_subject(
    mut cert_builder: X509Builder,
    issuer: &X509NameRef,
    subject: &X509NameRef,
) -> Result<X509Builder> {
    cert_builder.set_issuer_name(issuer)?;
    cert_builder.set_subject_name(subject)?;
    Ok(cert_builder)
}

/// Encodes identity hash to base32 encoded string.
/// Returns a string that is a valid DNS.
pub fn digest_identity_dns(identity: &str) -> Result<String> {
    let digest_bytes =
        hash(MessageDigest::sha256(), identity.as_bytes()).context("Failed to compute hash")?;
    Ok(BASE32_DNSSEC.encode(&digest_bytes).to_string())
}

/// Adds certificate subject's identity as a base32 encoded identity stored
/// in the DNS field of the Subject Alternative Name.
/// The identity is meant to be an FQIN, but it is not enforced and can be
/// any string.
/// Base32 encoding is done to convert FQIN (or any other input) into
/// a string that is more similar to a valid domain and that doesn't contain
/// semicolon as it may interpreted as ip:port delimiter by gRPCio.
/// Issuer should be None if it is a self signed certificate.
pub fn add_identity_as_subject_alt_name(
    mut cert_builder: X509Builder,
    identity: &str,
    issuer: Option<&X509>,
) -> Result<X509Builder> {
    let san_value = digest_identity_dns(identity)?;
    log::info!("Digest FQIN {} to {}", identity, &san_value);
    let ctx = cert_builder.x509v3_context(issuer.map(|v| v.as_ref()), None);

    let subject_alternative_name =
        openssl::x509::extension::SubjectAlternativeName::new().dns(&san_value).build(&ctx)?;

    cert_builder.append_extension(subject_alternative_name)?;
    Ok(cert_builder)
}

/// Extracts certificate subject's identity. It's stored in the DNS field of
/// the Subject Alternative Name.
/// The identity that is returned from this function is a hash. So it can only
/// be used to verify if it matches the expected identity.
fn extract_raw_identity(certificate: &X509) -> anyhow::Result<String> {
    let all_sans = certificate.subject_alt_names();

    let mut all_sans = match all_sans {
        Some(all_sans) => all_sans,
        None => bail!("SANs not found in the cert"),
    };

    if all_sans.len() != 1 {
        bail!("Too many SANs found. 1 SAN is expected.");
    }

    let san = all_sans.pop().unwrap();
    let digested_identity = match san.dnsname() {
        Some(dns_name) => dns_name.to_string(),
        None => bail!("SAN is not DNS"),
    };
    Ok(digested_identity)
}

/// Verifies that identity in the cert corresponds to the expected identity.
pub fn verify_identity(certificate: &X509, expected_identity: &str) -> Result<bool> {
    let cert_raw_identity = extract_raw_identity(certificate)?;
    let digested_expected_identity = digest_identity_dns(expected_identity)?;
    Ok(*cert_raw_identity.as_bytes() == *digested_expected_identity.as_bytes())
}

/// Verifies the given X.509 certification request.
///
/// Returns an error if verification fails for any reason.
pub fn verify_request(x509_req: &X509Req, expected_subject_name: &str) -> Result<()> {
    let unsecure_cert = request_to_cert_unsecure(x509_req)?;
    if !verify_identity(&unsecure_cert, expected_subject_name)? {
        bail!("Request has identity that doesn't correspond to {expected_subject_name}");
    }

    let subject_public_key = x509_req.public_key()?;
    verify_ec_key(&subject_public_key)?;

    if !x509_req.verify(&subject_public_key)? {
        bail!("Signature verification failed");
    }

    Ok(())
}

/// Extracts all the certificates and converts them into X509 from a string
/// of PEM encoded certs.
pub fn pems_to_x509(pem_data: &str) -> anyhow::Result<Vec<X509>> {
    X509::stack_from_pem(pem_data.as_bytes()).context("Conversion of PEMs to X509 failed")
}

/// Serializes into a list of PEM-encoded certificates.
pub fn x509_to_pems(certs: &Vec<X509>) -> anyhow::Result<String> {
    let mut all_certs_pem = Vec::new();

    for cert in certs {
        if !all_certs_pem.is_empty() {
            all_certs_pem.push(b'\n');
        }
        all_certs_pem.append(&mut cert.to_pem()?);
    }

    Ok(String::from_utf8(all_certs_pem)?)
}

/// Verifies the given key is an EC of the expected group
fn verify_ec_key<T: HasParams>(pkey: &PKeyRef<T>) -> Result<()> {
    match pkey.id() {
        pkey::Id::EC => match pkey.ec_key()?.group().curve_name() {
            Some(Nid::X9_62_PRIME256V1) => Ok(()),
            _ => Err(anyhow!("EC key with invalid curve")),
        },
        _ => Err(anyhow!("Not an EC key")),
    }
}

/// Builds an X.509 Certification Request for the given EC key and subject name.
pub fn build_certification_request(
    ec_key: &EcKey<Private>,
    subject_name: &str,
) -> anyhow::Result<X509Req> {
    let pkey = PKey::from_ec_key(ec_key.clone())?;

    let mut cert_req_builder = X509ReqBuilder::new()?;
    cert_req_builder.set_pubkey(&pkey)?;

    let v3_context = cert_req_builder.x509v3_context(None);
    let san_value = digest_identity_dns(subject_name)?;
    let subject_alternative_name = openssl::x509::extension::SubjectAlternativeName::new()
        .dns(&san_value)
        .build(&v3_context)?;

    let mut extension_stack: Stack<X509Extension> = Stack::new()?;
    extension_stack.push(subject_alternative_name)?;

    cert_req_builder.add_extensions(&extension_stack)?;

    cert_req_builder.sign(&pkey, MessageDigest::sha256())?;

    Ok(cert_req_builder.build())
}

/// Converts the X509Req into PEM
pub fn csr_to_pem(req: &X509Req) -> anyhow::Result<String> {
    Ok(String::from_utf8(req.to_pem().context("Failed to convert CSR to PEM")?)?)
}

// The result of this function is used to extract identity from the cert.
// Don't use it for any other purpose.
fn request_to_cert_unsecure(x509_req: &X509Req) -> Result<X509> {
    // Have to put the X509Extension of the request into a brand new certificate just to be able to
    // read the SAN in that extension.
    // Didn't find a way to read an X509Extension directly.

    let ec_key = build_ec_private_key()?;
    let pkey = PKey::from_ec_key(ec_key)?;
    let subject_public_key = public_from_private(&pkey)?;

    let issuer = to_x509_common_name("issuer")?;
    let subject = to_x509_common_name("subject")?;
    let mut cert_builder = X509Builder::new()
        .context("Failed creating an X509 cert_builder")
        .and_then(set_v3)
        .and_then(set_indefinite_validity_period)
        .and_then(|cert_builder| set_subject_public_key(cert_builder, &subject_public_key))
        .and_then(|cert_builder| set_issuer_and_subject(cert_builder, &issuer, &subject))?;

    const EXPECTED_NUM_EXTENSIONS: usize = 1;
    let mut extensions = x509_req.extensions()?;
    if extensions.len() != EXPECTED_NUM_EXTENSIONS {
        bail!(
            "Invalid number of extensions. Expected {EXPECTED_NUM_EXTENSIONS}, got {}",
            extensions.len()
        );
    }
    let extension: X509Extension =
        extensions.pop().ok_or(anyhow!("Failed to get request extension"))?;
    cert_builder.append_extension(extension)?;

    let cert = sign_certificate(cert_builder, &pkey)?.build();

    Ok(cert)
}

/// Sets the "KeyCertSign" bit in KeyUsage extension.
pub fn set_cert_sign_key_usage(mut cert_builder: X509Builder) -> Result<X509Builder> {
    let key_usage_extension =
        KeyUsage::new().key_cert_sign().build().context("Create KeyUsage extension")?;
    cert_builder.append_extension(key_usage_extension).context("Append KeyUsage extension")?;
    Ok(cert_builder)
}

/// Sets the digital signature bit in KeyUsage extension.
pub fn set_digital_signature_key_usage(mut cert_builder: X509Builder) -> Result<X509Builder> {
    let key_usage_extension =
        KeyUsage::new().digital_signature().build().context("Create KeyUsage extension")?;
    cert_builder.append_extension(key_usage_extension).context("Append KeyUsage extension")?;
    Ok(cert_builder)
}

/// Sets the public key.
pub fn set_subject_public_key(
    mut cert_builder: X509Builder,
    public_key: &PKey<Public>,
) -> Result<X509Builder> {
    cert_builder.set_pubkey(public_key).context("Setting public key")?;
    Ok(cert_builder)
}

/// Marks the x509 certificate as v3.
pub fn set_v3(mut cert_builder: X509Builder) -> Result<X509Builder> {
    cert_builder.set_version(2).context("Setting version 3")?;
    Ok(cert_builder)
}

/// Sets practically infinite validity period ("valid not before" and "valid not after") for the certificate.
pub fn set_indefinite_validity_period(mut cert_builder: X509Builder) -> Result<X509Builder> {
    cert_builder
        .set_not_before(
            Asn1Time::from_str(INDEFINITE_NOT_BEFORE_TIME).context("Asn1 time creation")?.borrow(),
        )
        .context("Valid not before")?;
    cert_builder
        .set_not_after(
            Asn1Time::from_str(INDEFINITE_NOT_AFTER_TIME).context("Asn1 time creation")?.borrow(),
        )
        .context("Valid not after")?;
    Ok(cert_builder)
}

/// Signs the certificate.
pub fn sign_certificate(
    mut cert_builder: X509Builder,
    private_key: &PKey<Private>,
) -> Result<X509Builder> {
    cert_builder.sign(private_key, MessageDigest::sha256()).context("Signing certificate")?;
    Ok(cert_builder)
}

/// Creates an `X509Name` from `str`.
pub fn to_x509_common_name(common_name: &str) -> anyhow::Result<X509Name> {
    let mut name_builder = X509Name::builder()?;
    name_builder.append_entry_by_text(COMMON_NAME_KEY, common_name)?;
    Ok(name_builder.build())
}

/// Creates a base32-encoded `X509Name` from `str`.
pub fn to_base32_x509_common_name(name: &str) -> Result<X509Name> {
    to_x509_common_name(&BASE32_DNSSEC.encode(name.as_bytes()))
}

/// Given an SDV VM instance name, returns the subject name of the service
/// discovery gRPC client or server running in that VM.
pub fn sd_subject_name(vm_name: &str) -> String {
    format!("service_discovery.{vm_name}")
}

/// Return an X.509 common name as a String.
pub fn x509_common_name_to_string(x509_name: &X509NameRef) -> anyhow::Result<String> {
    let common_name: &X509NameEntryRef = x509_name
        .entries()
        .find(|name_entry| name_entry.object().nid() == Nid::COMMONNAME)
        .ok_or(anyhow!("X.509 name does not have a \"common name\" entry."))?;

    Ok(common_name.data().as_utf8()?.to_string())
}

/// Returns a base32-encoded X.509 common name as a String.
pub fn base32_x509_common_name_to_string(x509_name: &X509NameRef) -> anyhow::Result<String> {
    let common_name: &X509NameEntryRef = x509_name
        .entries()
        .find(|name_entry| name_entry.object().nid() == Nid::COMMONNAME)
        .ok_or(anyhow!("X.509 name does not have a \"common name\" entry."))?;

    let decoded_bytes = BASE32_DNSSEC
        .decode(common_name.data().as_slice())
        .context("Common name is not base32-encoded")?;

    Ok(String::from_utf8(decoded_bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openssl::pkey::PKey;
    use rand::Rng;

    // The largest FQIN possible. 255 human-readable chars (256 if you count a null terminator).
    const LARGEST_FQIN: &str = "vvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvv:pppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppp.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/uuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuu";

    #[test]
    // Check that we can convert a cert signing request without subject name into a certificate.
    fn test_request_to_certificate() -> Result<()> {
        // Certificate signing request on service bundle side.
        let cert_signing_request =
            build_certification_request(&build_ec_private_key()?, LARGEST_FQIN)?;

        // Here service bundle sends a request to service discovery.

        // SD part.
        let issuer_priv_key = PKey::from_ec_key(build_ec_private_key()?)?;
        let issuer_name = "issuer_name";
        let issuer_certificate = build_root_certificate(&issuer_priv_key, issuer_name)?;

        // We assume that SD knows the FQIN of the requester.
        verify_request(&cert_signing_request, LARGEST_FQIN)?;

        // Check that SD can create a certificate and that identity there is
        // extractable and correct.
        let x509_certificate = build_leaf_certificate(
            &issuer_priv_key,
            &issuer_certificate,
            &cert_signing_request.public_key()?,
            LARGEST_FQIN,
        )?;

        assert!(verify_identity(&x509_certificate, LARGEST_FQIN)?);
        Ok(())
    }

    #[test]
    fn test_generate_ec_private_key() -> Result<()> {
        let private_key_pem = generate_ec_private_key()?;
        let private_key_pem = String::from_utf8(private_key_pem)?;
        assert!(private_key_pem.starts_with("-----BEGIN EC PRIVATE KEY-----\n"));
        assert!(private_key_pem.ends_with("-----END EC PRIVATE KEY-----\n"));
        Ok(())
    }

    #[test]
    fn test_self_signing_root_certificate() -> Result<()> {
        let private_key_pem = generate_ec_private_key()?;
        let vm_name = "instance1";
        let root_cert_pem = generate_root_certificate(&private_key_pem, vm_name)?.to_pem()?;
        let root_cert_pem = String::from_utf8(root_cert_pem)?;
        assert!(root_cert_pem.starts_with("-----BEGIN CERTIFICATE-----\n"));
        assert!(root_cert_pem.ends_with("-----END CERTIFICATE-----\n"));
        Ok(())
    }

    #[test]
    fn test_signing_leaf_certificate() -> Result<()> {
        let private_key = PKey::from_ec_key(build_ec_private_key()?)?;
        let vm_name = "instance1";
        let root_cert = build_root_certificate(&private_key, vm_name)?;

        let service_private_key = PKey::from_ec_key(build_ec_private_key()?)?;
        let service_public_key = public_from_private(&service_private_key)?;
        let service_fqin = "instance1:com.google.sdv.Service/default".to_string().replace(":", "|");
        let leaf_cert =
            build_leaf_certificate(&private_key, &root_cert, &service_public_key, &service_fqin)?;
        let leaf_cert_pem = String::from_utf8(leaf_cert.to_pem()?)?;
        assert!(leaf_cert_pem.starts_with("-----BEGIN CERTIFICATE-----\n"));
        assert!(leaf_cert_pem.ends_with("-----END CERTIFICATE-----\n"));
        Ok(())
    }

    fn generate_string(str_length: usize) -> String {
        // The chars that are used in FQIN
        const ALLOWED_CHARS: &[u8] =
            b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ:/";
        let mut rng = rand::thread_rng();
        let mut generated_string = String::with_capacity(str_length);
        for _ in 0..str_length {
            let idx = rng.random_range(0..ALLOWED_CHARS.len());
            generated_string.push(ALLOWED_CHARS[idx] as char);
        }

        generated_string
    }

    #[test]
    fn test_leaf_certificate_encode_decode() -> Result<()> {
        let issuer_priv_key = PKey::from_ec_key(build_ec_private_key()?)?;
        let issuer_name = "issuer_name";
        let issuer_certificate = build_root_certificate(&issuer_priv_key, issuer_name)?;

        let subject_priv_key = PKey::from_ec_key(build_ec_private_key()?)?;
        let subject_public_key = public_from_private(&subject_priv_key)?;
        let long_subject_name = generate_string(600);
        assert_eq!(long_subject_name.len(), 600);

        let cert = build_leaf_certificate(
            &issuer_priv_key,
            &issuer_certificate,
            &subject_public_key,
            &long_subject_name,
        )?;

        assert!(verify_identity(&cert, &long_subject_name)?);

        Ok(())
    }

    #[test]
    fn build_request_and_verify_identity() -> Result<()> {
        let ec_key = build_ec_private_key()?;
        let cert_request = build_certification_request(&ec_key, LARGEST_FQIN)?;

        let cert = request_to_cert_unsecure(&cert_request)?;

        assert!(verify_identity(&cert, LARGEST_FQIN)?);

        Ok(())
    }

    #[test]
    fn verify_ec_public_key_fails() -> Result<()> {
        use openssl::rsa::Rsa;

        let rsa_priv_key = Rsa::generate(2048)?;

        let rsa_pub_key = Rsa::from_public_components(
            rsa_priv_key.n().to_owned()?,
            rsa_priv_key.e().to_owned()?,
        )?;

        let pub_key = PKey::from_rsa(rsa_pub_key)?;

        assert!(verify_ec_key(&pub_key).is_err());

        Ok(())
    }

    #[test]
    fn verify_ec_public_key_passes() -> Result<()> {
        let priv_key = PKey::from_ec_key(build_ec_private_key()?)?;
        let pub_key = public_from_private(&priv_key)?;

        verify_ec_key(&pub_key)
    }

    #[test]
    fn test_pems_to_x509() -> Result<()> {
        let mut certs: Vec<X509> = vec![];
        let issuer_priv_key = PKey::from_ec_key(build_ec_private_key()?)?;
        certs.push(build_root_certificate(&issuer_priv_key, "issuer_name_1")?);
        certs.push(build_root_certificate(&issuer_priv_key, "issuer_name_2")?);

        let pem_data = x509_to_pems(&certs)?;

        let certs = pems_to_x509(&pem_data).unwrap();
        assert_eq!(certs.len(), 2);
        Ok(())
    }

    #[test]
    fn x509_common_name_to_string_ok() -> Result<()> {
        const TEST_NAME: &str = "foobar";
        let x509_name = to_x509_common_name(TEST_NAME)?;

        let extracted_common_name = x509_common_name_to_string(&x509_name)?;

        assert_eq!(extracted_common_name.as_str(), TEST_NAME);

        Ok(())
    }

    #[test]
    fn base32_x509_common_name_to_string_ok() -> Result<()> {
        const TEST_NAME: &str = "foobar";
        let x509_name = to_base32_x509_common_name(TEST_NAME)?;

        let extracted_common_name = base32_x509_common_name_to_string(&x509_name)?;

        assert_eq!(extracted_common_name.as_str(), TEST_NAME);

        Ok(())
    }
}
