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

//! UdsPublicKeys: The UDS (Unique Device Secret) public keys of devices that can join the Mesh.

use anyhow::Context;
use ciborium::Value;
use coset::{AsCborValue, CborSerializable, CoseError, CoseKey};
use std::collections::VecDeque;
use std::path::Path;

/// Path to the uds_pubs file.
///
/// The uds_pubs must be in the vvmtruststore partition.
pub const UDS_PUBS_PATH: &str = "/vvmtruststore/uds_pubs";

/// Version of the UdsPublicKeys CDDL
const UDS_PUBS_VERSION: u32 = 1;

/// The UDS (Unique Device Secret) public keys of devices that can join the Mesh.
///
/// This is the structure that is stored in CBOR in the vvmtruststore partition.
#[derive(Clone, Debug, PartialEq)]
pub struct UdsPublicKeys {
    /// The UDS (Unique Device Secret) public keys of hardware that can join the Mesh.
    ///
    /// Only VMs running in hardware identified by one of the UDS keys listed here may
    /// join the mesh.
    pub keys: Vec<CoseKey>,
}

impl UdsPublicKeys {
    /// Loads UdsPublicKeys from its standard location.
    pub fn load() -> anyhow::Result<Self> {
        let path = Path::new(UDS_PUBS_PATH);

        let cbor: Vec<u8> =
            std::fs::read(path).with_context(|| format!("Failed to read file: {path:#?}"))?;

        UdsPublicKeys::from_slice(&cbor).context("Failed to deserialize UdsPublicKeys from CBOR")
    }
}

fn semantic_error(s: &str) -> CoseError {
    use ciborium::de::Error;
    use CoseError::DecodeFailed;

    DecodeFailed(Error::semantic(None, s))
}

// It implements the following CDDL:
//
// UdsPublicKeys = [
//  1, ; version
//  + keys: bstr .cbor COSE_Key, ; the byte strings appear in a sorted order
// ]
impl AsCborValue for UdsPublicKeys {
    fn from_cbor_value(value: Value) -> Result<Self, CoseError> {
        let mut values: VecDeque<Value> = value
            .into_array()
            .map_err(|_| semantic_error("UdsPublicKeys must be an array"))?
            .into();

        let version: u32 = values
            .pop_front()
            .ok_or(semantic_error("UdsPublicKeys array is missing a version"))?
            .into_integer()
            .map_err(|_| semantic_error("UdsPublicKeys::version is not an integer"))?
            .try_into()
            .map_err(|_| semantic_error("UdsPublicKeys::version doesn't fit into an u32"))?;

        if version != UDS_PUBS_VERSION {
            return Err(semantic_error("Unsupported UdsPublicKeys version"));
        }

        let keys: Vec<CoseKey> = {
            let mut keys = Vec::new();
            for value in values {
                let bytes =
                    value.into_bytes().map_err(|_| semantic_error("Key must be a byte string"))?;
                let key = CoseKey::from_slice(bytes.as_slice()).map_err(|e| {
                    semantic_error(&format!("Byte string does not encode a CoseKey: {e}"))
                })?;
                keys.push(key);
            }
            keys
        };

        if keys.is_empty() {
            return Err(semantic_error("UdsPublicKeys must have at least one CoseKey"));
        }

        Ok(UdsPublicKeys { keys })
    }

    fn to_cbor_value(self) -> Result<Value, CoseError> {
        let mut keys = self
            .keys
            .into_iter()
            .map(CborSerializable::to_vec)
            .collect::<Result<Vec<Vec<u8>>, CoseError>>()?;

        // For a UdsPublicKeys with given set of keys, its serialized CBOR must be the same
        // regardless of the order of those keys in its vector.  This is needed to ensure that the
        // uds_pubs of all VMs in a device are exactly the same.  Which means their VVMFactoryTrust
        // (SHA-256(uds_pubs)) is also the same, allowing a device to burn the VVMFactoryTrust into
        // OTP memory only once and use it for all its SDV VMs.
        keys.sort();

        let mut uds_pubs = vec![Value::from(UDS_PUBS_VERSION)];

        for key_bytes in keys {
            uds_pubs.push(Value::from(key_bytes));
        }

        Ok(Value::Array(uds_pubs))
    }
}

impl CborSerializable for UdsPublicKeys {}

#[cfg(test)]
mod tests {
    use super::*;
    use coset::{iana::EllipticCurve, CoseKeyBuilder};

