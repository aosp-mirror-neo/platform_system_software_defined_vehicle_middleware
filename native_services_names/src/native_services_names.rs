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

use anyhow::bail;
use google_sdv_identity::aidl::google::sdv::identity::LocalServiceFqin::LocalServiceFqin;
use sdv_comms_id::ServiceFqin;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::str::FromStr;

const SYSTEM_EXT_NATIVE_SERVICES_CONFIG: &str = "/system_ext/etc/sdv_native_services_names";
const PRODUCT_NATIVE_SERVICES_CONFIG: &str = "/product/etc/sdv_native_services_names";
const VENDOR_NATIVE_SERVICES_CONFIG: &str = "/vendor/etc/sdv_native_services_names";

fn parse_local_fqin_from_string(s: &str) -> anyhow::Result<LocalServiceFqin> {
    let fqin = ServiceFqin::from_str(&format!("local-vm:{s}"))?;
    Ok(LocalServiceFqin {
        sdvPackageName: fqin.package_name().to_string(),
        serviceBundleName: fqin.service_name().to_string(),
        serviceInstanceName: fqin.instance_name().to_string(),
    })
}

/// Provides a mapping from UIDs to allowed service bundle QINs.
///
/// This is used to verify that a native service is not attempting to register with a service
/// bundle QIN that it is not authorized to use.
#[derive(Debug, PartialEq)]
pub struct NativeServicesNames {
    uid_to_local_fqins: HashMap<u32, HashSet<LocalServiceFqin>>,
}

impl NativeServicesNames {
    /// Creates a new `NativeServicesNames` instance from configuration files in standard locations.
    pub fn new() -> anyhow::Result<Self> {
        let system_ext_config = Self::read_config_to_string(SYSTEM_EXT_NATIVE_SERVICES_CONFIG)?;
        let product_config = Self::read_config_to_string(PRODUCT_NATIVE_SERVICES_CONFIG)?;
        let vendor_config = Self::read_config_to_string(VENDOR_NATIVE_SERVICES_CONFIG)?;
        Self::new_from_configs(&system_ext_config, &product_config, &vendor_config)
    }

    /// Creates a new `NativeServicesNames` containing the given native services.
    ///
    /// To be used in tests.
    #[cfg(feature = "test-util")]
    pub fn from_entries(entries: &[(u32, &str)]) -> Self {
        let mut uid_to_local_fqins: HashMap<u32, HashSet<LocalServiceFqin>> = HashMap::new();
        for (uid, fqin_str) in entries {
            let fqin = parse_local_fqin_from_string(fqin_str)
                .unwrap_or_else(|e| panic!("Invalid FQIN '{fqin_str}': {e}"));
            uid_to_local_fqins.entry(*uid).or_default().insert(fqin);
        }
        Self { uid_to_local_fqins }
    }

    fn new_from_configs(
        system_ext_config: &str,
        product_config: &str,
        vendor_config: &str,
    ) -> anyhow::Result<Self> {
        let mut uid_to_local_fqins = HashMap::new();
        Self::parse_and_extend_map(system_ext_config, &mut uid_to_local_fqins)?;
        Self::parse_and_extend_map(product_config, &mut uid_to_local_fqins)?;
        Self::parse_and_extend_map(vendor_config, &mut uid_to_local_fqins)?;

        Ok(Self { uid_to_local_fqins })
    }

