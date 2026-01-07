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

use super::sd_certificate_claims::SdCertificateClaims;
use super::vvm_config::VmDicePolicies;
use super::VvmConfig;

use anyhow::anyhow;
use ciborium::Value;
use coset::CborSerializable;

/// Checks the DICE chains against local DICE policies
pub(crate) fn check_dice_policy(
    claims: &SdCertificateClaims,
    vvm_config: &VvmConfig,
) -> anyhow::Result<()> {
    let VmDicePolicies { android_sdv_dice_policy, secure_world_dice_policy } =
        vvm_config.get_dice_policies(&claims.vm_name)?;

    android_sdv_dice_policy
        .matches_dice_chain(&explicitkeydice::to_explicit_chain(&claims.android_sdv)?)
        .map_err(|e| anyhow!("DICE policy mismatch: {e}"))?;

    secure_world_dice_policy
        .matches_dice_chain(&explicitkeydice::to_explicit_chain(&extract_secure_world_dice_chain(
            &claims.rkp_csr,
        )?)?)
        .map_err(|e| anyhow!("DICE policy mismatch: {e}"))?;

    Ok(())
}

/// Extracts the secure world dice chain from the Certificate Signing Request (CSR),
/// as defined in generateCertificateRequestV2.cddl.
fn extract_secure_world_dice_chain(certificate_signing_request: &[u8]) -> anyhow::Result<Vec<u8>> {
    let cert_sign_request = Value::from_slice(certificate_signing_request)?
        .into_array()
        .map_err(|_| anyhow!("Failed to convert certificate signing request to Array"))?;
    const DICE_CHAIN_INDEX: usize = 2;
    Ok(cert_sign_request
        .get(DICE_CHAIN_INDEX)
        .ok_or(anyhow!("Failed to find DiceCertChain from AuthenticatedRequest<T>"))?
        .clone()
        .to_vec()?)
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::mesh::credentials_checker::tests::create_secure_world_signer_cert_der;
    use crate::mesh::dice_chains::AuthenticatedCsr;
    use crate::mesh::vvm_config::{create_test_uds_ca_pub, VmConfig};
    use anyhow::Ok;
    use coset::CoseSign1;
    use crypto_rpc::cert::{build_ec_private_key, public_from_private};
    use dice_policy::{
        Constraint, DicePolicy, NodeConstraints, EXACT_MATCH_CONSTRAINT,
        GREATER_OR_EQUAL_CONSTRAINT,
    };
    use kmr_common::vec_try;
    use openssl::pkey::{PKey, Private};
    use sample_dice_handover::SdvDiceArtifactsBuilder;
    use sdv_comms_id::names::VmName;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use vvmtruststore::{CertificateRevocationList, ISS, KEY_USAGE, SUB};

    use super::*;

    const CODE_HASH_KEY: i64 = -4670545;
    const CONFIG_DESCRIPTOR_KEY: i64 = -4670548;
    const VERIFIED_BOOT_STATE_KEY: i64 = -71000;
    const BUILD_FINGERPRINT_KEY: i64 = -71001;
    const COMPONENT_NAME_KEY: i64 = -70002;
    const COMPONENT_VERSION_KEY: i64 = -70003;
    const SECURITY_VERSION_KEY: i64 = -70005;
    const SYSTEM_EXT_SECURITY_PATCH_KEY: i64 = -71002;
    const PRODUCT_SECURITY_PATCH_KEY: i64 = -71003;
    const VENDOR_SECURITY_PATCH_KEY: i64 = -71004;
    const BOOT_SECURITY_PATCH_KEY: i64 = -71005;

    fn generate_android_sdv_dice_chain() -> anyhow::Result<Vec<u8>> {
        let dice_artifacts = SdvDiceArtifactsBuilder::new_with_sample_values().build()?;

        Ok(dice_artifacts.dice_chain().to_vec())
    }

    fn generate_secure_world_dice_chain() -> anyhow::Result<Vec<u8>> {
        Ok(AuthenticatedCsr::create()?.cbor)
    }

    fn create_sd_certificate_claims(vm_name: &str) -> anyhow::Result<SdCertificateClaims> {
        let android_sdv = generate_android_sdv_dice_chain()?;
        let android_sdv_signer_certificate = CoseSign1::default();

        let sd_private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let subject_public_key = public_from_private(&sd_private_key)?;
        let vm_name: VmName = vm_name.to_owned().try_into()?;

        Ok(SdCertificateClaims {
            android_sdv,
            android_sdv_signer_certificate,
            rkp_csr: generate_secure_world_dice_chain()?,
            secure_world_signer_certificate_der: create_secure_world_signer_cert_der()?,
            subject_public_key,
            uds_certs_cbor: None,
            vm_name,
        })
    }

    fn build_vvm_config(
        android_sdv_dice_policy: DicePolicy,
        secure_world_dice_policy: DicePolicy,
        vm_name: &str,
    ) -> anyhow::Result<VvmConfig> {
        let uds_ca_pub = create_test_uds_ca_pub()?;
        let uds_ca_rev_list = CertificateRevocationList::default();
        let policies = vec![android_sdv_dice_policy, secure_world_dice_policy];

        let remote_cfg = VmConfig {
            name: vm_name.to_owned().try_into()?,
            // Arbitrary address is used which doesn't affect test result
            addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 98, 3)), 2001),
            android: 0,
            secure_world: 1,
        };
        let remote_cfgs = vec![remote_cfg];

        let own_cfg = VmConfig {
            name: "self".to_owned().try_into()?,
            // Arbitrary address is used which doesn't affect test result
            addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 98, 3)), 2001),
            android: 0,
            secure_world: 1,
        };

        Ok(VvmConfig { uds_ca_pub, uds_ca_rev_list, policies, remote_cfgs, own_cfg })
    }

    fn empty_constraint() -> NodeConstraints {
        NodeConstraints(vec![].into_boxed_slice())
    }

    fn greater_or_equal_constraint(val: i64) -> anyhow::Result<NodeConstraints> {
        Ok(NodeConstraints(
            vec![Constraint::new(GREATER_OR_EQUAL_CONSTRAINT, vec![], ciborium::Value::from(val))
                .map_err(|s| anyhow!(s))?]
            .into_boxed_slice(),
        ))
    }

    fn create_empty_policy(layer_count: usize) -> DicePolicy {
        let node_constraints_list =
            vec![NodeConstraints(vec![].into_boxed_slice(),); layer_count].into_boxed_slice();
        DicePolicy { version: 1, node_constraints_list }
    }

    pub(crate) fn create_good_android_sdv_policy() -> anyhow::Result<DicePolicy> {
        Ok(DicePolicy {
            version: 1,
            node_constraints_list: vec![
                // Expects to match EXPLICIT_KEY_DICE_CERT_CHAIN_VERSION dice node
                greater_or_equal_constraint(1)?,
                // Expects to match UDS public key dice node
                empty_constraint(),
                // Expects to match CDI0 dice node
                NodeConstraints(
                    vec![
                        // Code hash can be verified as an alternative to security version
                        Constraint::new(
                            EXACT_MATCH_CONSTRAINT,
                            vec![CODE_HASH_KEY],
                            ciborium::Value::Bytes(vec![
                                22, 72, 242, 85, 83, 35, 221, 21, 46, 131, 56, 195, 100, 56, 99,
                                38, 15, 207, 91, 209, 58, 211, 64, 62, 35, 248, 52, 76, 109, 162,
                                190, 37, 28, 176, 41, 232, 195, 251, 184, 128, 220, 177, 210, 179,
                                145, 77, 211, 251, 1, 15, 228, 233, 70, 162, 192, 38, 87, 90, 186,
                                48, 247, 21, 152, 20,
                            ]),
                        )
                        .map_err(|s| anyhow!(s))?,
                        // Component name == "ABL"
                        Constraint::new(
                            EXACT_MATCH_CONSTRAINT,
                            vec![CONFIG_DESCRIPTOR_KEY, COMPONENT_NAME_KEY],
                            ciborium::Value::Text("ABL".to_string()),
                        )
                        .map_err(|s| anyhow!(s))?,
                    ]
                    .into_boxed_slice(),
                ),
                // Expects to match CDI1 dice node
                NodeConstraints(
                    vec![
                        // Security version >= 2
                        Constraint::new(
                            GREATER_OR_EQUAL_CONSTRAINT,
                            vec![CONFIG_DESCRIPTOR_KEY, SECURITY_VERSION_KEY],
                            ciborium::Value::from(2),
                        )
                        .map_err(|s| anyhow!(s))?,
                        // Component name == "AVB"
                        Constraint::new(
                            EXACT_MATCH_CONSTRAINT,
                            vec![CONFIG_DESCRIPTOR_KEY, COMPONENT_NAME_KEY],
                            ciborium::Value::Text("AVB".to_string()),
                        )
                        .map_err(|s| anyhow!(s))?,
                    ]
                    .into_boxed_slice(),
                ),
                // Expects to match Android SDV (leaf) dice node
                NodeConstraints(
                    vec![
                        // Component version (system OS version) >= 15
                        Constraint::new(
                            GREATER_OR_EQUAL_CONSTRAINT,
                            vec![CONFIG_DESCRIPTOR_KEY, COMPONENT_VERSION_KEY],
                            ciborium::Value::from(15),
                        )
                        .map_err(|s| anyhow!(s))?,
                        // Security version (system security patch level) >= 20241105
                        Constraint::new(
                            GREATER_OR_EQUAL_CONSTRAINT,
                            vec![CONFIG_DESCRIPTOR_KEY, SECURITY_VERSION_KEY],
                            ciborium::Value::from(20241105),
                        )
                        .map_err(|s| anyhow!(s))?,
                        // Verified boot state == "green"
                        Constraint::new(
                            EXACT_MATCH_CONSTRAINT,
                            vec![CONFIG_DESCRIPTOR_KEY, VERIFIED_BOOT_STATE_KEY],
                            ciborium::Value::Text("green".to_string()),
                        )
                        .map_err(|s| anyhow!(s))?,
                        // Build fingerprint == "a build fingerprint"
                        Constraint::new(
                            EXACT_MATCH_CONSTRAINT,
                            vec![CONFIG_DESCRIPTOR_KEY, BUILD_FINGERPRINT_KEY],
                            ciborium::Value::Text("a build fingerprint".to_string()),
                        )
                        .map_err(|s| anyhow!(s))?,
                        // system_ext security patch level >= 20241105
                        Constraint::new(
                            GREATER_OR_EQUAL_CONSTRAINT,
                            vec![CONFIG_DESCRIPTOR_KEY, SYSTEM_EXT_SECURITY_PATCH_KEY],
                            ciborium::Value::from(20241105),
                        )
                        .map_err(|s| anyhow!(s))?,
                        // product security patch level >= 20241105
                        Constraint::new(
                            GREATER_OR_EQUAL_CONSTRAINT,
                            vec![CONFIG_DESCRIPTOR_KEY, PRODUCT_SECURITY_PATCH_KEY],
                            ciborium::Value::from(20241105),
                        )
                        .map_err(|s| anyhow!(s))?,
                        // vendor security patch level >= 20241105
                        Constraint::new(
                            GREATER_OR_EQUAL_CONSTRAINT,
                            vec![CONFIG_DESCRIPTOR_KEY, VENDOR_SECURITY_PATCH_KEY],
                            ciborium::Value::from(20241105),
                        )
                        .map_err(|s| anyhow!(s))?,
                        // boot security patch level >= 20241105
                        Constraint::new(
                            GREATER_OR_EQUAL_CONSTRAINT,
                            vec![CONFIG_DESCRIPTOR_KEY, BOOT_SECURITY_PATCH_KEY],
                            ciborium::Value::from(20241105),
                        )
                        .map_err(|s| anyhow!(s))?,
                    ]
                    .into_boxed_slice(),
                ),
            ]
            .into_boxed_slice(),
        })
    }

    fn create_bad_android_sdv_policy_bigger_number() -> anyhow::Result<DicePolicy> {
        Ok(DicePolicy {
            version: 1,
            node_constraints_list: vec![
                // Expects to match EXPLICIT_KEY_DICE_CERT_CHAIN_VERSION dice node
                greater_or_equal_constraint(100)?,
                // Expects to match UDS public key dice node
                empty_constraint(),
                // Expects to match CDI0 dice node
                empty_constraint(),
                // Expects to match CDI1 dice node
                empty_constraint(),
                // Expects to match Android SDV (leaf) dice node
                empty_constraint(),
            ]
            .into_boxed_slice(),
        })
    }

    fn create_bad_android_sdv_policy_string_mismatch() -> anyhow::Result<DicePolicy> {
        Ok(DicePolicy {
            version: 1,
            node_constraints_list: vec![
                // Expects to match EXPLICIT_KEY_DICE_CERT_CHAIN_VERSION dice node
                empty_constraint(),
                // Expects to match UDS public key dice node
                empty_constraint(),
                // Expects to match CDI0 dice node
                NodeConstraints(
                    vec![Constraint::new(
                        EXACT_MATCH_CONSTRAINT,
                        // For component name field
                        vec![CONFIG_DESCRIPTOR_KEY, COMPONENT_NAME_KEY],
                        ciborium::Value::Text("mismatch".to_string()),
                    )
                    .map_err(|s| anyhow!(s))?]
                    .into_boxed_slice(),
                ),
                // Expects to match CDI1 dice node
                empty_constraint(),
                // Expects to match Android SDV (leaf) dice node
                empty_constraint(),
            ]
            .into_boxed_slice(),
        })
    }

    pub(crate) fn create_good_secure_world_policy() -> anyhow::Result<DicePolicy> {
        Ok(DicePolicy {
            version: 1,
            node_constraints_list: vec![
                // Expects to match EXPLICIT_KEY_DICE_CERT_CHAIN_VERSION dice node
                greater_or_equal_constraint(1)?,
                // Expects to match UDS public key dice node
                empty_constraint(),
                // Expects to match CDI0 dice node
                NodeConstraints(
                    vec![
                        Constraint::new(
                            EXACT_MATCH_CONSTRAINT,
                            vec![ISS],
                            ciborium::Value::Text("Issuer".to_string()),
                        )
                        .map_err(|s| anyhow!(s))?,
                        Constraint::new(
                            EXACT_MATCH_CONSTRAINT,
                            vec![SUB],
                            ciborium::Value::Text("Subject".to_string()),
                        )
                        .map_err(|s| anyhow!(s))?,
                        Constraint::new(
                            EXACT_MATCH_CONSTRAINT,
                            vec![KEY_USAGE],
                            ciborium::Value::Bytes(vec_try![0x20].map_err(|s| {
                                anyhow!("Failed to create CBOR bytes for key usage field: {s:?}")
                            })?),
                        )
                        .map_err(|s| anyhow!(s))?,
                    ]
                    .into_boxed_slice(),
                ),
            ]
            .into_boxed_slice(),
        })
    }

    fn create_bad_secure_world_policy_bigger_number() -> anyhow::Result<DicePolicy> {
        Ok(DicePolicy {
            version: 1,
            node_constraints_list: vec![
                // Expects to match EXPLICIT_KEY_DICE_CERT_CHAIN_VERSION dice node
                greater_or_equal_constraint(2)?,
                // Expects to match UDS public key dice node
                empty_constraint(),
                // Expects to match CDI0 dice node
                empty_constraint(),
            ]
            .into_boxed_slice(),
        })
    }

    fn create_bad_secure_world_policy_string_mismatch() -> anyhow::Result<DicePolicy> {
        Ok(DicePolicy {
            version: 1,
            node_constraints_list: vec![
                // Expects to match EXPLICIT_KEY_DICE_CERT_CHAIN_VERSION dice node
                empty_constraint(),
                // Expects to match UDS public key dice node
                empty_constraint(),
                // Expects to match CDI0 dice node
                NodeConstraints(
                    vec![Constraint::new(
                        EXACT_MATCH_CONSTRAINT,
                        vec![KEY_USAGE],
                        ciborium::Value::Text("mismatch".to_string()),
                    )
                    .map_err(|s| anyhow!(s))?]
                    .into_boxed_slice(),
                ),
            ]
            .into_boxed_slice(),
        })
    }

    #[test]
    fn check_android_sdv_dice_policy_succeed() -> anyhow::Result<()> {
        let android_sdv_dice_policy = create_good_android_sdv_policy()?;
        let secure_world_dice_policy = create_empty_policy(3);
        let vvm_config =
            build_vvm_config(android_sdv_dice_policy, secure_world_dice_policy, "testvm")?;
        let claims = create_sd_certificate_claims("testvm")?;

        check_dice_policy(&claims, &vvm_config)?;

        Ok(())
    }

    #[test]
    fn check_android_sdv_dice_policy_fail_with_unknown_vm() -> anyhow::Result<()> {
        let android_sdv_dice_policy = create_good_android_sdv_policy()?;
        let secure_world_dice_policy = create_empty_policy(3);
        let vvm_config =
            build_vvm_config(android_sdv_dice_policy, secure_world_dice_policy, "testvm")?;
        let claims = create_sd_certificate_claims("unknown")?;

        let err = check_dice_policy(&claims, &vvm_config).expect_err("check() should have failed");

        assert_eq!(err.to_string(), "Unknown VM name: \"unknown\"");

        Ok(())
    }

    #[test]
    fn check_android_sdv_dice_policy_fail_with_smaller_number() -> anyhow::Result<()> {
        let android_sdv_dice_policy = create_bad_android_sdv_policy_bigger_number()?;
        let secure_world_dice_policy = create_empty_policy(3);
        let vvm_config =
            build_vvm_config(android_sdv_dice_policy, secure_world_dice_policy, "testvm")?;
        let claims = create_sd_certificate_claims("testvm")?;

        let err = check_dice_policy(&claims, &vvm_config).expect_err("check() should have failed");

        assert!(err.to_string().contains("DICE policy mismatch"));

        Ok(())
    }

    #[test]
    fn check_android_sdv_dice_policy_fail_with_wrong_string() -> anyhow::Result<()> {
        let android_sdv_dice_policy = create_bad_android_sdv_policy_string_mismatch()?;
        let secure_world_dice_policy = create_empty_policy(3);
        let vvm_config =
            build_vvm_config(android_sdv_dice_policy, secure_world_dice_policy, "testvm")?;
        let claims = create_sd_certificate_claims("testvm")?;

        let err = check_dice_policy(&claims, &vvm_config).expect_err("check() should have failed");

        assert!(err.to_string().contains("DICE policy mismatch"));

        Ok(())
    }

    #[test]
    fn check_secure_world_dice_policy_succeed() -> anyhow::Result<()> {
        let android_sdv_dice_policy = create_empty_policy(5);
        let secure_world_dice_policy = create_good_secure_world_policy()?;
        let vvm_config =
            build_vvm_config(android_sdv_dice_policy, secure_world_dice_policy, "testvm")?;
        let claims = create_sd_certificate_claims("testvm").unwrap();

        check_dice_policy(&claims, &vvm_config)?;

        Ok(())
    }

    #[test]
    fn check_secure_world_dice_policy_fail_with_smaller_number() -> anyhow::Result<()> {
        let android_sdv_dice_policy = create_empty_policy(5);
        let secure_world_dice_policy = create_bad_secure_world_policy_bigger_number()?;
        let vvm_config =
            build_vvm_config(android_sdv_dice_policy, secure_world_dice_policy, "testvm")?;
        let claims = create_sd_certificate_claims("testvm")?;

        let err = check_dice_policy(&claims, &vvm_config).expect_err("check() should have failed");

        assert!(err.to_string().contains("DICE policy mismatch"));

        Ok(())
    }

    #[test]
    fn check_secure_world_dice_policy_fail_with_wrong_string() -> anyhow::Result<()> {
        let android_sdv_dice_policy = create_empty_policy(5);
        let secure_world_dice_policy = create_bad_secure_world_policy_string_mismatch()?;
        let vvm_config =
            build_vvm_config(android_sdv_dice_policy, secure_world_dice_policy, "testvm")?;
        let claims = create_sd_certificate_claims("testvm")?;

        let err = check_dice_policy(&claims, &vvm_config).expect_err("check() should have failed");

        assert!(err.to_string().contains("DICE policy mismatch"));

        Ok(())
    }
}
