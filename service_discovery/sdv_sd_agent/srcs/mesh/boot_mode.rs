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
use anyhow::{anyhow, bail};
use log::error;

const SYSTEM_PROP_BOOT_MODE: &str = "ro.boot.sdv.boot_mode";
const SYSTEM_PROP_VERIFIED_BOOT_STATE: &str = "ro.boot.verifiedbootstate";
const SYSTEM_PROP_IGNORE_AVB_STATE: &str = "ro.boot.sdv.ignore_avb_state";
const SYSTEM_PROP_BUILD_TYPE: &str = "ro.build.type";
const LOCKED: &str = "locked";
const UNLOCKED: &str = "unlocked";
const GREEN: &str = "green";
const YELLOW: &str = "yellow";
const ORANGE: &str = "orange";
const USER: &str = "user";

/// The SDV Boot Mode
///
/// The SDV Boot Mode has two possible values: UNLOCKED and LOCKED. With regards to Service
/// Discovery mesh establishment, the LOCKED mode means that verification errors are fatal whereas
/// in the UNLOCKED mode they are not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SdvBootMode {
    Locked,
    Unlocked,
}

fn check_verified_boot_state(boot_mode: SdvBootMode) -> anyhow::Result<()> {
    match properties::read(SYSTEM_PROP_VERIFIED_BOOT_STATE)? {
        Some(string) => match string.as_str() {
            GREEN => (), // All good
            YELLOW => {
                return Err(anyhow!(
                    "SDV does not support {SYSTEM_PROP_VERIFIED_BOOT_STATE}={YELLOW}"
                ));
            }
            ORANGE if boot_mode == SdvBootMode::Locked => {
                return Err(anyhow!("SDV Boot Mode cannot be LOCKED if AVB is UNLOCKED"));
            }
            ORANGE => (),
            invalid => {
                return Err(anyhow!(
                    "Invalid value for {SYSTEM_PROP_VERIFIED_BOOT_STATE}: {invalid}"
                ));
            }
        },
        None => {
            return Err(anyhow!(
                "Android system property {SYSTEM_PROP_VERIFIED_BOOT_STATE} not set"
            ))
        }
    };
    Ok(())
}

fn should_ignore_avb_state() -> anyhow::Result<bool> {
    Ok(properties::read_bool(SYSTEM_PROP_IGNORE_AVB_STATE, false)? && !is_user_build()?)
}

/// Returns whether build type is `user`.
fn is_user_build() -> anyhow::Result<bool> {
    match properties::read(SYSTEM_PROP_BUILD_TYPE)? {
        Some(value) => match value.as_str() {
            USER => Ok(true),
            _ => Ok(false),
        },
        None => bail!("{SYSTEM_PROP_BUILD_TYPE} system property does not exist"),
    }
}

pub(crate) fn get_sdv_boot_mode() -> anyhow::Result<SdvBootMode> {
    let boot_mode = match properties::read(SYSTEM_PROP_BOOT_MODE)? {
        Some(string) => match string.as_str() {
            LOCKED => SdvBootMode::Locked,
            UNLOCKED => SdvBootMode::Unlocked,
            invalid => bail!("Invalid value for {SYSTEM_PROP_BOOT_MODE}: {invalid}"),
        },
        None => {
            error!("Android system property {SYSTEM_PROP_BOOT_MODE} not set. Assuming {LOCKED}");
            SdvBootMode::Locked
        }
    };

    if !should_ignore_avb_state()? {
        check_verified_boot_state(boot_mode)?;
    }

    Ok(boot_mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::properties::testing::{set_property, set_property_ok_value};

    struct Props<'a> {
        sdv: &'a str,
        avb: &'a str,
        ignore_avb: Option<String>,
        build_type: &'a str,
    }

    impl Props<'_> {
        pub fn set(self) {
            set_property_ok_value(SYSTEM_PROP_BOOT_MODE, self.sdv);
            set_property_ok_value(SYSTEM_PROP_VERIFIED_BOOT_STATE, self.avb);
            set_property(SYSTEM_PROP_IGNORE_AVB_STATE, Ok(self.ignore_avb));
            set_property_ok_value(SYSTEM_PROP_BUILD_TYPE, self.build_type);
        }
    }

    #[test]
    fn sdv_locked_avb_green_ok() -> anyhow::Result<()> {
        Props { sdv: LOCKED, avb: GREEN, ignore_avb: None, build_type: USER }.set();
        assert_eq!(get_sdv_boot_mode()?, SdvBootMode::Locked);
        Ok(())
    }

    #[test]
    fn sdv_unlocked_avb_green_ok() -> anyhow::Result<()> {
        Props { sdv: UNLOCKED, avb: GREEN, ignore_avb: None, build_type: USER }.set();
        assert_eq!(get_sdv_boot_mode()?, SdvBootMode::Unlocked);
        Ok(())
    }

    #[test]
    fn sdv_unlocked_avb_orange_ok() -> anyhow::Result<()> {
        Props { sdv: UNLOCKED, avb: ORANGE, ignore_avb: None, build_type: USER }.set();
        assert_eq!(get_sdv_boot_mode()?, SdvBootMode::Unlocked);
        Ok(())
    }

    #[test]
    fn sdv_locked_avb_orange_err() -> anyhow::Result<()> {
        Props { sdv: LOCKED, avb: ORANGE, ignore_avb: None, build_type: USER }.set();
        assert!(get_sdv_boot_mode().is_err());
        Ok(())
    }

    #[test]
    fn sdv_locked_ignore_avb_orange_userdebug_ok() -> anyhow::Result<()> {
        Props {
            sdv: LOCKED,
            avb: ORANGE,
            ignore_avb: Some("true".to_owned()),
            build_type: "userdebug",
        }
        .set();
        assert_eq!(get_sdv_boot_mode()?, SdvBootMode::Locked);
        Ok(())
    }

    /// Checks that you cannot ignore AVB state in user builds.
    #[test]
    fn sdv_locked_ignore_avb_orange_user_err() -> anyhow::Result<()> {
        Props { sdv: LOCKED, avb: ORANGE, ignore_avb: Some("true".to_owned()), build_type: USER }
            .set();
        assert!(get_sdv_boot_mode().is_err());
        Ok(())
    }

    #[test]
    fn sdv_locked_avb_yellow_err() -> anyhow::Result<()> {
        Props { sdv: LOCKED, avb: YELLOW, ignore_avb: None, build_type: USER }.set();
        assert!(get_sdv_boot_mode().is_err());
        Ok(())
    }

    #[test]
    fn sdv_unlocked_avb_yellow_err() -> anyhow::Result<()> {
        Props { sdv: UNLOCKED, avb: YELLOW, ignore_avb: None, build_type: USER }.set();
        assert!(get_sdv_boot_mode().is_err());
        Ok(())
    }
}
