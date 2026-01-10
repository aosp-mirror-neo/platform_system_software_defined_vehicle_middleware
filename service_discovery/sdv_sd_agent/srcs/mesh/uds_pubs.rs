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

use crate::sha256::sha256;
use crate::system::properties;
use anyhow::Context;
use coset::{CborSerializable, CoseKey};
use log::warn;
use std::path::Path;
use vvmtruststore::{UdsPublicKeys, UDS_PUBS_PATH};

/// Name of the Android system property where the VvmFactoryTrust is located.
///
/// The bootloader is required to read the value it passes to Android via the
/// androidboot.sdv.vvmfactorytrust kernel parameter from one-time-programmable
/// memory. This memory is written during factory provisioning.
const SYSTEM_PROP_FACTORY_TRUST: &str = "ro.boot.sdv.vvmfactorytrust";

const ZERO: &str = "00";

/// The UDS (Unique Device Secret) public keys of hardware that can join the Mesh.
///
/// Additionally, those keys may or may not be trusted.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UdsPubs {
    /// The UDS (Unique Device Secret) public keys of hardware that can join the Mesh.
    ///
    /// Only VMs running in hardware identified by one of the UDS keys listed here may
    /// join the mesh.
    keys: Vec<CoseKey>,

    /// Whether those keys are inherently trusted.
    ///
    /// If `true`, those keys can act as root of trust in DICE (Device Identifier
    /// Composition Engine) certificate chains. Ie, a self-signed UDS certificate at the
    /// root of a DICE chain is sufficient. Otherwise, a UDS certificate signed by a
    /// Certificate Authority (defined in the VvmConfig) is needed.
    trusted: bool,
}

pub(crate) fn get_factory_trust() -> anyhow::Result<Option<Vec<u8>>> {
    let factory_trust_hex: String = match properties::read(SYSTEM_PROP_FACTORY_TRUST)? {
        Some(s) => s,
        None => {
            warn!("Android system property not set: {SYSTEM_PROP_FACTORY_TRUST}");
            return Ok(None);
        }
    };

    if factory_trust_hex == ZERO {
        return Ok(None);
    }

    let factory_trust: Vec<u8> = hex::decode(&factory_trust_hex).with_context(|| {
        format!(
            "Failed to decode hexadecimal string in {}: \"{}\"",
            SYSTEM_PROP_FACTORY_TRUST, &factory_trust_hex
        )
    })?;

    Ok(Some(factory_trust))
}

impl UdsPubs {
    /// Loads UdsPubs from a well defined filepath.
    ///
    /// Factory trust comes the from bootloader via a kernel parameter.
    pub fn load() -> anyhow::Result<Self> {
        Self::load_with_params(Path::new(UDS_PUBS_PATH), get_factory_trust()?)
    }

    fn load_with_params(path: &Path, factory_trust: Option<Vec<u8>>) -> anyhow::Result<Self> {
        let cbor: Vec<u8> =
            std::fs::read(path).with_context(|| format!("Failed to read file: {path:#?}"))?;

        let trusted = match factory_trust {
            Some(trusted_hash) => sha256(&cbor)? == trusted_hash,
            None => false,
        };

        let UdsPublicKeys { keys } = UdsPublicKeys::from_slice(&cbor)
            .context("Failed to deserialize UdsPublicKeys from CBOR")?;

        Ok(UdsPubs { keys, trusted })
    }

    /// Whether the keys() are inherently trusted.
    pub fn trusted(&self) -> bool {
        self.trusted
    }

    /// The UDS (Unique Device Secret) public keys of hardware that can join the Mesh.
    pub fn keys(&self) -> &[CoseKey] {
        self.keys.as_slice()
    }

    #[cfg(test)]
    pub fn new_trusted(keys: Vec<CoseKey>) -> Self {
        Self { keys, trusted: true }
    }

