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

#[path = "../src/cbor.rs"]
mod cbor;
#[path = "../src/validation.rs"]
mod validation;

// The module under test:
#[allow(dead_code)] // Due to some pub items that are not used in the tests
#[path = "../src/uds_certs.rs"]
mod uds_certs;

use anyhow::{anyhow, Context};
use coset::CborSerializable;
use crypto_rpc::cert::{build_root_certificate, public_from_private, to_x509_common_name};
use openssl::x509::X509;
use sample_dice_handover::create_random_uds;
use test_uds_certs_generator::{
    create_x509_uds_cert_chain_from_pkey_secrets, generate_ec_p256_key, uds_public_pkey_from_uds,
    UdsCertificateBuilder, UdsIntermediateCaCertificateBuilder, UdsRootCaCertificateBuilder,
    AN_EXAMPLE_UDS_ID,
};
use uds_certs::{UdsCertChain, UdsCertificates};
use validation::verify_uds_certificate_subject;

/// Creates a cert chain with a Root CA, two intermediate CAs and a UDS certificate at
/// the leaf.
fn create_example_uds_cert_chain() -> anyhow::Result<Vec<X509>> {
    let uds = create_random_uds();
    let keys = vec![generate_ec_p256_key()?, generate_ec_p256_key()?, generate_ec_p256_key()?];
    create_x509_uds_cert_chain_from_pkey_secrets(&keys, &uds)
}

fn create_example_uds_certificates() -> anyhow::Result<UdsCertificates> {
    let uds_cert_chain = create_example_uds_cert_chain()?;
    Ok(vec![UdsCertChain::try_from(uds_cert_chain)?].into())
}

fn create_rsa_uds_root_cert() -> anyhow::Result<X509> {
    use openssl::pkey::PKey;
    use openssl::rsa::Rsa;
    let rsa_priv_key = Rsa::generate(2048)?;
    let rsa_pub_key =
        Rsa::from_public_components(rsa_priv_key.n().to_owned()?, rsa_priv_key.e().to_owned()?)?;
    let priv_key = PKey::from_rsa(rsa_priv_key)?;
    let pub_key = PKey::from_rsa(rsa_pub_key)?;

    UdsRootCaCertificateBuilder::new()
        .set_private_key(&priv_key)
        .set_public_key(&pub_key)
        .set_ca_name("Google CA")?
        .build()
}

#[test]
fn to_cbor_and_back() -> anyhow::Result<()> {
    let uds_certificates = create_example_uds_certificates()?;

    let cbor: Vec<u8> =
        uds_certificates.clone().to_vec().context("Failed to serialize UdsCertificates")?;

    let deserialized_uds_certificates =
        UdsCertificates::from_slice(&cbor).context("Failed to deserialize UdsCertificates")?;

    assert_eq!(uds_certificates, deserialized_uds_certificates);

    Ok(())
}

#[test]
fn self_signed_uds_cert_is_invalid() -> anyhow::Result<()> {
    let uds_ca_private_key = generate_ec_p256_key()?;
    let uds_ca_name = "Google";
    let uds_root_cert = build_root_certificate(&uds_ca_private_key, uds_ca_name)?;

    let uds_private_key = generate_ec_p256_key()?;
    // The UDS certificate is self-signed instead of being signed by the root CA.
    let uds_cert = build_root_certificate(&uds_private_key, AN_EXAMPLE_UDS_ID)?;

    let err = UdsCertChain::try_from(vec![uds_root_cert, uds_cert])
        .err()
        .ok_or(anyhow!("Error expected"))?;
    assert!(err.to_string().contains("self signed certificate"), "unexpected error: {err:#?}");

    Ok(())
}

#[test]
fn valid_uds_cert_chain() -> anyhow::Result<()> {
    let uds_cert_chain = create_example_uds_cert_chain()?;
    UdsCertChain::try_from(uds_cert_chain)?;
    Ok(())
}

#[test]
fn shuffled_uds_cert_chain_is_invalid() -> anyhow::Result<()> {
    let mut uds_cert_chain = create_example_uds_cert_chain()?;

    uds_cert_chain.swap(1, 2);

    let err = UdsCertChain::try_from(uds_cert_chain).err().ok_or(anyhow!("Error expected"))?;
    assert!(
        err.to_string()
            .contains("The input certificate chain doesn't form a contiguous chain of trust"),
        "unexpected error: {err:#?}"
    );

    Ok(())
}