    fn read_config_to_string(path: &str) -> anyhow::Result<String> {
        match fs::read_to_string(path) {
            Ok(content) => Ok(content),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e.into()),
        }
    }

    // Parse a config which contains a <UID>,<LOCAL_FQIN> pair in each line. E.g.:
    // 7500,com.sdv.google.vpm.VpmSystemServiceBundle/default
    // 7501,com.sdv.google.orchestrator.OrchestratorServiceBundle/default
    // ..
    fn parse_and_extend_map(
        content: &str,
        uid_to_local_fqins: &mut HashMap<u32, HashSet<LocalServiceFqin>>,
    ) -> anyhow::Result<()> {
        for line in content.lines() {
            let parts: Vec<&str> = line.splitn(2, ',').collect();
            if parts.len() != 2 {
                bail!("Invalid line in config: {line}");
            }
            let uid = u32::from_str(parts[0])?;
            if !Self::is_allowed_native_service_uid(uid) {
                bail!("UID {uid} from native services config is not in the accepted range");
            }
            let fqin = parse_local_fqin_from_string(parts[1])?;

            if !uid_to_local_fqins.entry(uid).or_default().insert(fqin) {
                bail!(
                    "Duplicate FQIN entry {} for UID {uid} in the native services config",
                    parts[1]
                );
            }
        }
        Ok(())
    }

    /// Checks if a given `LocalServiceFqin` is allowed for a specific service UID.
    ///
    /// Returns `true` if the UID is configured and the service bundle QIN is in the allowed set
    /// for that UID, otherwise it returns `false`.
    pub fn is_local_fqin_allowed_for_service_uid(&self, uid: u32, fqin: &LocalServiceFqin) -> bool {
        if let Some(fqins) = self.uid_to_local_fqins.get(&uid) {
            return fqins.contains(fqin);
        }
        false
    }

    fn is_allowed_native_service_uid(uid: u32) -> bool {
        // using root is not allowed in the config, because it is handled separately.
        if uid == sdv_aid::AID_ROOT {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_SYSTEM_EXT_UID: u32 = sdv_aid::AID_SYSTEM_EXT_RESERVED_START;
    const A_LOCAL_FQIN: &str = "com.a.B/c";

    #[test]
    fn new_from_configs_succeeds() -> anyhow::Result<()> {
        let config = format!("{A_SYSTEM_EXT_UID},{A_LOCAL_FQIN}");
        NativeServicesNames::new_from_configs(&config, "", "")?;
        Ok(())
    }

    #[test]
    fn new_from_configs_succeeds_with_no_configs() -> anyhow::Result<()> {
        let native_services_names = NativeServicesNames::new_from_configs("", "", "")?;
        assert!(native_services_names.uid_to_local_fqins.is_empty());
        Ok(())
    }

    #[test]
    fn new_from_configs_fails_for_invalid_line() {
        let config = format!("{A_SYSTEM_EXT_UID},{A_LOCAL_FQIN}\ninvalid_line");
        let result = NativeServicesNames::new_from_configs(&config, "", "");
        assert!(result.is_err());
    }

    #[test]
    fn new_from_configs_fails_for_invalid_uid() {
        let config = format!("ASD,{A_LOCAL_FQIN}");
        let result = NativeServicesNames::new_from_configs(&config, "", "");
        assert!(result.is_err());
    }

    #[test]
    fn new_from_configs_fails_for_negative_uid() {
        let config = format!("-1,{A_LOCAL_FQIN}");
        let result = NativeServicesNames::new_from_configs(&config, "", "");
        assert!(result.is_err());
    }

    #[test]
    fn new_from_configs_fails_for_uid_not_in_accepted_range() {
        let aid_root = sdv_aid::AID_ROOT;
        let config = format!("{aid_root},{A_LOCAL_FQIN}");
        let result = NativeServicesNames::new_from_configs(&config, "", "");
        assert!(result.is_err());
    }

    #[test]
    fn new_from_configs_fails_for_invalid_local_fqin() {
        let config = format!("{A_SYSTEM_EXT_UID},com.a.B");
        let result = NativeServicesNames::new_from_configs(&config, "", "");
        assert!(result.is_err());
    }

    #[test]
    fn new_from_configs_fails_for_multiple_config_in_same_line() {
        let config = format!("{A_SYSTEM_EXT_UID},{A_LOCAL_FQIN},{A_SYSTEM_EXT_UID},{A_LOCAL_FQIN}");
        let result = NativeServicesNames::new_from_configs(&config, "", "");
        assert!(result.is_err());
    }

    #[test]
    fn new_from_configs_fails_for_duplicate_fqin_for_same_uid() {
        let uid = sdv_aid::AID_SYSTEM_EXT_RESERVED_START;
        let fqin = "com.a.B/c";
        let config = format!("{uid},{fqin}\n{uid},{fqin}");
        let result = NativeServicesNames::new_from_configs(&config, "", "");
        assert!(result.is_err());
    }

    #[test]
    fn is_allowed_works_from_all_configs() -> anyhow::Result<()> {
        let uid1 = sdv_aid::AID_SYSTEM_EXT_RESERVED_START;
        let uid2 = sdv_aid::AID_PRODUCT_RESERVED_START;
        let uid3 = sdv_aid::AID_OEM_RESERVED_START;
        let fqin1 = "com.a.B/c";
        let fqin2 = "com.d.E/f";
        let fqin3 = "com.g.H/i";

        let system_ext_config = format!("{uid1},{fqin1}");
        let product_config = format!("{uid2},{fqin2}");
        let vendor_config = format!("{uid3},{fqin3}");

        let native_services_names = NativeServicesNames::new_from_configs(
            &system_ext_config,
            &product_config,
            &vendor_config,
        )?;

        assert!(native_services_names
            .is_local_fqin_allowed_for_service_uid(uid1, &parse_local_fqin_from_string(fqin1)?));
        assert!(native_services_names
            .is_local_fqin_allowed_for_service_uid(uid2, &parse_local_fqin_from_string(fqin2)?));
        assert!(native_services_names
            .is_local_fqin_allowed_for_service_uid(uid3, &parse_local_fqin_from_string(fqin3)?));
        Ok(())
    }

    #[test]
    fn is_allowed_succeeds_for_uids_with_multiple_fqins() -> anyhow::Result<()> {
        let uid = sdv_aid::AID_SYSTEM_EXT_RESERVED_START;
        let fqin1 = "com.a.B/c";
        let fqin2 = "com.d.E/f";
        let config = format!("{uid},{fqin1}\n{uid},{fqin2}");

        let native_services_names = NativeServicesNames::new_from_configs(&config, "", "")?;

        assert!(native_services_names
            .is_local_fqin_allowed_for_service_uid(uid, &parse_local_fqin_from_string(fqin1)?));
        assert!(native_services_names
            .is_local_fqin_allowed_for_service_uid(uid, &parse_local_fqin_from_string(fqin2)?));
        Ok(())
    }

    #[test]
    fn is_allowed_fails_if_fqin_is_not_allowed_for_uid() -> anyhow::Result<()> {
        let uid = sdv_aid::AID_SYSTEM_EXT_RESERVED_START;
        let fqin1 = "com.a.B/c";
        let fqin2 = "com.d.E/f";
        let config = format!("{uid},{fqin1}");

        let native_services_names = NativeServicesNames::new_from_configs(&config, "", "")?;

        assert!(!native_services_names
            .is_local_fqin_allowed_for_service_uid(uid, &parse_local_fqin_from_string(fqin2)?));
        Ok(())
    }

    #[test]
    fn is_allowed_fails_if_all_configs_are_empty() -> anyhow::Result<()> {
        let uid = sdv_aid::AID_SYSTEM_EXT_RESERVED_START;
        let fqin = "com.a.B/c";

        let native_services_names = NativeServicesNames::new_from_configs("", "", "")?;

        assert!(!native_services_names
            .is_local_fqin_allowed_for_service_uid(uid, &parse_local_fqin_from_string(fqin)?));
        Ok(())
    }

    #[test]
    fn is_allowed_fails_for_unconfigured_uid() -> anyhow::Result<()> {
        let configured_uid = sdv_aid::AID_SYSTEM_EXT_RESERVED_START;
        let unconfigured_uid = sdv_aid::AID_SYSTEM_EXT_RESERVED_START + 1;
        let fqin = "com.a.B/c";
        let config = format!("{configured_uid},{fqin}");

        let native_services_names = NativeServicesNames::new_from_configs(&config, "", "")?;

        assert!(!native_services_names.is_local_fqin_allowed_for_service_uid(
            unconfigured_uid,
            &parse_local_fqin_from_string(fqin)?
        ));
        Ok(())
    }

    #[test]
    fn new_succeeds_regardless_of_device_config() -> anyhow::Result<()> {
        // Independently of the device configuration, whether config files are configured or not,
        // new() must succeed.
        NativeServicesNames::new()?;
        Ok(())
    }
}