    #[cfg(test)]
    pub fn new_untrusted(keys: Vec<CoseKey>) -> Self {
        Self { keys, trusted: false }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use coset::{iana::EllipticCurve, CoseKeyBuilder};

    pub(crate) fn create_example_key1() -> CoseKey {
        CoseKeyBuilder::new_ec2_pub_key(
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
        .build()
    }

    pub(crate) fn create_example_key2() -> CoseKey {
        CoseKeyBuilder::new_ec2_pub_key(
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
        .build()
    }

    pub(crate) fn create_example_key3() -> CoseKey {
        CoseKeyBuilder::new_ec2_pub_key(
            EllipticCurve::P_256,
            vec![
                154, 58, 124, 142, 90, 110, 77, 47, 138, 27, 157, 60, 126, 95, 106, 75, 157, 44,
                142, 26, 127, 91, 109, 74, 140, 155, 46, 122, 31, 93, 107, 76,
            ],
            vec![
                17, 34, 51, 68, 85, 102, 119, 136, 153, 170, 187, 204, 221, 238, 255, 0, 16, 32,
                48, 64, 80, 96, 112, 128, 144, 160, 176, 192, 208, 224, 240, 15,
            ],
        )
        .build()
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{create_example_key1, create_example_key2};
    use super::*;
    use crate::sha256::test_support::sha256_foobar;
    use crate::system::properties::testing::set_property_ok_value;
    use coset::CborSerializable;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_example_uds_public_keys() -> UdsPublicKeys {
        UdsPublicKeys { keys: vec![create_example_key1(), create_example_key2()] }
    }

    #[test]
    fn get_factory_trust_zero() -> anyhow::Result<()> {
        set_property_ok_value(SYSTEM_PROP_FACTORY_TRUST, ZERO);
        assert_eq!(get_factory_trust()?, None);
        Ok(())
    }

    #[test]
    fn get_factory_trust_valid() -> anyhow::Result<()> {
        // sha256("foobar") as a HEX string
        let hex_sha256_foobar = "c3ab8ff13720e8ad9047dd39466b3c8974e592c2fa383d4a3960714caef0c4f2";

        set_property_ok_value(SYSTEM_PROP_FACTORY_TRUST, hex_sha256_foobar);
        assert_eq!(get_factory_trust()?, Some(sha256_foobar()));
        Ok(())
    }

    #[test]
    fn load_no_factory_trust() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let uds_pubs_path = tmp_dir.path().join("uds_pubs");
        {
            let mut file = File::create(&uds_pubs_path)?;
            file.write_all(&(create_example_uds_public_keys().to_vec()?))?;
        }

        let uds_pubs = UdsPubs::load_with_params(&uds_pubs_path, None)?;

        assert!(!uds_pubs.trusted);

        Ok(())
    }

    #[test]
    fn load_matching_factory_trust() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let uds_pubs_path = tmp_dir.path().join("uds_pubs");
        let uds_pubs_bytes = create_example_uds_public_keys().to_vec()?;
        {
            let mut file = File::create(&uds_pubs_path)?;
            file.write_all(&uds_pubs_bytes)?;
        }

        let factory_trust = Some(sha256(&uds_pubs_bytes)?);
        let uds_pubs = UdsPubs::load_with_params(&uds_pubs_path, factory_trust)?;

        assert!(uds_pubs.trusted);

        Ok(())
    }

    #[test]
    fn load_wrong_factory_trust() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new()?;
        let uds_pubs_path = tmp_dir.path().join("uds_pubs");
        {
            let mut file = File::create(&uds_pubs_path)?;
            let uds_pubs_bytes = create_example_uds_public_keys().to_vec()?;
            file.write_all(&uds_pubs_bytes)?;
        }

        let uds_pubs = UdsPubs::load_with_params(&uds_pubs_path, Some(sha256_foobar()))?;

        assert!(!uds_pubs.trusted);

        Ok(())
    }
}
