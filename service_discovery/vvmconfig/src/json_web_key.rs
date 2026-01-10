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

use anyhow::anyhow;
use coset::iana::{EllipticCurve, OkpKeyParameter};
use coset::{CoseKey, CoseKeyBuilder};
use serde::Deserialize;

/// A JSON Web Key
///
/// We only care are about parameters used by keys supported in the Android
/// Profile for DICE.
///
/// See https://www.iana.org/assignments/jose/jose.xhtml#web-key-parameters for
/// the full list of parameters.
#[derive(Debug, Deserialize, Default)]
pub struct JsonWebKey {
    pub kty: String,
    pub crv: Option<String>,

    #[serde(with = "option_base64")]
    pub x: Option<Vec<u8>>,

    #[serde(with = "option_base64")]
    pub y: Option<Vec<u8>>,
}

mod option_base64 {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use serde::Deserialize;
    use serde::Deserializer;

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let base64 = <Option<String>>::deserialize(d)?;
        match base64 {
            Some(v) => {
                URL_SAFE_NO_PAD.decode(v.as_bytes()).map(Some).map_err(serde::de::Error::custom)
            }
            None => Ok(None),
        }
    }
}

/// Converts a JSON Web Key to a COSE Key
///
/// It fails if the key type is not supported by the Android Profile for DICE.
pub fn jwk_to_cose_key(jwk: JsonWebKey) -> anyhow::Result<CoseKey> {
    match (jwk.kty.as_str(), jwk.crv.as_deref(), jwk.x, jwk.y) {
        ("EC", Some("P-256"), Some(x), Some(y)) => {
            Ok(CoseKeyBuilder::new_ec2_pub_key(EllipticCurve::P_256, x, y)
                .algorithm(coset::iana::Algorithm::ES256)
                .build())
        }
        ("EC", Some("P-384"), Some(x), Some(y)) => {
            Ok(CoseKeyBuilder::new_ec2_pub_key(EllipticCurve::P_384, x, y)
                .algorithm(coset::iana::Algorithm::ES384)
                .build())
        }
        ("OKP", Some("Ed25519"), Some(x), None) => Ok(CoseKeyBuilder::new_okp_key()
            .param(
                OkpKeyParameter::Crv as i64,
                ciborium::Value::from(EllipticCurve::Ed25519 as u64),
            )
            .param(OkpKeyParameter::X as i64, ciborium::Value::from(x))
            .algorithm(coset::iana::Algorithm::EdDSA)
            .build()),
        _ => Err(anyhow!("Key type or curve not supported by the Android Profile for DICE")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwk_to_cose_key_ec_p256_ok() {
        let x = vec![1; 32];
        let y = vec![2; 32];
        let jwk = JsonWebKey {
            kty: "EC".to_string(),
            crv: Some("P-256".to_string()),
            x: Some(x.clone()),
            y: Some(y.clone()),
        };

        let cose_key = jwk_to_cose_key(jwk).unwrap();

        let expected_cose_key = CoseKeyBuilder::new_ec2_pub_key(EllipticCurve::P_256, x, y)
            .algorithm(coset::iana::Algorithm::ES256)
            .build();

        assert_eq!(cose_key, expected_cose_key);
    }

    #[test]
    fn test_jwk_to_cose_key_ec_p384_ok() {
        let x = vec![1; 48];
        let y = vec![2; 48];
        let jwk = JsonWebKey {
            kty: "EC".to_string(),
            crv: Some("P-384".to_string()),
            x: Some(x.clone()),
            y: Some(y.clone()),
        };

        let cose_key = jwk_to_cose_key(jwk).unwrap();

        let expected_cose_key = CoseKeyBuilder::new_ec2_pub_key(EllipticCurve::P_384, x, y)
            .algorithm(coset::iana::Algorithm::ES384)
            .build();

        assert_eq!(cose_key, expected_cose_key);
    }

    #[test]
    fn test_jwk_to_cose_key_okp_ed25519_ok() {
        let x = vec![1; 32];
        let jwk = JsonWebKey {
            kty: "OKP".to_string(),
            crv: Some("Ed25519".to_string()),
            x: Some(x.clone()),
            y: None,
        };

        let cose_key = jwk_to_cose_key(jwk).unwrap();

        let expected_cose_key = CoseKeyBuilder::new_okp_key()
            .param(
                OkpKeyParameter::Crv as i64,
                ciborium::Value::from(EllipticCurve::Ed25519 as u64),
            )
            .param(OkpKeyParameter::X as i64, ciborium::Value::from(x))
            .algorithm(coset::iana::Algorithm::EdDSA)
            .build();

        assert_eq!(cose_key, expected_cose_key);
    }

    #[test]
    fn test_jwk_to_cose_key_unsupported_kty_err() {
        let jwk = JsonWebKey {
            kty: "RSA".to_string(),
            crv: Some("P-256".to_string()),
            x: Some(vec![1; 32]),
            y: Some(vec![2; 32]),
        };

        let result = jwk_to_cose_key(jwk);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Key type or curve not supported by the Android Profile for DICE"
        );
    }

    #[test]
    fn test_jwk_to_cose_key_unsupported_crv_err() {
        let jwk = JsonWebKey {
            kty: "EC".to_string(),
            crv: Some("secp256k1".to_string()),
            x: Some(vec![1; 32]),
            y: Some(vec![2; 32]),
        };

        let result = jwk_to_cose_key(jwk);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Key type or curve not supported by the Android Profile for DICE"
        );
    }

    #[test]
    fn test_jwk_to_cose_key_ec_missing_params_err() {
        let jwk_missing_x = JsonWebKey {
            kty: "EC".to_string(),
            crv: Some("P-256".to_string()),
            x: None,
            y: Some(vec![2; 32]),
        };
        assert!(jwk_to_cose_key(jwk_missing_x).is_err());

        let jwk_missing_y = JsonWebKey {
            kty: "EC".to_string(),
            crv: Some("P-256".to_string()),
            x: Some(vec![1; 32]),
            y: None,
        };
        assert!(jwk_to_cose_key(jwk_missing_y).is_err());

        let jwk_okp_with_y = JsonWebKey {
            kty: "OKP".to_string(),
            crv: Some("Ed25519".to_string()),
            x: Some(vec![1; 32]),
            y: Some(vec![2; 32]),
        };
        assert!(jwk_to_cose_key(jwk_okp_with_y).is_err());
    }
}