    fn create_example_uds_public_keys() -> UdsPublicKeys {
        let key1 = CoseKeyBuilder::new_ec2_pub_key(
            EllipticCurve::P_256,
            vec![
                34, 113, 60, 21, 5, 175, 166, 227, 223, 238, 76, 85, 237, 149, 239, 4, 185, 13, 6,
                150, 232, 134, 135, 100, 118, 44, 8, 85, 34, 116, 78, 4,
            ],
            vec![
                131, 229, 51, 227, 235, 176, 238, 252, 199, 252, 244, 138, 23, 131, 178, 103, 174,
                240, 52, 176, 241, 130, 25, 36, 192, 180, 99, 37, 35, 169, 195, 147,
            ],
        )
        .build();

        let key2 = CoseKeyBuilder::new_ec2_pub_key(
            EllipticCurve::P_256,
            vec![
                34, 113, 65, 21, 5, 175, 166, 212, 223, 238, 76, 85, 237, 149, 239, 4, 185, 13, 6,
                150, 232, 134, 135, 100, 118, 44, 8, 85, 34, 116, 78, 4,
            ],
            vec![
                131, 229, 50, 227, 235, 176, 238, 252, 199, 252, 244, 138, 23, 131, 178, 103, 174,
                240, 52, 176, 240, 130, 25, 36, 192, 180, 99, 37, 35, 169, 195, 147,
            ],
        )
        .build();

        UdsPublicKeys { keys: vec![key1, key2] }
    }

    #[test]
    fn to_cbor_and_back() -> anyhow::Result<()> {
        let uds_pubs = create_example_uds_public_keys();

        let cbor: Vec<u8> =
            uds_pubs.clone().to_vec().context("Failed to serialize uds_pubs to CBOR")?;

        let deserialized_uds_pubs =
            UdsPublicKeys::from_slice(&cbor).context("Failed to deserialize uds_pubs from CBOR")?;

        assert_eq!(uds_pubs, deserialized_uds_pubs);

        Ok(())
    }

    fn create_eddsa_key(x: Vec<u8>) -> CoseKey {
        use coset::iana::{self, EnumI64};
        let label_crv = iana::OkpKeyParameter::Crv.to_i64();
        let label_x = iana::OkpKeyParameter::X.to_i64();
        CoseKeyBuilder::new_okp_key()
            .param(label_crv, Value::from(iana::EllipticCurve::Ed25519.to_i64()))
            .param(label_x, Value::from(x))
            .algorithm(iana::Algorithm::EdDSA)
            .add_key_op(iana::KeyOperation::Verify)
            .build()
    }

    #[test]
    fn cbor_has_sorted_keys() -> anyhow::Result<()> {
        let key1 = create_eddsa_key(vec![
            99, 58, 151, 135, 192, 64, 51, 139, 145, 37, 3, 246, 48, 233, 223, 126, 147, 15, 151,
            86, 221, 27, 53, 102, 252, 25, 69, 75, 124, 94, 104, 102,
        ]);
        let key2 = create_eddsa_key(vec![
            148, 141, 245, 249, 146, 147, 86, 112, 94, 219, 146, 53, 14, 170, 167, 205, 229, 106,
            237, 177, 94, 241, 77, 41, 252, 155, 115, 101, 131, 47, 120, 3,
        ]);
        let key3 = create_eddsa_key(vec![
            242, 127, 119, 129, 239, 170, 82, 197, 105, 143, 253, 124, 144, 92, 68, 3, 82, 102,
            224, 232, 20, 15, 3, 208, 176, 196, 228, 85, 70, 63, 189, 69,
        ]);
        let key4 = create_eddsa_key(vec![
            129, 97, 191, 59, 211, 225, 194, 162, 37, 41, 51, 3, 125, 213, 128, 244, 96, 206, 48,
            8, 104, 91, 219, 202, 187, 216, 9, 245, 210, 16, 216, 172,
        ]);
        let key5 = create_eddsa_key(vec![
            102, 213, 139, 112, 235, 217, 94, 152, 185, 58, 183, 103, 165, 52, 100, 255, 127, 9,
            170, 83, 144, 55, 134, 231, 241, 195, 49, 218, 207, 138, 76, 163,
        ]);
        let key6 = create_eddsa_key(vec![
            178, 187, 90, 151, 99, 14, 6, 110, 179, 161, 135, 148, 91, 189, 25, 245, 190, 57, 147,
            16, 204, 218, 175, 242, 57, 182, 17, 18, 67, 191, 23, 232,
        ]);

        let uds_public_keys_a = UdsPublicKeys {
            keys: vec![
                key1.clone(),
                key2.clone(),
                key3.clone(),
                key4.clone(),
                key5.clone(),
                key6.clone(),
            ],
        }
        .to_vec()?;

        let uds_public_keys_b = UdsPublicKeys {
            keys: vec![
                key6.clone(),
                key5.clone(),
                key4.clone(),
                key3.clone(),
                key2.clone(),
                key1.clone(),
            ],
        }
        .to_vec()?;

        let uds_public_keys_c =
            UdsPublicKeys { keys: vec![key4, key3, key5, key6, key2, key1] }.to_vec()?;

        assert_eq!(uds_public_keys_a, uds_public_keys_b);
        assert_eq!(uds_public_keys_b, uds_public_keys_c);

        Ok(())
    }
}
