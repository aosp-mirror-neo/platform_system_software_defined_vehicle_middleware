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

use crate::system::properties;
use anyhow::{anyhow, bail, Context, Ok};
use coset::CborSerializable;
use dice_policy::DicePolicy;
use hwtrust::publickey::PublicKey;
use log::info;
use sdv_comms_id::names::VmName;
use std::fs::read_dir;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use vvmtruststore::CertificateRevocationList;

const VVM_CONFIG_FILE_PREFIX: &str = "vvmconfig";

#[cfg(sdv_sd_agent_for_a15)]
const VVM_CONFIG_DIR: &str = "/vendor/etc";

#[cfg(not(sdv_sd_agent_for_a15))]
const VVM_CONFIG_DIR: &str = "/product/etc";

const SYSTEM_PROP_INSTANCE_NAME: &str = "ro.boot.sdv.instance_name";
const SYSTEM_PROP_VVMCONFIG: &str = "ro.boot.sdv.vvmconfig";

#[derive(Debug, PartialEq, Clone)]
pub(crate) struct VmConfig {
    /// VM instance name
    pub name: VmName,

    /// Socket address where the Service Discovery agent of this VM can be found.
    pub addr: SocketAddr,

    /// Index of the DICE policy for the Android SDV VM DICE chain.
    pub android: u32,

    /// Index of the DICE policy for the Secure World DICE chain used by the Android SDV VM.
    pub secure_world: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct VvmConfig {
    /// Public key of the UDS root provisioning authority
    pub uds_ca_pub: PublicKey,

    /// Revocation list of intermediate CAs of UDS certificates
    pub uds_ca_rev_list: CertificateRevocationList,

    /// Array of DICE policies used to verify DICE chains of SDV VMs.
    ///
    /// The `android` and `secureWorld` fields of `VmConfig` are indexes
    /// to this array.
    pub policies: Vec<DicePolicy>,

    /// The configurations of all remote (ie, peer) VMs
    pub remote_cfgs: Vec<VmConfig>,

    /// The VM's own configuration.
    pub own_cfg: VmConfig,
}

#[derive(Debug, PartialEq)]
pub(crate) struct VmDicePolicies<'a> {
    /// The DICE policy used to verify Android SDV DICE chains of SDV VMs.
    pub android_sdv_dice_policy: &'a DicePolicy,

    /// The DICE policy used to verify secure world DICE chains of SDV VMs.
    pub secure_world_dice_policy: &'a DicePolicy,
}

/// Searches for a file named vvmconfig[.*] in /etc and returns its path. Fails if there
/// are file system errors or if no matching file is found.
///
/// If there are multiple files matching that pattern, it will return the first one it
/// finds. Therefore in this case it's undetermined which of the matching files it will
/// return. Note that systems are not expected to have multiple vvmconfig files.
///
/// Some examples of suffix usage:
/// * vvmconfig.example: an example vvmconfig file for development purposes
/// * vvmconfig.$VEHICLE_CODE_NAME: the vvmconfig file for a particular vehicle model,
///   shared among all VMs of that vehicle model or platform.
/// * vvmconfig.$PRODUCT_NAME: the vvmconfig of a particular SDV android product.
fn find_vvmconfig_file(dir: &Path) -> anyhow::Result<PathBuf> {
    for entry in read_dir(dir)? {
        let path = entry?.path();
        if path.file_stem().is_some_and(|p| p == VVM_CONFIG_FILE_PREFIX) {
            return Ok(path);
        }
    }

    Err(anyhow!("Could not find a vvmconfig file in {:#?}.", dir))
}

fn load_vvmconfig(path: &Path) -> anyhow::Result<vvmconfig::VvmConfig> {
    info!("Loading VvmConfig from {:#?}", &path);
    let cbor: Vec<u8> =
        std::fs::read(path).with_context(|| format!("Failed to read file: {path:#?}"))?;

    vvmconfig::VvmConfig::from_slice(&cbor).context("Failed to deserialize VvmConfig from CBOR")
}

impl TryFrom<vvmconfig::VvmConfig> for VvmConfig {
    type Error = anyhow::Error;

