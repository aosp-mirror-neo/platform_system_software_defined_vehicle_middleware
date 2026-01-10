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

use crate::{util::ce, KeyUsage};
use ciborium::value::Value;
use coset::{AsCborValue, CborSerializable, CoseError, CoseKey};

/// Key of the "iss" (Issuer) claim.
/// As specified in https://datatracker.ietf.org/doc/html/rfc8392#section-3.1.1
pub const ISS: i64 = 1;

/// Key of the "sub" (Subject) claim.
/// As specified in https://datatracker.ietf.org/doc/html/rfc8392#section-3.1.2
pub const SUB: i64 = 2;

/// Key of the "subjectPublicKey" claim.
/// As specified in Open Profile for DICE, CBOR UDS Certificates, Additional Fields
/// https://pigweed.googlesource.com/open-dice/+/HEAD/docs/specification.md#profile-design-certificate-details-cbor-uds-certificates-additional-fields
const SUBJECT_PUBLIC_KEY: i64 = -4670552;

/// Key of the "keyUsage" claim.
/// As specified in Open Profile for DICE, CBOR UDS Certificates, Additional Fields
/// https://pigweed.googlesource.com/open-dice/+/HEAD/docs/specification.md#profile-design-certificate-details-cbor-uds-certificates-additional-fields
pub const KEY_USAGE: i64 = -4670553;

/// The claims made by the CBOR Web Token of a certificate.
#[derive(Clone, Debug, PartialEq)]
pub struct Claims {
    /// The "iss" (Issuer) claim
    /// As specified in RFC 8392, section 3.1.1
    pub issuer: String,

    /// The "sub" (Subject) claim
    /// As specified in RFC 8392, section 3.1.2
    pub subject: String,

    /// The "subjectPublicKey" claim
    /// As specified in Open Profile for DICE, CBOR UDS Certificates, Additional Fields.
    pub subject_public_key: CoseKey,

    /// The "keyUsage" claim
    /// As specified in Open Profile for DICE, CBOR UDS Certificates, Additional Fields
    pub key_usage: KeyUsage,
}

impl AsCborValue for Claims {
    fn from_cbor_value(value: Value) -> Result<Self, CoseError> {
        let map: Vec<(Value, Value)> = value.into_map().map_err(|_| ce("CWT must be a map"))?;

        let mut issuer: Result<String, CoseError> = Err(ce("Issuer claim is missing"));
        let mut subject: Result<String, CoseError> = Err(ce("Subject claim is missing"));
        let mut subject_public_key: Result<CoseKey, CoseError> =
            Err(ce("subjectPublicKey is missing"));
        let mut key_usage: Result<KeyUsage, CoseError> = Err(ce("keyUsage claim is missing"));

        for (key, value) in map.into_iter() {
            let int_key: i64 = key
                .into_integer()
                .map_err(|e| ce(&format!("Claim key must be an integer: {e:?}")))?
                .try_into()
                .map_err(|_| ce("Claim key integer must fit into an i64"))?;

            match int_key {
                ISS => {
                    issuer = value
                        .into_text()
                        .map_err(|e| ce(&format!("\"iss\" claim must be a text string: {e:?}")));
                }
                SUB => {
                    subject = value
                        .into_text()
                        .map_err(|e| ce(&format!("\"sub\" claim must be a text string: {e:?}")));
                }
                SUBJECT_PUBLIC_KEY => {
                    let cbor_bytes = value.into_bytes().map_err(|e| {
                        ce(&format!("\"subjectPublicKey\" claim must be a byte string: {e:?}"))
                    })?;

                    subject_public_key = CoseKey::from_slice(&cbor_bytes).map_err(|e| {
                        ce(&format!(
                            "\"subjectPublicKey\" byte string does not encode a COSE_Key: {e:?}"
                        ))
                    });
                }
                KEY_USAGE => {
                    key_usage = KeyUsage::from_cbor_value(value);
                }
                _ => return Err(ce(&format!("Unknown key {int_key:?}"))),
            }
        }

        Ok(Claims {
            issuer: issuer?,
            subject: subject?,
            subject_public_key: subject_public_key?,
            key_usage: key_usage?,
        })
    }

    fn to_cbor_value(self) -> Result<Value, CoseError> {
        Ok(Value::Map(vec![
            (Value::Integer(ISS.into()), Value::Text(self.issuer.clone())),
            (Value::Integer(SUB.into()), Value::Text(self.subject.clone())),
            (
                Value::Integer(SUBJECT_PUBLIC_KEY.into()),
                Value::Bytes(self.subject_public_key.to_vec()?),
            ),
            (Value::Integer(KEY_USAGE.into()), self.key_usage.to_cbor_value()?),
        ]))
    }
}

impl CborSerializable for Claims {}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use crypto_rpc::cert::{build_ec_private_key, public_from_private};
    use hwtrust::publickey::PublicKey;
    use openssl::pkey::{PKey, Private};

    fn generate_public_cose_key() -> anyhow::Result<CoseKey> {
        let private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let public_key: PublicKey = public_from_private(&private_key)?.try_into()?;
        public_key.to_cose_key()
    }

    #[test]
    fn to_value_and_back() -> anyhow::Result<()> {
        let claims = Claims {
            issuer: "foo".to_owned(),
            subject: "bar".to_owned(),
            subject_public_key: generate_public_cose_key()?,
            key_usage: KeyUsage { digital_signature: true, key_cert_sign: false },
        };

        let value = claims.clone().to_cbor_value().context("to CBOR value")?;

        let decoded_claims = Claims::from_cbor_value(value).context("from CBOR value")?;

        assert_eq!(claims, decoded_claims);

        Ok(())
    }
}