#[test]
fn uds_cert_chain_with_missing_cert_is_invalid() -> anyhow::Result<()> {
    // Creates the following chain:
    // Root_CA -> Intermediate_CA_1 -> Intermediate_CA_2 -> UDS_Cert
    let mut uds_cert_chain = create_example_uds_cert_chain()?;

    // It will now be:
    // Root_CA -> Intermediate_CA_1 -x- UDS_Cert
    // Which is no longer a chain as UDS_Cert is issued by the now absent Intermediate_CA_2.
    uds_cert_chain.remove(2);

    let err = UdsCertChain::try_from(uds_cert_chain).err().ok_or(anyhow!("Error expected"))?;
    assert!(
        err.to_string().contains("unable to get local issuer certificate"),
        "unexpected error: {err:#?}"
    );

    Ok(())
}

#[test]
fn uds_cert_chain_with_expired_root_ca_is_valid() -> anyhow::Result<()> {
    use test_uds_certs_generator::ValidityPeriod;

    let some_very_old_valid_since_date = "20000101000000Z";
    let an_expired_valid_until_date = "20250101000000Z";

    let uds = create_random_uds();
    let uds_pub = uds_public_pkey_from_uds(&uds)?;
    let root_ca_private_key = generate_ec_p256_key()?;
    let root_ca_public_keys = public_from_private(&root_ca_private_key)?;

    let root_cert = UdsRootCaCertificateBuilder::new()
        .set_private_key(&root_ca_private_key)
        .set_public_key(&root_ca_public_keys)
        .set_ca_name("Google CA")?
        .set_validity_period(ValidityPeriod {
            not_before: some_very_old_valid_since_date.to_owned(),
            not_after: an_expired_valid_until_date.to_owned(),
        })
        .build()?;
    let uds_cert = UdsCertificateBuilder::new()
        .set_issuer_private_key(&root_ca_private_key)
        .set_issuer_subject_name(root_cert.subject_name())
        .set_subject_public_key(&uds_pub)
        .set_uds_id(AN_EXAMPLE_UDS_ID)?
        .build()?;

    UdsCertChain::try_from(vec![root_cert, uds_cert])?;

    Ok(())
}

#[test]
fn expired_uds_cert_is_valid() -> anyhow::Result<()> {
    use test_uds_certs_generator::ValidityPeriod;

    let some_very_old_valid_since_date = "20000101000000Z";
    let an_expired_valid_until_date = "20250101000000Z";

    let uds = create_random_uds();
    let uds_pub = uds_public_pkey_from_uds(&uds)?;
    let root_ca_private_key = generate_ec_p256_key()?;
    let root_ca_public_keys = public_from_private(&root_ca_private_key)?;

    let root_cert = UdsRootCaCertificateBuilder::new()
        .set_private_key(&root_ca_private_key)
        .set_public_key(&root_ca_public_keys)
        .set_ca_name("Google CA")?
        .build()?;
    let uds_cert = UdsCertificateBuilder::new()
        .set_validity_period(ValidityPeriod {
            not_before: some_very_old_valid_since_date.to_owned(),
            not_after: an_expired_valid_until_date.to_owned(),
        })
        .set_issuer_private_key(&root_ca_private_key)
        .set_issuer_subject_name(root_cert.subject_name())
        .set_subject_public_key(&uds_pub)
        .set_uds_id(AN_EXAMPLE_UDS_ID)?
        .build()?;

    UdsCertChain::try_from(vec![root_cert, uds_cert])?;

    Ok(())
}

#[test]
fn uds_cert_chain_with_extra_unnecessary_cert_is_invalid() -> anyhow::Result<()> {
    let uds = create_random_uds();
    let keys = vec![generate_ec_p256_key()?, generate_ec_p256_key()?, generate_ec_p256_key()?];
    // Create a shorter chain by leaving out the last intermediate cert
    let mut shorter_uds_cert_chain =
        create_x509_uds_cert_chain_from_pkey_secrets(&keys[0..keys.len() - 1], &uds)?;

    // Create the complete test chain that has an extra intermediate cert
    let full_uds_cert_chain = create_x509_uds_cert_chain_from_pkey_secrets(&keys, &uds)?;
    // Insert the extra (valid) intermediate CA right before the UDS cert.
    shorter_uds_cert_chain.insert(
        shorter_uds_cert_chain.len() - 1,
        full_uds_cert_chain.last().ok_or(anyhow!("assert error"))?.clone(),
    );

    let err =
        UdsCertChain::try_from(shorter_uds_cert_chain).err().ok_or(anyhow!("Error expected"))?;
    assert!(
        err.to_string()
            .contains("The input certificate chain doesn't form a contiguous chain of trust"),
        "unexpected error: {err:#?}"
    );

    Ok(())
}

