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

use anyhow::Context;
use ciborium::de::Error;
use coset::{CoseError, CoseKey};
use hwtrust::publickey::PublicKey;
use openssl::pkey::{PKey, Public};
use CoseError::DecodeFailed;

/// Helper function for creating a semantic CoseError.
pub(crate) fn ce(s: &str) -> CoseError {
    DecodeFailed(Error::semantic(None, s))
}

/// Converts a public CoseKey into an OpenSSL PKey.
/// Only supports EC (P-256, P-384) and OKP (Ed25519) keys.
pub fn ec_or_okp_cose_key_to_pkey(cose_key: &CoseKey) -> anyhow::Result<PKey<Public>> {
    // PublicKey::from_cose_key() only supports EC (P-256, P-384) and OKP (Ed25519) keys.
    let libhwtrust_key = PublicKey::from_cose_key(cose_key)
        .context("Failed to convert COSE key to libhwtrust key")?;
    Ok(libhwtrust_key.pkey().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use coset::CoseKeyBuilder;
    use openssl::ec::{EcGroup, EcKey};
    use openssl::nid::Nid;

    fn build_ec2_p256_key_using_coset() -> CoseKey {
        use coset::iana::{Algorithm, EllipticCurve};

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
        .algorithm(Algorithm::ES256)
        .build()
    }

    fn build_okp_key_using_coset() -> CoseKey {
        use ciborium::Value;
        use coset::iana::{Algorithm, EllipticCurve, EnumI64, KeyOperation, OkpKeyParameter};

        let x = vec![
            99, 58, 151, 135, 192, 64, 51, 139, 145, 37, 3, 246, 48, 233, 223, 126, 147, 15, 151,
            86, 221, 27, 53, 102, 252, 25, 69, 75, 124, 94, 104, 102,
        ];
        let label_crv = OkpKeyParameter::Crv.to_i64();
        let label_x = OkpKeyParameter::X.to_i64();
        CoseKeyBuilder::new_okp_key()
            .param(label_crv, Value::from(EllipticCurve::Ed25519.to_i64()))
            .param(label_x, Value::from(x))
            .algorithm(Algorithm::EdDSA)
            .add_key_op(KeyOperation::Verify)
            .build()
    }

    #[test]
    fn can_convert_ec2_p256_key_built_using_cose() -> anyhow::Result<()> {
        let pkey = ec_or_okp_cose_key_to_pkey(&build_ec2_p256_key_using_coset())?;

        let _ec_key = pkey.ec_key()?;

        Ok(())
    }

    #[test]
    fn can_convert_ec2_p256_key() -> anyhow::Result<()> {
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
        let ec_key = EcKey::generate(&group)?;
        let pkey = PKey::from_ec_key(
            EcKey::from_public_key(&group, ec_key.public_key()).context("EcKey from EcPoint")?,
        )
        .context("PKey from EcKey")?;
        let public_key: PublicKey = pkey.clone().try_into()?;

        let pkey_result = ec_or_okp_cose_key_to_pkey(&public_key.to_cose_key()?)?;

        let _ec_key = pkey.ec_key()?;

        assert!(pkey_result.public_eq(&pkey));

        Ok(())
    }

    #[test]
    fn can_convert_ec2_p384_key() -> anyhow::Result<()> {
        let group = EcGroup::from_curve_name(Nid::SECP384R1)?;
        let ec_key = EcKey::generate(&group)?;
        let pkey = PKey::from_ec_key(
            EcKey::from_public_key(&group, ec_key.public_key()).context("EcKey from EcPoint")?,
        )
        .context("PKey from EcKey")?;
        let public_key: PublicKey = pkey.clone().try_into()?;

        let pkey_result = ec_or_okp_cose_key_to_pkey(&public_key.to_cose_key()?)?;

        let _ec_key = pkey.ec_key()?;

        assert!(pkey_result.public_eq(&pkey));

        Ok(())
    }

    #[test]
    fn can_convert_eddsa_key() -> anyhow::Result<()> {
        let _pkey = ec_or_okp_cose_key_to_pkey(&build_okp_key_using_coset())?;

        Ok(())
    }

    #[test]
    fn cannot_convert_rsa_key() -> anyhow::Result<()> {
        use coset::iana::{KeyOperation, KeyType};
        // This is an incomeplete key declaration, but the KeyType is set, which is sufficient for
        // the purposes of this test.
        let rsa_cose_key = CoseKeyBuilder::new()
            .kty(coset::KeyType::Assigned(KeyType::RSA))
            .add_key_op(KeyOperation::Verify)
            .build();
        let err = ec_or_okp_cose_key_to_pkey(&rsa_cose_key).expect_err("Error expected");
        assert!(
            err.root_cause().to_string().contains("Unexpected KeyType"),
            "Received some other error than expected: {err:#?}"
        );

        Ok(())
    }
}
