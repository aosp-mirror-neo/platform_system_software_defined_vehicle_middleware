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

use crate::mesh::VvmConfig;
use crate::sha256::sha256;
use anyhow::{bail, Context};
use sdv_comms_id::ServiceFqin;
use std::time::{Duration, Instant};

/// Size of a u64 value, in bytes.
const U64_BYTE_SIZE: usize = 8;

/// Size of the hash part of the SID, in bytes.
const HASH_BYTE_SIZE: usize = U64_BYTE_SIZE - 1;

/// Maximum number of retries for generating a unique SID.
const MAX_SID_GENERATION_RETRIES: u8 = 3;

/// Generates SIDs (Service bundle instance identification number)
///
/// SID is a 64-bit number that identifies a service bundle instance. It is guaranteed to be unique
/// vehicle-wide across all existing service instances.
///
/// An SID is unique not just per FQIN, but per execution of a service with a given FQIN.
/// This includes executions across VM reboots. There's no guarantee for that uniqueness though.
#[derive(Clone)]
pub(crate) struct SidGenerator {
    /// The instant when this SidGenerator was created.
    /// Used to calculate UPTIME.
    start_time: Instant,

    /// The BOOT_ID
    boot_id: Vec<u8>,

    /// The index of the local VM
    local_vm_index: u8,
}

impl SidGenerator {
    pub(crate) fn try_new(vvm_config: &VvmConfig) -> anyhow::Result<Self> {
        let boot_id = generate_boot_id().context("Failed to generate BOOT_ID")?;
        let start_time = Instant::now();
        let local_vm_index = calculate_local_vm_index(vvm_config)?;
        Ok(Self { start_time, boot_id, local_vm_index })
    }

    /// Generates the SID of a service instance with the given FQIN that has been registered at
    /// this moment.
    ///
    /// It internally retries, generating a different SID, if the provided `is_sid_taken` function
    /// returns true for that generated SID.
    pub(crate) fn generate_sid(
        &self,
        fqin: &ServiceFqin,
        is_sid_taken: &impl Fn(u64) -> bool,
    ) -> anyhow::Result<u64> {
        for attempt_index in 0..MAX_SID_GENERATION_RETRIES {
            let sid = compute_sid(
                self.start_time.elapsed(),
                self.boot_id.clone(),
                self.local_vm_index,
                fqin,
                attempt_index,
            )
            .context(format!(
                "Failed to compute an SID for {fqin:?} with attempt_index {attempt_index}"
            ))?;

            if !is_sid_taken(sid) {
                return Ok(sid);
            }
        }
        bail!(
            "Failed to generate a unique SID for {fqin:?} after {} retries",
            MAX_SID_GENERATION_RETRIES
        )
    }
}

/// The SID of a given FQIN generated at the moment represented by UPTIME, with a given
/// ATTEMPT_INDEX is:
/// VM_INDEX || FIRST_7_BYTES_OF(SHA256(BOOT_ID || UPTIME || FQIN || ATTEMPT_INDEX))
/// Where:
/// * "||" stands for concatenation.
/// * VM_INDEX is the index of the local VM name in a sorted list of all VM names in the vehicle,
///   stored as a byte (u8).
/// * BOOT_ID is a random number identifying the current sdv_sd_agent execution.
/// * UPTIME is the system uptime (how long the system has been running).
/// * ATTEMPT_INDEX is a small integer (u8) that ensures hash input is different on each attempt.
///   If a generated SID collides with a preexisting one (a very unlikely, but possible situation),
///   generation will be retried.
///
/// VM_INDEX splits the SID namespace between the VMs, ensuring that there will be no hash
/// collisions across VMs.
/// Note: We could even make VM_INDEX take just as many bits as needed to represent all indexes of
/// the given vvmconfig, so that we can gain more bits for the FQIN hash.  Just 2 or 3 bits would
/// be enough for most vehicle configurations.  But that's an optimization we can do without.
fn compute_sid(
    uptime: Duration,
    boot_id: Vec<u8>,
    local_vm_index: u8,
    fqin: &ServiceFqin,
    attempt_index: u8,
) -> anyhow::Result<u64> {
    let mut uptime_bytes = uptime.as_millis().to_be_bytes().to_vec();
    let mut boot_id_bytes = boot_id;
    let fqin_str: String = fqin.clone().into();
    let mut fqin_bytes_vec = fqin_str.into_bytes();
    let mut attempt_index_bytes_vec = attempt_index.to_be_bytes().to_vec();

    let mut hash_input: Vec<u8> = Vec::new();
    hash_input.append(&mut boot_id_bytes);
    hash_input.append(&mut uptime_bytes);
    hash_input.append(&mut fqin_bytes_vec);
    hash_input.append(&mut attempt_index_bytes_vec);

    let mut hash = sha256(&hash_input)?;
    hash.truncate(HASH_BYTE_SIZE);

    let mut u64_bytes = vec![local_vm_index];
    u64_bytes.extend(hash);
    assert_eq!(u64_bytes.len(), U64_BYTE_SIZE);

    let u64_bytes: [u8; U64_BYTE_SIZE] = u64_bytes.try_into().unwrap();

    Ok(u64::from_be_bytes(u64_bytes))
}

