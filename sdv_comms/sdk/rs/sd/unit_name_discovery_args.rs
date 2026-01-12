// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use bytemuck::TransparentWrapper;
use sdv_comms_bindgen::ASDV_UnitNameDiscoveryArgs;
use sdv_comms_common::ffi_str;
use sdv_status_rs::SdvResult;

/// Defines args for Service Unit discovery based on Unit Names.
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq)]
pub struct UnitNameDiscoveryArgs(ASDV_UnitNameDiscoveryArgs);

// SAFETY: [UnitNameDiscoveryArgs] is a transparent wrapper around ASDV_UnitNameDiscoveryArgs, and meets safety
// requirements for [TransparentWrapper].
unsafe impl TransparentWrapper<ASDV_UnitNameDiscoveryArgs> for UnitNameDiscoveryArgs {}

impl UnitNameDiscoveryArgs {
    /// Returns a str for the SDV Package Name of this UnitNameDiscoveryArgs.
    pub fn get_sdv_package_name(&self) -> &str {
        ffi_str::bytes_to_str(&self.0.sdv_package_name).expect("Invalid SDV package name")
    }

    /// Returns a str for the Service Bundle Name of this UnitNameDiscoveryArgs.
    pub fn get_service_bundle_name(&self) -> &str {
        ffi_str::bytes_to_str(&self.0.service_bundle_name).expect("Invalid service bundle name")
    }

    /// Returns a str for the Unit Name of this UnitNameDiscoveryArgs.
    pub fn get_unit_name(&self) -> Option<&str> {
        Some(ffi_str::bytes_to_str(&self.0.unit_name).expect("Invalid unit name"))
            .filter(|unit_name| !unit_name.is_empty())
    }

    /// Returns a str for the SDV VM Name of this UnitNameDiscoveryArgs.
    pub fn get_sdv_vm_name(&self) -> Option<&str> {
        Some(ffi_str::bytes_to_str(&self.0.sdv_vm_name).expect("Invalid SDV VM name"))
            .filter(|vm_name| !vm_name.is_empty())
    }

    /// Returns a new [UnitNameDiscoveryArgsBuilder].
    pub fn new_builder() -> UnitNameDiscoveryArgsBuilder {
        UnitNameDiscoveryArgsBuilder::default()
    }
}

/// Builder for [UnitNameDiscoveryArgs].
#[derive(Clone, Debug, Default)]
pub struct UnitNameDiscoveryArgsBuilder {
    sdv_package_name: String,
    service_bundle_name: String,
    service_unit_name: String,
    sdv_vm_name: String,
}

impl UnitNameDiscoveryArgsBuilder {
    /// Sets the SDV Package Name for this [UnitNameDiscoveryArgsBuilder].
    pub fn set_sdv_package_name(mut self, sdv_package_name: impl Into<String>) -> Self {
        self.sdv_package_name = sdv_package_name.into();
        self
    }

    /// Sets the Service Bundle Name for this [UnitNameDiscoveryArgsBuilder].
    pub fn set_service_bundle_name(mut self, service_bundle_name: impl Into<String>) -> Self {
        self.service_bundle_name = service_bundle_name.into();
        self
    }

    /// Sets the Service Unit Name for this [UnitNameDiscoveryArgsBuilder].
    pub fn set_service_unit_name(mut self, service_unit_name: impl Into<String>) -> Self {
        self.service_unit_name = service_unit_name.into();
        self
    }

    /// Sets the SDV VM Name for this [UnitNameDiscoveryArgsBuilder].
    pub fn set_sdv_vm_name(mut self, sdv_vm_name: impl Into<String>) -> Self {
        self.sdv_vm_name = sdv_vm_name.into();
        self
    }

    /// Builds a [UnitNameDiscoveryArgs] from this [UnitNameDiscoveryArgsBuilder].
    pub fn build(self) -> SdvResult<UnitNameDiscoveryArgs> {
        let mut unda = ASDV_UnitNameDiscoveryArgs::default();
        ffi_str::str_to_bytes(self.sdv_vm_name.as_str(), &mut unda.sdv_vm_name)?;
        ffi_str::str_to_bytes(self.sdv_package_name.as_str(), &mut unda.sdv_package_name)?;
        ffi_str::str_to_bytes(self.service_bundle_name.as_str(), &mut unda.service_bundle_name)?;
        ffi_str::str_to_bytes(self.service_unit_name.as_str(), &mut unda.unit_name)?;
        Ok(UnitNameDiscoveryArgs::wrap(unda))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::{assert_that, prelude::eq};

    #[test]
    fn test_default() {
        let args = UnitNameDiscoveryArgs::new_builder().build().unwrap();

        assert_that!(args.get_sdv_vm_name(), eq(None));
        assert_that!(args.get_sdv_package_name(), eq(""));
        assert_that!(args.get_service_bundle_name(), eq(""));
        assert_that!(args.get_unit_name(), eq(None));
    }

    #[test]
    fn test_initialized() {
        let args = UnitNameDiscoveryArgs::new_builder()
            .set_sdv_vm_name("vm")
            .set_sdv_package_name("com.goog.sdv")
            .set_service_bundle_name("WindowManager")
            .set_service_unit_name("unit")
            .build()
            .unwrap();

        assert_that!(args.get_sdv_vm_name(), eq(Some("vm")));
        assert_that!(args.get_sdv_package_name(), eq("com.goog.sdv"));
        assert_that!(args.get_service_bundle_name(), eq("WindowManager"));
        assert_that!(args.get_unit_name(), eq(Some("unit")));
    }

    #[test]
    fn test_initialized_with_empty_strings() {
        let args = UnitNameDiscoveryArgs::new_builder()
            .set_sdv_vm_name("")
            .set_sdv_package_name("com.goog.sdv")
            .set_service_bundle_name("WindowManager")
            .set_service_unit_name("")
            .build()
            .unwrap();

        assert_that!(args.get_sdv_vm_name(), eq(None));
        assert_that!(args.get_sdv_package_name(), eq("com.goog.sdv"));
        assert_that!(args.get_service_bundle_name(), eq("WindowManager"));
        assert_that!(args.get_unit_name(), eq(None));
    }
}
