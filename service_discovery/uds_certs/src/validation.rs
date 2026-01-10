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

//! Functions for validating certificate chains

use anyhow::{anyhow, bail, ensure, Context};
use openssl::{
    nid::Nid,
    x509::{X509NameEntryRef, X509NameRef, X509},
};

/// Prefix of an X.509 UDS certificate subject name
const UDS_CERT_SUBJECT_PREFIX: &str = "SERIALNUMBER=";

pub(crate) fn validate_chain(chain: &[X509]) -> anyhow::Result<()> {
    validate_x509_uds_cert_chain(chain)?;

    let uds_cert = chain.last().ok_or_else(|| anyhow!("Empty UDS certificate chain"))?;

    let uds_cert_subject = x509_common_name_to_string(uds_cert.subject_name())
        .context("Invalid UDS certificate subject")?;
    verify_uds_certificate_subject(uds_cert_subject.as_str())
        .context("Invalid UDS certificate subject")?;

    // TODO: b/383995982 - Verify that the leaf UDS X.509 certificate fields are according to the
    //       Open Profile for DICE:
    //       https://pigweed.googlesource.com/open-dice/+/HEAD/docs/specification.md#x_509-uds-certificates

    Ok(())
}

/// Verifies that:
///  * The given list of X.509 certificates form a chain of at least a root and a leaf
///  * The order of the certificates in the chain matches their position in the given list
///  * All certificates have a supported subject public key type
// TODO: b/391818684 make the relevant check from libhwtrust public and use that instead.
fn validate_x509_uds_cert_chain(chain: &[X509]) -> anyhow::Result<()> {
    use openssl::pkey::Id;
    use openssl::stack::Stack;
    use openssl::x509::store::X509StoreBuilder;
    use openssl::x509::verify::X509VerifyFlags;
    use openssl::x509::X509StoreContext;

    for cert in chain {
        let id = cert.public_key()?.id();
        // RSA is not supported, buecase the UDS CA public key in the vvmconfig is converted from
        // COSE key to an OpenSSL key using PublicKey::from_cose_key(), which doesn't support RSA.
        ensure!(matches!(id, Id::EC | Id::ED25519), "Unsupported algorithm ID {id:?}");
    }

    // X509StoreContext::init() needs the certificates organized as trust_store (root cert),
    // target (leaf cert) and untrusted (intermediate certs). See
    // https://docs.openssl.org/master/man3/X509_STORE_CTX_new/

    let mut trust_store_builder = X509StoreBuilder::new()?;
    let (root, unprocessed_certs) =
        chain.split_first().ok_or(anyhow!("The UDS cert chain must contain at least two certs"))?;
    trust_store_builder.add_cert(root.clone())?;
    // Do not check the validity period of the certificates against the current time.
    trust_store_builder.set_flags(X509VerifyFlags::NO_CHECK_TIME)?;
    let trust_store = trust_store_builder.build();

    let (target, unprocessed_certs) = unprocessed_certs
        .split_last()
        .ok_or(anyhow!("The UDS cert chain must contain at least two certs"))?;

    // The order of the intermediates within the Stack doesn't matter: OpenSSL tries to find a
    // path from the trust_store through the untrusted to the target.
    let mut untrusted = Stack::new()?;
    for cert in unprocessed_certs {
        untrusted.push(cert.clone())?;
    }

    let mut store_context = X509StoreContext::new()?;
    // init() returns the result from the with_context closure which has type
    // Result<T, ErrorStack>. Here, we use T=Result<(), Error> to be able to sneak out
    // errors from the closure.
    store_context.init(&trust_store, target, &untrusted, |context| {
        let success = context.verify_cert()?;
        if !success {
            return Ok(Err(anyhow!("Could not verify certificate chain: {:#?}", context.error())));
        }
        Ok(validate_verified_chain(context, chain))
    })?
}

/// X509StoreContext::init() finds a valid certificate chain from the trusted root certificate
/// using any of the untrusted certificates until the target leaf cert. This function validates
/// that the verified chain in the context is the same as the original chain meaning that
/// 1. all intermediate certificates were used to form the verified chain
/// 2. the intermediate certificates were in the same order as in the verified chain
fn validate_verified_chain(
    context: &openssl::x509::X509StoreContextRef,
    original_chain: &[X509],
) -> anyhow::Result<()> {
    let verified_chain = context.chain().ok_or_else(|| anyhow!("missing verified cert chain"))?;
    // X509StoreContext::chain() returns the certificate chain from leaf to root.
    if !verified_chain.iter().rev().eq(original_chain.iter()) {
        bail!("The input certificate chain doesn't form a contiguous chain of trust");
    }
    Ok(())
}

/// Verifies that the given `subject` matches the format:
/// "SERIALNUMBER=UDS_ID". Where UDS_ID is the hex (lower case) encoding
/// of an UDS_ID-long value.
pub(crate) fn verify_uds_certificate_subject(subject: &str) -> anyhow::Result<()> {
    let uds_id_hex = subject
        .strip_prefix(UDS_CERT_SUBJECT_PREFIX)
        .ok_or_else(|| anyhow!("Subject does not start with {UDS_CERT_SUBJECT_PREFIX}"))?;

    validate_id_hex(uds_id_hex)
}

/// The size of an ID (UDS_ID or CDI_ID), in bytes.
/// As per
/// https://pigweed.googlesource.com/open-dice/+/HEAD/docs/specification.md#deriving-identifiers
const ID_SIZE: usize = 20;

// TODO: b/425314094 - Avoid having duplicate validate_id_hex() implementations
/// Validates that the given string is a lower-case hex encoding of
/// a value mathing the size of an UDS_ID or a CDI_ID.
fn validate_id_hex(id_hex: &str) -> anyhow::Result<()> {
    let id = hex::decode(id_hex).context("{id_hex} is not a hex encoded value")?;
    ensure!(id.len() == ID_SIZE, "hex encoded value {id_hex} has an invalid size");
    ensure!(
        id_hex.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
        "{id_hex} is not a lower case hex encoded value"
    );
    Ok(())
}

/// Return an X.509 common name as a String.
fn x509_common_name_to_string(x509_name: &X509NameRef) -> anyhow::Result<String> {
    let common_name: &X509NameEntryRef = x509_name
        .entries()
        .find(|name_entry| name_entry.object().nid() == Nid::COMMONNAME)
        .ok_or(anyhow!("X.509 name does not have a \"common name\" entry."))?;

    Ok(common_name.data().as_utf8()?.to_string())
}