    fn try_from(value: vvmconfig::VvmConfig) -> Result<Self, Self::Error> {
        let own_name = properties::read(SYSTEM_PROP_INSTANCE_NAME)?
            .ok_or(anyhow!("Android system property not set: {}", SYSTEM_PROP_INSTANCE_NAME))?;

        // Split vm_configs into remote_cfgs and own_cfg.
        let mut maybe_own_cfg: Option<VmConfig> = None;
        let mut remote_cfgs = Vec::new();
        let mut vm_configs = value.vm_configs;
        while let Some((name, cfg)) = vm_configs.pop_last() {
            if name == own_name {
                maybe_own_cfg = Some(VmConfig::from_cfg(name, cfg)?);
            } else {
                remote_cfgs.push(VmConfig::from_cfg(name, cfg)?);
            }
        }

        let own_cfg = maybe_own_cfg
            .ok_or(anyhow!("VvmConfig does not have a VM with instance name \"{}\"", own_name))?;

        Ok(VvmConfig {
            uds_ca_pub: value.uds_ca_pub,
            uds_ca_rev_list: (&value.uds_ca_rev_list).try_into()?, //crl,
            policies: value.policies,
            remote_cfgs,
            own_cfg,
        })
    }
}

impl VmConfig {
    fn from_cfg(name: String, cfg: vvmconfig::VmConfig) -> anyhow::Result<Self> {
        let socket_addrs: Vec<SocketAddr> = cfg.ips.into_iter().map(SocketAddr::from).collect();

        // This is already ensured by the CBOR format, but checking here again anyway
        // as the vvmconfig::VmConfig struct itself allows for an empty vector.
        if socket_addrs.is_empty() {
            return Err(anyhow!("VmConfig for {name} must have at least one IpAndPort"));
        }

        Ok(VmConfig {
            name: name.clone().try_into()?,
            // The CBOR format allows for several Ip addresses where SD agent
            // can be found (eg, one could have an ipv4 and an ipv6).
            // But our impl is currently using only one anyway.
            addr: socket_addrs[0],
            android: cfg.android,
            secure_world: cfg.secure_world,
        })
    }

    #[cfg(test)]
    pub(crate) fn new_for_testing(name: VmName, socket_addr: SocketAddr) -> Self {
        VmConfig { name, addr: socket_addr, android: 0, secure_world: 0 }
    }
}

impl VvmConfig {
    pub fn load_from_file() -> anyhow::Result<Self> {
        Self::load_from_file_internal(
            Path::new(VVM_CONFIG_DIR),
            properties::read(SYSTEM_PROP_VVMCONFIG)?,
        )
    }

    fn load_from_file_internal(
        dir: &Path,
        maybe_file_name: Option<String>,
    ) -> anyhow::Result<Self> {
        let vvmconfig_path = match maybe_file_name {
            Some(file_name) => create_file_path(dir, &file_name)?,
            None => find_vvmconfig_file(dir).context("Failed to find a vvmconfig file.")?,
        };

        VvmConfig::try_from(load_vvmconfig(&vvmconfig_path)?)
    }

    pub fn find_remote_config(&self, vm_name: &VmName) -> Option<&VmConfig> {
        self.remote_cfgs.iter().find(|vm_cfg| &vm_cfg.name == vm_name)
    }

    /// Returns the Android SDV DICE policy and secure world DICE policy of a specific remote VM.
    pub fn get_dice_policies(&self, vm_name: &VmName) -> anyhow::Result<VmDicePolicies> {
        let vm_config = self
            .find_remote_config(vm_name)
            .ok_or(anyhow!("Unknown VM name: {:?}", vm_name.name()))?;
        let android_sdv_dice_policy = self
            .policies
            .get::<usize>(vm_config.android.try_into()?)
            .ok_or(anyhow!("No dice policy found for Android SDV dice chain"))?;
        let secure_world_dice_policy = self
            .policies
            .get::<usize>(vm_config.secure_world.try_into()?)
            .ok_or(anyhow!("No dice policy found for secure world dice chain"))?;
        Ok(VmDicePolicies { android_sdv_dice_policy, secure_world_dice_policy })
    }
}