/// Generates a BOOT_ID.
fn generate_boot_id() -> anyhow::Result<Vec<u8>> {
    use openssl::bn::{BigNum, MsbOption};

    let mut big = BigNum::new()?;

    // For our usage, it doesn't matter if the generated number is even or odd.
    // We use it as a nonce, so uniqueness is the only property that matters for us.
    let must_be_odd = false;

    big.rand(32, MsbOption::MAYBE_ZERO, must_be_odd)?;

    Ok(big.to_vec())
}

/// Calculates the VM_INDEX of the local VM.
fn calculate_local_vm_index(vvm_config: &VvmConfig) -> anyhow::Result<u8> {
    let mut all_vm_names: Vec<String> = Vec::new();

    let own_vm_name = String::from(vvm_config.own_cfg.name.clone());

    all_vm_names.push(own_vm_name.clone());

    for remote_cfg in &vvm_config.remote_cfgs {
        all_vm_names.push(String::from(remote_cfg.name.clone()));
    }

    all_vm_names.sort();

    if all_vm_names.len() > u8::MAX.into() {
        bail!("VvmConfig has too many VMs");
    }

    // unwrap: It must be found, since we added it above.
    // unwrap(2): We checked the length of the vector above, to ensure the index fits in a u8.
    Ok(all_vm_names.binary_search(&own_vm_name).unwrap().try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::vvm_config::{create_test_uds_ca_pub, VmConfig};
    use std::cell::Cell;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use vvmtruststore::CertificateRevocationList;

    fn create_service_fqin() -> ServiceFqin {
        ServiceFqin::builder()
            .vm_name("vm-front-left")
            .package_name("com.goog.sdv")
            .service_name("WindowManager")
            .instance_name("main")
            .build()
            .unwrap()
    }

    fn create_fake_vvmconfig() -> VvmConfig {
        VvmConfig {
            uds_ca_pub: create_test_uds_ca_pub().unwrap(),
            uds_ca_rev_list: CertificateRevocationList::default(),
            policies: vec![],
            remote_cfgs: vec![
                VmConfig::new_for_testing(
                    "vm-front-right".to_owned().try_into().unwrap(),
                    "192.168.98.4:2002".parse().unwrap(),
                ),
                VmConfig::new_for_testing(
                    "vm-rear-left".to_owned().try_into().unwrap(),
                    "192.168.98.5:2003".parse().unwrap(),
                ),
            ],
            own_cfg: VmConfig::new_for_testing(
                "vm-front-left".to_owned().try_into().unwrap(),
                "192.168.98.3:2001".parse().unwrap(),
            ),
        }
    }

    #[test]
    fn generate_sid_ok() -> anyhow::Result<()> {
        let fqin = create_service_fqin();

        let vvm_config = create_fake_vvmconfig();

        let sid_gen = SidGenerator::try_new(&vvm_config)?;
        let _ = sid_gen.generate_sid(&fqin, &|_| false)?;
        Ok(())
    }

    /// Computing an SID for an FQIN a different moments must yield different SIDs.
    #[test]
    fn compute_sid_same_fqin_diff_uptime() -> anyhow::Result<()> {
        let fqin = create_service_fqin();

        let boot_id = generate_boot_id().context("Failed to generate a BOOT_ID")?;
        let vm_index = 1u8;

        let sid_time1 =
            compute_sid(Duration::new(10, 1_000_000), boot_id.clone(), vm_index, &fqin, 0)?;

        // We have a millisecond resolution. So that should be enough
        // to get a different SID
        let sid_time2 =
            compute_sid(Duration::new(10, 2_000_000), boot_id.clone(), vm_index, &fqin, 0)?;

        assert_ne!(sid_time1, sid_time2);

        Ok(())
    }

    /// Computing an SID for an FQIN with different BOOT_IDs must yield different SIDs,
    /// even if the uptime is the same.
    #[test]
    fn compute_sid_same_fqin_diff_boot_id() -> anyhow::Result<()> {
        let fqin = create_service_fqin();
        let vm_index = 1u8;

        let boot_id1 = generate_boot_id().context("Failed to generate a BOOT_ID")?;
        let boot_id2 = generate_boot_id().context("Failed to generate a BOOT_ID")?;

        let uptime = Duration::new(10, 1_000_000);

        let sid1 = compute_sid(uptime, boot_id1, vm_index, &fqin, 0)?;
        let sid2 = compute_sid(uptime, boot_id2, vm_index, &fqin, 0)?;

        assert_ne!(sid1, sid2);

        Ok(())
    }

    #[test]
    fn compute_sid_is_deterministic() -> anyhow::Result<()> {
        let fqin = create_service_fqin();
        let vm_index = 1u8;

        let boot_id = generate_boot_id().context("Failed to generate a BOOT_ID")?;
        let uptime = Duration::new(10, 1000);

        let sid_time1 = compute_sid(uptime, boot_id.clone(), vm_index, &fqin, 0)?;
        let sid_time1_b = compute_sid(uptime, boot_id.clone(), vm_index, &fqin, 0)?;

        // The function must be deterministic. Given the same input, it must output the same value.
        assert_eq!(sid_time1, sid_time1_b);

        Ok(())
    }

    #[test]
    fn compute_sid_same_input_diff_attempt_index() -> anyhow::Result<()> {
        let fqin = create_service_fqin();
        let boot_id = generate_boot_id().context("Failed to generate a BOOT_ID")?;
        let vm_index = 1u8;
        let uptime = Duration::new(10, 1_000_000);

        let sid_attempt0 = compute_sid(uptime, boot_id.clone(), vm_index, &fqin, 0)?;
        let sid_attempt1 = compute_sid(uptime, boot_id.clone(), vm_index, &fqin, 1)?;

        assert_ne!(sid_attempt0, sid_attempt1, "SIDs should differ with different attempt indexes");

        // Test determinism with attempt_index
        let sid_attempt0_again = compute_sid(uptime, boot_id.clone(), vm_index, &fqin, 0)?;
        assert_eq!(
            sid_attempt0, sid_attempt0_again,
            "SID with same attempt_index should be deterministic"
        );
        Ok(())
    }

    #[test]
    fn calculate_local_vm_index_ok() -> anyhow::Result<()> {
        let vvm_config = create_fake_vvmconfig();
        // Sorted VM names: ["vm-front-left", "vm-front-right", "vm-rear-left"]
        // "vm-front-left" is the local VM, so its index should be 0.
        let index = calculate_local_vm_index(&vvm_config)?;
        assert_eq!(index, 0);

        // Change local VM to "vm-front-right"
        let mut vvm_config_2 = create_fake_vvmconfig();
        vvm_config_2.own_cfg = VmConfig::new_for_testing(
            "vm-front-right".to_owned().try_into().unwrap(),
            "192.168.98.4:2002".parse().unwrap(),
        );
        vvm_config_2.remote_cfgs = vec![
            VmConfig::new_for_testing(
                "vm-front-left".to_owned().try_into().unwrap(),
                "192.168.98.3:2001".parse().unwrap(),
            ),
            VmConfig::new_for_testing(
                "vm-rear-left".to_owned().try_into().unwrap(),
                "192.168.98.5:2003".parse().unwrap(),
            ),
        ];
        // Sorted VM names: ["vm-front-left", "vm-front-right", "vm-rear-left"]
        // "vm-front-right" is the local VM, so its index should be 1.
        let index_2 = calculate_local_vm_index(&vvm_config_2)?;
        assert_eq!(index_2, 1);

        Ok(())
    }

    #[test]
    fn calculate_local_vm_index_too_many_vms_err() -> anyhow::Result<()> {
        let mut vvm_config = create_fake_vvmconfig();
        for i in 0..256 {
            let vm_name = format!("remote-vm-{i}");
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, i as u8)), 2000 + i);
            vvm_config.remote_cfgs.push(VmConfig::new_for_testing(vm_name.try_into()?, addr));
        }
        assert!(calculate_local_vm_index(&vvm_config).is_err());
        Ok(())
    }

    #[test]
    fn generate_sid_retries_on_collision() -> anyhow::Result<()> {
        let fqin = create_service_fqin();
        let vvm_config = create_fake_vvmconfig();
        let sid_gen = SidGenerator::try_new(&vvm_config)?;

        let first_sid_attempted = Cell::new(None::<u64>);
        let call_count = Cell::new(0u32);

        let is_taken_closure = |sid: u64| {
            let current_call = call_count.get();
            call_count.set(current_call + 1);

            if current_call == 0 {
                first_sid_attempted.set(Some(sid));
                true // Mark first SID as taken
            } else {
                false // Mark subsequent SIDs as not taken
            }
        };

        let generated_sid_val = sid_gen.generate_sid(&fqin, &is_taken_closure)?;

        assert_eq!(call_count.get(), 2, "is_taken_closure should have been called twice.");
        assert!(
            first_sid_attempted.get().is_some(),
            "First SID attempt should have been recorded."
        );
        assert_ne!(
            Some(generated_sid_val),
            first_sid_attempted.get(),
            "Generated SID should be different from the one marked as taken."
        );
        Ok(())
    }

    #[test]
    fn generate_sid_fails_after_max_retries() -> anyhow::Result<()> {
        let fqin = create_service_fqin();
        let vvm_config = create_fake_vvmconfig();
        let sid_gen = SidGenerator::try_new(&vvm_config)?;

        let is_sid_taken = |_sid: u64| true; // Always collide

        let result = sid_gen.generate_sid(&fqin, &is_sid_taken);
        assert!(result.is_err(), "Expected an error after max retries");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains(&format!("after {MAX_SID_GENERATION_RETRIES} retries")));
        Ok(())
    }
}
