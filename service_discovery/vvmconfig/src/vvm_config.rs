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

//! Provides the VvmConfig data structure along with logic to serialize and deserialize
//! it to/from CBOR and JSON.
//! The CBOR format is described in vvmconfig.cddl

use crate::dice_policy_json::{json_to_dice_policy, DicePolicyJson};
use crate::error::semantic_error;
use crate::json_web_key::{jwk_to_cose_key, JsonWebKey};
use crate::vm_config::VmConfig;
use ciborium::Value;
use coset::{AsCborValue, CborSerializable, CoseError, CoseKey};
use dice_policy::DicePolicy;
use hwtrust::publickey::PublicKey;
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;
use CoseError::EncodeFailed;

/// Version of the VvmConfig spec
const VVMCONFIG_VERSION: u32 = 1;

/// Vehicle Virtual Machine Configuration
///
/// Configuration for all the SDV VMs of a vehicle.
#[derive(Clone, Debug, PartialEq)]
pub struct VvmConfig {
    /// Public key of the UDS root provisioning authority
    pub uds_ca_pub: PublicKey,

    /// Revocation list of intermediate CAs of UDS certificates
    pub uds_ca_rev_list: Vec<CoseKey>,

    /// Array of DICE policies used to verify DICE chains of SDV VMs.
    ///
    /// The `android` and `secureWorld` fields of `VmConfig` are indexes
    /// to this array.
    pub policies: Vec<DicePolicy>,

    /// Maps an SDV VM instance name ot its configuration.
    pub vm_configs: BTreeMap<String, VmConfig>,
}

impl VvmConfig {
    /// Creates a VvmConfig from a path to a vvmconfig file.
    pub fn from_path(path: &std::path::Path) -> anyhow::Result<Self> {
        let cbor = std::fs::read(path)?;

        Ok(VvmConfig::from_slice(&cbor)?)
    }
}

// Must be kept in sync with vvmconfig.cddl
impl AsCborValue for VvmConfig {
    fn from_cbor_value(value: Value) -> Result<Self, CoseError> {
        let [version, uds_ca_pub, uds_ca_rev_list, policies, vm_configs] = value
            .into_array()
            .map_err(|_| semantic_error("VvmConfig must be an array"))?
            .try_into()
            .map_err(|_| semantic_error("VvmConfig array must have 5 items"))?;

        let version: u32 = version
            .into_integer()
            .map_err(|_| semantic_error("VmConfig::version is not an integer"))?
            .try_into()
            .map_err(|_| semantic_error("VmConfig::version doesn't fit into an u32"))?;

        if version != VVMCONFIG_VERSION {
            return Err(semantic_error("Unsupported VvmConfig version"));
        }

        let uds_ca_pub = PublicKey::from_cose_key(
            &CoseKey::from_cbor_value(uds_ca_pub)
                .map_err(|_| semantic_error("VvmConfig::uds_ca_pub must be a CoseKey"))?,
        )
        .map_err(|_| semantic_error("VvmConfig::uds_ca_pub invalid key"))?;

        let uds_ca_rev_list = uds_ca_rev_list
            .into_array()
            .map_err(|_| semantic_error("VvmConfig::uds_ca_rev_list must be an array"))?
            .into_iter()
            .map(CoseKey::from_cbor_value)
            .collect::<Result<Vec<CoseKey>, CoseError>>()?;

        let policies = policies
            .into_array()
            .map_err(|_| semantic_error("VvmConfig::policies must be an array"))?
            .into_iter()
            .map(DicePolicy::from_cbor_value)
            .collect::<Result<Vec<DicePolicy>, CoseError>>()?;

        let vm_configs_iter = vm_configs
            .into_map()
            .map_err(|_| semantic_error("VvmConfig::vm_configs must be a map"))?
            .into_iter();

        let mut vm_configs = BTreeMap::new();

        for (value_key, value_vm_config) in vm_configs_iter {
            let key: String = value_key.into_text().map_err(|v| {
                semantic_error(
                    format!("VvmConfig::vm_configs map key must be a tstr, but got {v:?}").as_str(),
                )
            })?;
            let vm_config = VmConfig::from_cbor_value(value_vm_config)?;
            vm_configs.insert(key, vm_config);
        }

        Ok(VvmConfig { uds_ca_pub, uds_ca_rev_list, policies, vm_configs })
    }