/// Ensures the given string represents a file name and not a relative or absolute path.
fn create_file_path(dir: &Path, file_name: &str) -> anyhow::Result<PathBuf> {
    if file_name.is_empty()
        || file_name.contains(std::path::MAIN_SEPARATOR_STR)
        || file_name.contains("..")
    {
        bail!("Invalid file name: {file_name}");
    }

    let file_path = dir.join(file_name);

    if file_path.parent() != Some(dir) {
        // Somehow file_name managed to change the directory still.
        bail!("Invalid file name: {file_name}");
    }

    Ok(file_path)
}

#[cfg(test)]
pub fn create_test_uds_ca_pub() -> anyhow::Result<PublicKey> {
    use coset::{iana::EllipticCurve, CoseKeyBuilder};

    PublicKey::from_cose_key(
        &CoseKeyBuilder::new_ec2_pub_key(
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
        .algorithm(coset::iana::Algorithm::ES256)
        .build(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs::File;
    use std::io::Write;
    use std::net::{IpAddr, Ipv4Addr};
    use tempfile::tempdir;

    use dice_policy::{
        Constraint, DicePolicy, NodeConstraints, EXACT_MATCH_CONSTRAINT,
        GREATER_OR_EQUAL_CONSTRAINT,
    };

    fn create_android_dice_policy() -> anyhow::Result<DicePolicy> {
        Ok(DicePolicy {
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
        })
    }

    fn create_secure_world_dice_policy() -> anyhow::Result<DicePolicy> {
        Ok(DicePolicy {
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
        })
    }

    fn create_vm_configs() -> BTreeMap<String, vvmconfig::VmConfig> {
        use vvmconfig::IpAndPort;
        let mut expected_vm_configs = BTreeMap::new();
        expected_vm_configs.insert(
            "foo".to_owned(),
            vvmconfig::VmConfig {
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
            vvmconfig::VmConfig {
                ips: vec![IpAndPort { ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), port: None }],
                android: 0,
                secure_world: 1,
            },
        );

        expected_vm_configs
    }

    fn create_vvm_config() -> anyhow::Result<vvmconfig::VvmConfig> {
        Ok(vvmconfig::VvmConfig {
            uds_ca_pub: create_test_uds_ca_pub()?,
            uds_ca_rev_list: vec![],
            policies: vec![create_android_dice_policy()?, create_secure_world_dice_policy()?],
            vm_configs: create_vm_configs(),
        })
    }

    fn create_mesh_vvm_config(local_vm: &str, remote_vm: &str) -> anyhow::Result<VvmConfig> {
        Ok(VvmConfig {
            uds_ca_pub: create_test_uds_ca_pub()?,
            uds_ca_rev_list: CertificateRevocationList::default(),
            policies: vec![create_android_dice_policy()?, create_secure_world_dice_policy()?],
            remote_cfgs: vec![VmConfig {
                name: remote_vm.to_owned().try_into()?,
                // Arbitrary address is used which doesn't affect test result
                addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), 2002),
                android: 0,
                secure_world: 1,
            }],
            own_cfg: VmConfig {
                name: local_vm.to_owned().try_into()?,
                // Arbitrary address is used which doesn't affect test result
                addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), 2002),
                android: 0,
                secure_world: 1,
            },
        })
    }

    #[test]
    fn find_vvmconfig_file_many_files() -> anyhow::Result<()> {
        let tmp_dir = tempdir()?;

        let vvmconfig_path = tmp_dir.path().join("vvmconfig");
        let vvmconfig_foo_path = tmp_dir.path().join("vvmconfig.foo");

        File::create(&vvmconfig_path)?;
        File::create(&vvmconfig_foo_path)?;
        File::create(tmp_dir.path().join("unrelated.conf"))?;

        let path = find_vvmconfig_file(tmp_dir.path())?;

        assert!(path == vvmconfig_path || path == vvmconfig_foo_path);

        Ok(())
    }

    #[test]
    fn find_vvmconfig_file_single_file_with_suffix() -> anyhow::Result<()> {
        let tmp_dir = tempdir()?;
        let vvmconfig_foo_path = tmp_dir.path().join("vvmconfig.foo");

        File::create(&vvmconfig_foo_path)?;
        File::create(tmp_dir.path().join("unrelated.conf"))?;

        let path = find_vvmconfig_file(tmp_dir.path())?;

        assert_eq!(path, vvmconfig_foo_path);

        Ok(())
    }

    #[test]
    fn find_vvmconfig_file_no_file() -> anyhow::Result<()> {
        let tmp_dir = tempdir()?;

        File::create(tmp_dir.path().join("unrelated.conf"))?;

        let result = find_vvmconfig_file(tmp_dir.path());

        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn load_from_specific_file_name_ok() -> anyhow::Result<()> {
        let file_name = String::from("foobar_config");
        let tmp_dir = tempdir()?;
        let file_path = tmp_dir.path().join(file_name.as_str());

        let vvm_config = create_vvm_config()?;
        File::create(&file_path)?.write_all(&vvm_config.to_vec()?)?;

        properties::testing::set_property_ok_value(SYSTEM_PROP_INSTANCE_NAME, "foo");

        // Tell VvmConfig to load a file with a specific name in the tmp_dir.
        let _ = VvmConfig::load_from_file_internal(tmp_dir.path(), Some(file_name))?;

        Ok(())
    }

    #[test]
    fn load_from_nonexistent_file_name_err() -> anyhow::Result<()> {
        let file_name = String::from("vvmconfig.foo");
        let tmp_dir = tempdir()?;
        let file_path = tmp_dir.path().join(file_name.as_str());

        let vvm_config = create_vvm_config()?;
        File::create(&file_path)?.write_all(&vvm_config.to_vec()?)?;

        properties::testing::set_property_ok_value(SYSTEM_PROP_INSTANCE_NAME, "foo");

        let nonexistent_file_name = String::from("vvmconfig.bar");

        // Tell VvmConfig to load file that does not exist in the tmp_dir.
        let load_error =
            VvmConfig::load_from_file_internal(tmp_dir.path(), Some(nonexistent_file_name.clone()))
                .expect_err("VvmConfig loading from a non-existing file must fail");

        assert!(load_error.to_string().contains("Failed to read file"));
        assert!(load_error.to_string().contains(&nonexistent_file_name));

        Ok(())
    }

    #[test]
    fn load_from_invalid_file_name_err() -> anyhow::Result<()> {
        let file_name = String::from("vvmconfig.foo");
        let tmp_dir = tempdir()?;
        let file_path = tmp_dir.path().join(file_name.as_str());

        let vvm_config = create_vvm_config()?;
        File::create(&file_path)?.write_all(&vvm_config.to_vec()?)?;

        properties::testing::set_property_ok_value(SYSTEM_PROP_INSTANCE_NAME, "foo");

        let invalid_file_name = String::from("/somewhere/vvmconfig.bar");

        // Tell VvmConfig to load file that does not exist in the tmp_dir.
        let load_error =
            VvmConfig::load_from_file_internal(tmp_dir.path(), Some(invalid_file_name.clone()))
                .expect_err("VvmConfig loading from a non-existing file must fail");

        assert!(load_error.to_string().contains("Invalid file name"));
        assert!(load_error.to_string().contains(&invalid_file_name));

        Ok(())
    }

    #[test]
    fn test_create_file_path() -> anyhow::Result<()> {
        let dir = Path::new("/product/etc");
        create_file_path(dir, "vvmconfig")?;
        create_file_path(dir, "vvmconfig.foo")?;
        create_file_path(dir, "some_file")?;
        create_file_path(dir, "../foo").expect_err("Must fail with a relative path");
        create_file_path(dir, "/somewhere/foo").expect_err("Must fail with an absolute path");
        create_file_path(dir, "somewhere/foo").expect_err("Must fail with a partial path");
        create_file_path(dir, "").expect_err("Must fail with an empty string");
        Ok(())
    }

    #[test]
    fn load_from_find_file_name_ok() -> anyhow::Result<()> {
        let file_name = String::from("vvmconfig.foo");
        let tmp_dir = tempdir()?;
        let file_path = tmp_dir.path().join(file_name.as_str());

        let vvm_config = create_vvm_config()?;
        File::create(&file_path)?.write_all(&vvm_config.to_vec()?)?;

        properties::testing::set_property_ok_value(SYSTEM_PROP_INSTANCE_NAME, "foo");

        // Tell VvmConfig load any vvmconfig file finds in the temp dir.
        let _ = VvmConfig::load_from_file_internal(tmp_dir.path(), None)?;

        Ok(())
    }

    #[test]
    fn load_from_cbor_file() -> anyhow::Result<()> {
        let vvm_config = create_vvm_config()?;
        let tmp_dir = tempdir()?;

        let expected_uds_ca_pub = vvm_config.uds_ca_pub.clone();

        let vvmconfig_path = tmp_dir.path().join("vvmconfig");

        File::create(&vvmconfig_path)?.write_all(&vvm_config.to_vec()?)?;

        properties::testing::set_property_ok_value(SYSTEM_PROP_INSTANCE_NAME, "foo");

        let mesh_vvmconfig = VvmConfig::try_from(load_vvmconfig(&vvmconfig_path)?)?;

        // Probe some of the fields to verify that loadin and conversion to the local VvmConfig
        // struct was indeed successful.
        assert_eq!(expected_uds_ca_pub, mesh_vvmconfig.uds_ca_pub);
        assert_eq!("foo", mesh_vvmconfig.own_cfg.name.as_ref());
        assert_eq!(1, mesh_vvmconfig.remote_cfgs.len());
        assert_eq!("bar", mesh_vvmconfig.remote_cfgs[0].name.as_ref());

        Ok(())
    }

    #[test]
    fn test_get_dice_policies_valid_vm_succeed() -> anyhow::Result<()> {
        let local_vm_name = "foo";
        let remote_vm_name = "bar";
        let mesh_vvmconfig = create_mesh_vvm_config(local_vm_name, remote_vm_name)?;
        let expected_policies = VmDicePolicies {
            android_sdv_dice_policy: &create_android_dice_policy()?,
            secure_world_dice_policy: &create_secure_world_dice_policy()?,
        };
        let remote_vm: VmName = remote_vm_name.to_owned().try_into()?;

        let result_policy = mesh_vvmconfig.get_dice_policies(&remote_vm)?;

        assert_eq!(result_policy, expected_policies);

        Ok(())
    }

    #[test]
    fn test_get_dice_policies_unknown_vm_fail() -> anyhow::Result<()> {
        let local_vm_name = "foo";
        let remote_vm_name = "bar";
        let mesh_vvmconfig = create_mesh_vvm_config(local_vm_name, remote_vm_name)?;
        let unknown_vm_name: VmName = "unknown".to_owned().try_into()?;

        let result_policy_err = mesh_vvmconfig
            .get_dice_policies(&unknown_vm_name)
            .expect_err("get_dice_policies() should have failed");

        assert_eq!(result_policy_err.to_string(), "Unknown VM name: \"unknown\"");

        Ok(())
    }

    #[test]
    fn test_get_dice_policies_no_android_sdv_policy_found_fail() -> anyhow::Result<()> {
        let local_vm_name = "foo";
        let remote_vm_name = "bar";
        let good_vvmconfig = create_mesh_vvm_config(local_vm_name, remote_vm_name)?;
        let mesh_vvmconfig = VvmConfig { policies: vec![], ..good_vvmconfig };
        let remote_vm: VmName = remote_vm_name.to_owned().try_into()?;

        let result_policy_err = mesh_vvmconfig
            .get_dice_policies(&remote_vm)
            .expect_err("get_dice_policies() should have failed");

        assert_eq!(
            result_policy_err.to_string(),
            "No dice policy found for Android SDV dice chain"
        );

        Ok(())
    }

    #[test]
    fn test_get_dice_policies_no_secure_world_policy_found_fail() -> anyhow::Result<()> {
        let local_vm_name = "foo";
        let remote_vm_name = "bar";
        let good_vvmconfig = create_mesh_vvm_config(local_vm_name, remote_vm_name)?;
        let mesh_vvmconfig =
            VvmConfig { policies: vec![create_android_dice_policy()?], ..good_vvmconfig };
        let remote_vm: VmName = remote_vm_name.to_owned().try_into()?;

        let result_policy_err = mesh_vvmconfig
            .get_dice_policies(&remote_vm)
            .expect_err("get_dice_policies() should have failed");

        assert_eq!(
            result_policy_err.to_string(),
            "No dice policy found for secure world dice chain"
        );

        Ok(())
    }
}
