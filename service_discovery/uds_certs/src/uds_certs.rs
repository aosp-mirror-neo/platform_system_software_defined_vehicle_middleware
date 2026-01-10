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

//! Classes for handling the uds_certs file.

use crate::{
    cbor::{ce, x509_from_cbor_value, x509_to_cbor_value},
    validation::validate_chain,
};
use anyhow::{anyhow, Context};
use ciborium::value::Value;
use coset::{AsCborValue, CborSerializable, CoseError};
use hwtrust::publickey::PublicKey;
use openssl::x509::X509;
use std::collections::VecDeque;
use std::convert::TryFrom;
use std::path::Path;

/// Path to the uds_certs file.
///
/// The uds_certs file must be in the vvmtruststore partition.
pub const UDS_CERTS_PATH: &str = "/vvmtruststore/uds_certs";

/// Version of the UdsCertificates CDDL
const UDS_CERTS_VERSION: u32 = 1;

/// A Certificate Chain for a UDS (Unique Device Secret) Public Key.
#[derive(Clone, Debug, PartialEq)]
pub struct UdsCertChain {
    chain: Vec<X509>,
}

/// Certificates for UDS (Unique Device Secret) Public Keys.
#[derive(Clone, Debug, PartialEq)]
pub struct UdsCertificates {
    certificate_chains: Vec<UdsCertChain>,
}

impl UdsCertChain {
    /// Returns the public key of the root certificate within the chain.
    pub fn root_ca_pub(&self) -> anyhow::Result<PublicKey> {
        PublicKey::try_from(
            self.chain
                .first()
                .ok_or_else(|| anyhow!("Empty UDS certificate chain"))?
                .public_key()
                .context("Failed to obtain pub key from root certificate")?,
        )
        .context("Failed to convert pub key")
    }

    /// Returns the public key of the leaf (UDS) certificate within the chain.
    pub fn uds_pub(&self) -> anyhow::Result<PublicKey> {
        PublicKey::try_from(
            self.chain
                .last()
                .ok_or_else(|| anyhow!("Empty UDS certificate chain"))?
                .public_key()
                .context("Failed to obtain pub key from leaf (UDS) certificate")?,
        )
        .context("Failed to convert pub key")
    }

    /// The individual X.509 certificates that make up the UDS certificate chain.
    ///
    /// Ordered from the root cert (chain.first()) to the UDS certificate at the end of the vector
    /// (chain.last()).
    pub fn chain(&self) -> &[X509] {
        &self.chain
    }
}

impl From<Vec<UdsCertChain>> for UdsCertificates {
    fn from(certificate_chains: Vec<UdsCertChain>) -> Self {
        Self { certificate_chains }
    }
}

impl UdsCertificates {
    /// Loads UdsCertificates from its standard location.
    pub fn load() -> anyhow::Result<Self> {
        let path = Path::new(UDS_CERTS_PATH);

        let cbor: Vec<u8> =
            std::fs::read(path).with_context(|| format!("Failed to read file: {path:?}"))?;

        UdsCertificates::from_slice(&cbor)
            .context("Failed to deserialize UdsCertificates from CBOR")
    }

    /// The UDS certificate chains.
    pub fn certificate_chains(&self) -> &[UdsCertChain] {
        &self.certificate_chains
    }
}

// It implements the following CDDL:
//
// UdsCertificates = [
//   1, ; version
//   *   UdsCertChain
// ]
//
// UdsCertChain = [
//  2* X509Certificate ; Root -> ... -> Leaf. "Root" is the vendor self-signed
//                     ; cert, "Leaf" contains UDS_Public. There may also be
//                     ; intermediate certificates between Root and Leaf.
// ]
//
// ; A bstr containing a DER-encoded X.509 certificate (RSA, NIST P-curve, or EdDSA)
// X509Certificate = bstr
impl AsCborValue for UdsCertificates {
    fn from_cbor_value(value: Value) -> Result<Self, CoseError> {
        let mut values: VecDeque<Value> =
            value.into_array().map_err(|_| ce("UdsCertificates must be an array"))?.into();

        let version: u32 = values
            .pop_front()
            .ok_or(ce("UdsCertificates array is missing a version"))?
            .into_integer()
            .map_err(|_| ce("UdsCertificates::version is not an integer"))?
            .try_into()
            .map_err(|_| ce("UdsCertificates::version doesn't fit into an u32"))?;

        if version != UDS_CERTS_VERSION {
            return Err(ce("Unsupported UdsCertificates version"));
        }

        let certificate_chains: Vec<UdsCertChain> = values
            .into_iter()
            .map(UdsCertChain::from_cbor_value)
            .collect::<Result<Vec<UdsCertChain>, CoseError>>()?;

        Ok(UdsCertificates { certificate_chains })
    }

    fn to_cbor_value(self) -> Result<Value, CoseError> {
        let mut uds_certs = vec![Value::from(UDS_CERTS_VERSION)];

        for cert_chain in self.certificate_chains {
            uds_certs.push(cert_chain.to_cbor_value()?);
        }

        Ok(Value::Array(uds_certs))
    }
}

impl CborSerializable for UdsCertificates {}

impl TryFrom<Vec<X509>> for UdsCertChain {
    type Error = anyhow::Error;

    fn try_from(chain: Vec<X509>) -> anyhow::Result<Self, Self::Error> {
        validate_chain(chain.as_slice())?;
        Ok(UdsCertChain { chain })
    }
}

impl AsCborValue for UdsCertChain {
    fn from_cbor_value(value: Value) -> Result<Self, CoseError> {
        let chain = value
            .into_array()
            .map_err(|_| ce("UdsCertChain must be an array"))?
            .into_iter()
            .map(x509_from_cbor_value)
            .collect::<Result<Vec<X509>, CoseError>>()?;

        UdsCertChain::try_from(chain).map_err(|e| ce(&format!("{e:?}")))
    }

    fn to_cbor_value(self) -> Result<Value, CoseError> {
        Ok(Value::Array(
            self.chain
                .into_iter()
                .map(x509_to_cbor_value)
                .collect::<Result<Vec<Value>, CoseError>>()?,
        ))
    }
}