    fn to_cbor_value(self) -> Result<Value, CoseError> {
        let mut config_map: Vec<(Value, Value)> = Vec::new();

        for (name, config) in self.vm_configs.into_iter() {
            config_map.push((Value::Text(name), config.to_cbor_value()?));
        }

        Ok(Value::Array(vec![
            Value::from(VVMCONFIG_VERSION),
            self.uds_ca_pub.to_cose_key().map_err(|_| EncodeFailed)?.to_cbor_value()?,
            Value::Array(
                self.uds_ca_rev_list
                    .into_iter()
                    .map(CoseKey::to_cbor_value)
                    .collect::<Result<Vec<Value>, CoseError>>()?,
            ),
            Value::Array(
                self.policies
                    .into_iter()
                    .map(DicePolicy::to_cbor_value)
                    .collect::<Result<Vec<Value>, CoseError>>()?,
            ),
            Value::Map(config_map),
        ]))
    }
}

impl CborSerializable for VvmConfig {}

// Ideally we would use vvmconfig::VvmConfig. But that contains CBOR-specific structs like
// COSE Keys.
//
// The equivalent of COSE Keys in JSON are JSON Web Keys (JWK).  Each is covered by a
// different specification and rust module.
//
// As we are not performing any operation with the JWK there's no need to use a JWK
// module. Parsing from JSON trivial enough.
//
// DICE policies could be deserialized directly from JSON with just some small changes in
// the dice_policy crate. But that would lead to less readable JSON files. Constraint
// types, for instance, would be represented by integers instead of human-readable
// strings like "ExactMatch".
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct VvmConfigJson {
    pub uds_ca_pub: JsonWebKey,
    pub uds_ca_rev_list: Vec<JsonWebKey>,
    pub policies: Vec<DicePolicyJson>,
    pub vm_configs: BTreeMap<String, VmConfig>,
}

impl std::convert::TryFrom<VvmConfigJson> for VvmConfig {
    type Error = anyhow::Error;

    fn try_from(vvmconfig_json: VvmConfigJson) -> Result<Self, Self::Error> {
        Ok(VvmConfig {
            uds_ca_pub: PublicKey::from_cose_key(&jwk_to_cose_key(vvmconfig_json.uds_ca_pub)?)?,
            uds_ca_rev_list: vvmconfig_json
                .uds_ca_rev_list
                .into_iter()
                .map(jwk_to_cose_key)
                .collect::<Result<_, _>>()?,
            policies: vvmconfig_json
                .policies
                .into_iter()
                .map(json_to_dice_policy)
                .collect::<Result<_, _>>()?,
            vm_configs: vvmconfig_json.vm_configs,
        })
    }
}

