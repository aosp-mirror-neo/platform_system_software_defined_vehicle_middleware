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

use crate::util::ce;
use ciborium::value::Value;
use coset::{AsCborValue, CoseError};

/// keyCertSign key usage.
/// Specified in RFC 5280, section "4.2.1.3. Key Usage".
/// See https://datatracker.ietf.org/doc/html/rfc5280#section-4.2.1.3
const KEY_CERT_SIGN: u8 = 1 << 5;

/// digitalSignature key usage.
/// Specified in RFC 5280, section "4.2.1.3. Key Usage".
/// See https://datatracker.ietf.org/doc/html/rfc5280#section-4.2.1.3
const DIGITAL_SIGNATURE: u8 = 1;

/// Allowed key usages
///
/// RFC 5280, section "4.2.1.3. Key Usage"
///
/// Note that it only represents the subset of key usages that are supported by Android SDV.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyUsage {
    /// digitalSignature key usage.
    pub digital_signature: bool,

    /// keyCertSign key usage.
    pub key_cert_sign: bool,
}

impl AsCborValue for KeyUsage {
    fn from_cbor_value(value: Value) -> Result<Self, CoseError> {
        let key_usage: Vec<u8> = value
            .into_bytes()
            .map_err(|e| ce(&format!("keyUsage claim must be a byte string: {e:?}")))?;

        if key_usage.is_empty() {
            return Err(ce("keyUsage claim byte string is empty"));
        }

        let digital_signature = key_usage[0] & DIGITAL_SIGNATURE != 0;
        let key_cert_sign = key_usage[0] & KEY_CERT_SIGN != 0;

        if key_usage[0] & !(DIGITAL_SIGNATURE | KEY_CERT_SIGN) != 0 {
            return Err(ce(&format!("Unsupported key usages: {key_usage:?}")));
        }

        if key_usage.len() > 1 {
            for byte in &key_usage[1..] {
                if *byte != 0 {
                    return Err(ce(&format!("Unsupported key usages: {key_usage:?}")));
                }
            }
        }

        Ok(KeyUsage { digital_signature, key_cert_sign })
    }

    fn to_cbor_value(self) -> Result<Value, CoseError> {
        let mut key_usage: Vec<u8> = vec![0, 0];

        if self.digital_signature {
            key_usage[0] |= DIGITAL_SIGNATURE;
        }

        if self.key_cert_sign {
            key_usage[0] |= KEY_CERT_SIGN;
        }

        Ok(Value::Bytes(key_usage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;

    #[test]
    fn to_value_and_back() -> anyhow::Result<()> {
        let key_usage = KeyUsage { digital_signature: true, key_cert_sign: false };

        let value = key_usage.clone().to_cbor_value().context("to CBOR value")?;

        let decoded_key_usage = KeyUsage::from_cbor_value(value).context("from CBOR value")?;

        assert_eq!(key_usage, decoded_key_usage);

        Ok(())
    }

    #[test]
    fn from_cbor_value_digital_signature_and_key_cert_sign_ok() -> anyhow::Result<()> {
        // digitalSignature (0) and keyCertSign (5) bits set.
        let bytes = vec![0b00100001u8, 0b00000000u8];

        let key_usage =
            KeyUsage::from_cbor_value(Value::Bytes(bytes)).context("from CBOR value")?;
        assert_eq!(key_usage, KeyUsage { digital_signature: true, key_cert_sign: true });
        Ok(())
    }

    #[test]
    fn from_cbor_value_digital_signature_and_key_cert_sign_single_byte_ok() -> anyhow::Result<()> {
        // digitalSignature (0) and keyCertSign (5) bits set.
        let bytes = vec![0b00100001u8];

        let key_usage =
            KeyUsage::from_cbor_value(Value::Bytes(bytes)).context("from CBOR value")?;
        assert_eq!(key_usage, KeyUsage { digital_signature: true, key_cert_sign: true });
        Ok(())
    }

    #[test]
    fn from_cbor_value_digital_signature_ok() -> anyhow::Result<()> {
        // digitalSignature (0) bit set.
        let bytes = vec![0b00000001u8, 0b00000000u8];

        let key_usage =
            KeyUsage::from_cbor_value(Value::Bytes(bytes)).context("from CBOR value")?;
        assert_eq!(key_usage, KeyUsage { digital_signature: true, key_cert_sign: false });
        Ok(())
    }

    #[test]
    fn from_cbor_value_key_cert_sign_ok() -> anyhow::Result<()> {
        // keyCertSign (5) bit set.
        let bytes = vec![0b00100000u8, 0b00000000u8];

        let key_usage =
            KeyUsage::from_cbor_value(Value::Bytes(bytes)).context("from CBOR value")?;
        assert_eq!(key_usage, KeyUsage { digital_signature: false, key_cert_sign: true });
        Ok(())
    }

    #[test]
    fn from_cbor_value_digital_signature_and_non_repudiation_err() -> anyhow::Result<()> {
        // digitalSignature (0) and nonRepudiation (1) bits set.
        let bytes = vec![0b00000011u8, 0b00000000u8];

        // Since we don't support the nonRepudiation key usage, decoding will fail.
        let result = KeyUsage::from_cbor_value(Value::Bytes(bytes));
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn from_cbor_value_key_cert_sign_and_bit_9_err() -> anyhow::Result<()> {
        // keyCertSign (5) and the invalid bit 9 are set
        let bytes = vec![0b00100000u8, 0b00000010u8];

        // Since bit 9 is undefined/invalid, decoding will fail.
        let result = KeyUsage::from_cbor_value(Value::Bytes(bytes));
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn from_cbor_value_single_byte_ok() -> anyhow::Result<()> {
        // No bits are set
        let bytes = vec![0b00000000u8];

        let key_usage =
            KeyUsage::from_cbor_value(Value::Bytes(bytes)).context("from CBOR value")?;
        assert_eq!(key_usage, KeyUsage { digital_signature: false, key_cert_sign: false });
        Ok(())
    }

    #[test]
    fn from_cbor_value_empty_vec_err() -> anyhow::Result<()> {
        let bytes: Vec<u8> = vec![];

        let result = KeyUsage::from_cbor_value(Value::Bytes(bytes));
        assert!(result.is_err());
        Ok(())
    }
}