#[test]
fn rsa_uds_cert_chain_is_invalid() -> anyhow::Result<()> {
    let uds = create_random_uds();
    // Creating a chain just to obtain a leaf UDS cert
    let ec_uds_cert_chain =
        create_x509_uds_cert_chain_from_pkey_secrets(&[generate_ec_p256_key()?], &uds)?;

    // Creating a cert chain where the root CA has an RSA key. It doesn't matter that the
    // UDS leaf cert is not signed by the root CA RSA key, the validation will fail before
    // because of the RSA key type.
    let cert_chain_with_rsa_cert =
        vec![create_rsa_uds_root_cert()?, ec_uds_cert_chain.last().unwrap().clone()];

    let err = UdsCertChain::try_from(cert_chain_with_rsa_cert).expect_err("Error expected");
    assert!(err.to_string().contains("Unsupported algorithm ID"), "unexpected error: {err:#?}");

    Ok(())
}

///   Verifies the behavior when a UDS certificate chain contains an issuer
///  name that does not match the parent certificate's subject name.
/// - Creates a valid root CA certificate.
/// - Creates an intermediate CA certificate with an incorrect issuer name.
/// - Creates a UDS certificate signed by the intermediate CA.
/// - Attempts to construct the certificate chain and verifies that an
///   error is raised due to the issuer/subject name mismatch.
#[test]
fn issuer_name_not_matching_parent_err() -> anyhow::Result<()> {
    let uds = create_random_uds();
    let uds_pub = uds_public_pkey_from_uds(&uds)?;
    let private_keys = [generate_ec_p256_key()?, generate_ec_p256_key()?, generate_ec_p256_key()?];
    let public_keys: Vec<_> =
        private_keys.iter().map(|x| public_from_private(x).unwrap()).collect();

    let mut uds_certificate_chain = vec![];

    let ca_name = "Google CA";
    let root_cert = UdsRootCaCertificateBuilder::new()
        .set_private_key(&private_keys[0])
        .set_public_key(&public_keys[0])
        .set_ca_name(ca_name)?
        .build()?;
    uds_certificate_chain.push(root_cert.clone());

    // Intermediate CA's issuer name should be equal to `ca_name`.
    // Here we set it to a different string instead to check for error.
    let wrong_issuer_name = to_x509_common_name("Wrong name")?;
    let intermediate_ca_cert = UdsIntermediateCaCertificateBuilder::new()
        .set_issuer_private_key(&private_keys[0])
        .set_issuer_subject_name(&wrong_issuer_name)
        .set_subject_public_key(&public_keys[1])
        .set_subject_name("Intermediate CA 0")?
        .build()?;
    uds_certificate_chain.push(intermediate_ca_cert.clone());

    let uds_cert = UdsCertificateBuilder::new()
        .set_issuer_private_key(&private_keys[1])
        .set_issuer_subject_name(intermediate_ca_cert.subject_name())
        .set_subject_public_key(&uds_pub)
        .set_uds_id(AN_EXAMPLE_UDS_ID)?
        .build()?;

    uds_certificate_chain.push(uds_cert);

    let err =
        UdsCertChain::try_from(uds_certificate_chain).err().ok_or(anyhow!("Error expected"))?;
    assert!(
        err.to_string().contains("unable to get local issuer certificate"),
        "unexpected error: {err:#?}"
    );

    Ok(())
}

#[test]
fn verify_uds_certificate_subject_ok() -> anyhow::Result<()> {
    verify_uds_certificate_subject("SERIALNUMBER=11223344556677889900aabbccddeeff77889900")?;
    Ok(())
}

#[test]
fn verify_uds_certificate_subject_no_prefix_err() -> anyhow::Result<()> {
    verify_uds_certificate_subject("11223344556677889900aabbccddeeff77889900")
        .expect_err("subject missing SERIALNUMBER= prefix should have failed");
    Ok(())
}

#[test]
fn verify_uds_certificate_subject_wrong_prefix_err() -> anyhow::Result<()> {
    verify_uds_certificate_subject("FOO=11223344556677889900aabbccddeeff77889900")
        .expect_err("subject with wrong prefix should have failed");
    Ok(())
}

#[test]
fn verify_uds_certificate_subject_short_hex_err() -> anyhow::Result<()> {
    verify_uds_certificate_subject("SERIALNUMBER=aabbccddeeff77889900")
        .expect_err("subject with short hex should have failed");
    Ok(())
}

#[test]
fn verify_uds_certificate_subject_upper_case_hex_err() -> anyhow::Result<()> {
    verify_uds_certificate_subject("SERIALNUMBER=11223344556677889900AABBCCDDEEFF77889900")
        .expect_err("subject with upper-case hex should have failed");
    Ok(())
}

#[test]
fn verify_uds_certificate_subject_not_hex_err() -> anyhow::Result<()> {
    verify_uds_certificate_subject("SERIALNUMBER=11223344556677889900gghhiijjkkll77889900")
        .expect_err("subject serial number is not a HEX value");
    Ok(())
}