impl<'de> Deserialize<'de> for VvmConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        let vvmconfig_json = VvmConfigJson::deserialize(deserializer)?;

        VvmConfig::try_from(vvmconfig_json).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::VvmConfig;
    use super::{VvmConfigJson, VVMCONFIG_VERSION};
    use crate::{IpAndPort, VmConfig};
    use anyhow::{anyhow, bail, Context};
    use coset::{iana::EllipticCurve, AsCborValue};
    use coset::{CborSerializable, CoseKeyBuilder};
    use dice_policy::{
        Constraint, DicePolicy, NodeConstraints, EXACT_MATCH_CONSTRAINT,
        GREATER_OR_EQUAL_CONSTRAINT,
    };
    use hwtrust::publickey::PublicKey;
    use std::collections::BTreeMap;
    use std::net::{IpAddr, Ipv4Addr};

    /// Tests serialization of the VvmConfig to CBOR and deserialization from
    /// CBOR back to the VvmConfig struct.
    #[test]
    fn to_cbor_and_back() -> anyhow::Result<()> {
        let vvmconfig = create_example_vvmconfig()?;

        let cbor: Vec<u8> =
            vvmconfig.clone().to_vec().context("Failed to serialize vvmconfig to CBOR")?;

        let deserialized_vvmconfig =
            VvmConfig::from_slice(&cbor).context("Failed to deserialize vvmconfig from CBOR")?;

        assert_eq!(vvmconfig, deserialized_vvmconfig);

        Ok(())
    }

    // Must be kept in sync with create_example_vvmconfig()
    static VVMCONFIG_JSON: &str = r#"
      {
        "udsCaPub": {
          "kty": "EC",
          "use": "sig",
          "crv": "P-256",
          "x": "InE8FQWvpuPf7kxV7ZXvBLkNBpbohodkdiwIVSJ0TgQ",
          "y": "g-Uz4-uw7vzH_PSKF4OyZ67wNLDxghkkwLRjJSOpw5M"
        },
        "udsCaRevList": [],
        "policies": [
          [
            [
              {"ExactMatch" : { "key_spec": [12], "value": {"Bytes": "SGVsbG8gV29ybGQh"}}},
              {"Ge" : { "key_spec": [123], "value": 37}}
            ]
          ],
          [
            [
              {"ExactMatch" : { "key_spec": [12], "value": {"Bytes": "Rm9vIEJhcg"}}},
              {"Ge" : { "key_spec": [98], "value": 11}}
            ]
          ]
        ],
        "vmConfigs": {
          "foo": {
            "ips": [
              { "ip": "192.168.1.1", "port": 2000 }
            ],
            "android": 0,
            "secureWorld": 1
          },
          "bar": {
            "ips": [
              { "ip": "192.168.1.2" }
            ],
            "android": 0,
            "secureWorld": 1
          }
        }
      }"#;

    #[test]
    fn vvmconfig_from_json() -> anyhow::Result<()> {
        // Create a VvmConfig from the JSON string

        let vvmconfig_json: VvmConfigJson =
            serde_json::from_str(VVMCONFIG_JSON).context("Failed to parse JSON string")?;
        let generated_vvmconfig = VvmConfig::try_from(vvmconfig_json)
            .context("Failed to create VvmConfig from VvmConfigJson")?;

        // Manually build the VvmConfig we expect to get from that JSON

        let expected_vvmconfig = create_example_vvmconfig()?;

        assert_eq!(
            generated_vvmconfig, expected_vvmconfig,
            "Generated VvmConfig didn't match expectations"
        );

        Ok(())
    }

    // Creates an example VvmConfig.
    //
    // Must be kept in sync with VVMCONFIG_JSON
    fn create_example_vvmconfig() -> anyhow::Result<VvmConfig> {
        let mut expected_vm_configs = BTreeMap::new();
        expected_vm_configs.insert(
            "foo".to_owned(),
            VmConfig {
                ips: vec![IpAndPort {
                    ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
                    port: Some(2000),
                }],
                android: 0,
                secure_world: 1,
            },
        );
        expected_vm_configs.insert(
            "bar".to_owned(),
            VmConfig {
                ips: vec![IpAndPort { ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), port: None }],
                android: 0,
                secure_world: 1,
            },
        );

        let android_policy = DicePolicy {
            version: 1,
            node_constraints_list: vec![NodeConstraints(
                vec![
                    Constraint::new(
                        EXACT_MATCH_CONSTRAINT,
                        vec![12],
                        ciborium::Value::from("Hello World!".as_bytes()),
                    )
                    .map_err(|s| anyhow!("Constraint creation failed: {}", s))?,
                    Constraint::new(
                        GREATER_OR_EQUAL_CONSTRAINT,
                        vec![123],
                        ciborium::Value::from(37),
                    )
                    .map_err(|s| anyhow!("Constraint creation failed: {}", s))?,
                ]
                .into_boxed_slice(),
            )]
            .into_boxed_slice(),
        };

        let secure_world_policy = DicePolicy {
            version: 1,
            node_constraints_list: vec![NodeConstraints(
                vec![
                    Constraint::new(
                        EXACT_MATCH_CONSTRAINT,
                        vec![12],
                        ciborium::Value::from("Foo Bar".as_bytes()),
                    )
                    .map_err(|s| anyhow!("Constraint creation failed: {}", s))?,
                    Constraint::new(
                        GREATER_OR_EQUAL_CONSTRAINT,
                        vec![98],
                        ciborium::Value::from(11),
                    )
                    .map_err(|s| anyhow!("Constraint creation failed: {}", s))?,
                ]
                .into_boxed_slice(),
            )]
            .into_boxed_slice(),
        };

        Ok(VvmConfig {
            uds_ca_pub: PublicKey::from_cose_key(
                &CoseKeyBuilder::new_ec2_pub_key(
                    EllipticCurve::P_256,
                    vec![
                        34, 113, 60, 21, 5, 175, 166, 227, 223, 238, 76, 85, 237, 149, 239, 4, 185,
                        13, 6, 150, 232, 134, 135, 100, 118, 44, 8, 85, 34, 116, 78, 4,
                    ],
                    vec![
                        131, 229, 51, 227, 235, 176, 238, 252, 199, 252, 244, 138, 23, 131, 178,
                        103, 174, 240, 52, 176, 241, 130, 25, 36, 192, 180, 99, 37, 35, 169, 195,
                        147,
                    ],
                )
                .algorithm(coset::iana::Algorithm::ES256)
                .build(),
            )?,
            uds_ca_rev_list: vec![],
            policies: vec![android_policy, secure_world_policy],
            vm_configs: expected_vm_configs,
        })
    }

    #[test]
    fn unsupported_version_fails() -> anyhow::Result<()> {
        let vvmconfig = create_example_vvmconfig()?;

        // Get the CBOR value representation
        let mut cbor_value = vvmconfig.to_cbor_value().context("Failed to create cbor value")?;

        // Tamper with the version
        if let ciborium::Value::Array(values) = &mut cbor_value {
            // The version is the first element.
            values[0] = ciborium::Value::from(VVMCONFIG_VERSION + 1);
        } else {
            return Err(anyhow!("Expected VvmConfig to be a CBOR array"));
        }

        // Serialize the tampered value to bytes
        let mut cbor_bytes = Vec::new();
        ciborium::into_writer(&cbor_value, &mut cbor_bytes)?;

        // Try to deserialize and expect an error
        let result = VvmConfig::from_slice(&cbor_bytes);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported VvmConfig version"));
        Ok(())
    }

    #[test]
    fn vm_configs_key_not_string_fails() -> anyhow::Result<()> {
        let mut cbor_value = create_example_vvmconfig()?.to_cbor_value()?;

        // Tamper with the vm_configs map key
        if let ciborium::Value::Array(values) = &mut cbor_value {
            // vm_configs is the 5th element.
            let map_values = values
                .get_mut(4)
                .expect("VvmConfig CBOR array should have 5 items")
                .as_map_mut()
                .expect("The fith element of the VvmConfig CBOR array should be a CBOR map");

            assert!(!map_values.is_empty());

            // Replace the key of the first element with an integer
            // Note that a ciborium map is a Vec<(key, value)>
            map_values[0].0 = ciborium::Value::from(123);
        } else {
            bail!("Expected VvmConfig to be a CBOR array");
        }

        // Serialize the tampered value to bytes
        let mut cbor_bytes = Vec::new();
        ciborium::into_writer(&cbor_value, &mut cbor_bytes)?;

        // Try to deserialize and expect an error
        let result = VvmConfig::from_slice(&cbor_bytes);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("VvmConfig::vm_configs map key must be a tstr"));

        Ok(())
    }
}
