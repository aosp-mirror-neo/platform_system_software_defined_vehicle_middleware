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

use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::UnitNameParameters::UnitNameParameters as UnitNameDiscoveryArgsAidl;
use sdv_comms_common::ffi_str;
use sdv_comms_id::names::{
    PackageName, ServiceName, ServiceUnitName, VmName,
};
use sdv_comms_id_validator::{
    PackageNameValidationError, ServiceNameValidationError, ServiceUnitNameValidationError, VmNameValidationError
};
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::str::FromStr;
use thiserror::Error;

use crate::ffi::ASDV_UnitNameDiscoveryArgs;

#[derive(Error, Debug, PartialEq)]
pub enum UnitNameDiscoveryArgsValidationError {
    /// Indicates that Unit Type is empty.
    #[error("should not be empty")]
    IsEmpty,
    #[error("invalid Package Name: {0}")]
    InvalidPackageName(#[from] PackageNameValidationError),
    /// Indicates Service Bundle Name error.
    #[error("invalid Service Name: {0}")]
    InvalidServiceName(#[from] ServiceNameValidationError),
    /// Indicates Unit Type Name error.
    #[error("invalid Unit Type Name: {0}")]
    InvalidUnitName(#[from] ServiceUnitNameValidationError),
    /// Indicates Vm Name error.
    #[error("invalid Vm Name: {0}")]
    InvalidVmName(#[from] VmNameValidationError),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct UnitNameDiscoveryArgs {
    package_name: PackageName,
    service_bundle_name: ServiceName,
    service_unit_name: Option<ServiceUnitName>,
    vm_name: Option<VmName>,
}

impl UnitNameDiscoveryArgs {
    /// Returns builder that should be used to create [UnitNameDiscoveryArgs] instance.
    pub fn builder() -> UnitNameDiscoveryArgsBuilder {
        UnitNameDiscoveryArgsBuilder::default()
    }
    /// Returns SDV Package Name.
    pub fn package_name(&self) -> &PackageName {
        &self.package_name
    }
    /// Returns SDV Service Bundle Name.
    pub fn service_bundle_name(&self) -> &ServiceName {
        &self.service_bundle_name
    }
    /// Returns Unit Name.
    pub fn service_unit_name(&self) -> Option<&ServiceUnitName> {
        self.service_unit_name.as_ref()
    }
    /// Returns Unit Name.
    pub fn vm_name(&self) -> Option<&VmName> {
        self.vm_name.as_ref()
    }
}

/// Converts ASDV_UnitNameDiscoveryArgs to UnitNameDiscoveryArgs struct.
impl TryFrom<&ASDV_UnitNameDiscoveryArgs> for UnitNameDiscoveryArgs {
    type Error = SdvStatus;

    fn try_from(asdv: &ASDV_UnitNameDiscoveryArgs) -> SdvResult<UnitNameDiscoveryArgs> {
        UnitNameDiscoveryArgs::builder()
            .vm_name(ffi_str::bytes_to_str(&asdv.sdv_vm_name)?)
            .package_name(ffi_str::bytes_to_str(&asdv.sdv_package_name)?)
            .service_bundle_name(ffi_str::bytes_to_str(&asdv.service_bundle_name)?)
            .service_unit_name(ffi_str::bytes_to_str(&asdv.unit_name)?)
            .build()
            .map_err(|_| {
                SdvStatus::with_message(
                    SdvStatusCode::InvalidArgument,
                    "One or more arguments in ASDV_UnitNameDiscoveryArgs are invalid strings",
                )
            })
    }
}

impl From<UnitNameDiscoveryArgs> for UnitNameDiscoveryArgsAidl {
    fn from(unda: UnitNameDiscoveryArgs) -> Self {
        UnitNameDiscoveryArgsAidl {
            sdvVmName: unda.vm_name().map(|vm_name| vm_name.to_string()),
            sdvPackageName: unda.package_name().to_string(),
            serviceBundleName: unda.service_bundle_name().to_string(),
            unitName: unda.service_unit_name().map(|vm_name| vm_name.to_string()),
        }
    }
}

/// Builder for [UnitNameDiscoveryArgs].
#[derive(Clone, Debug, Default)]
pub struct UnitNameDiscoveryArgsBuilder {
    package_name: String,
    service_bundle_name: String,
    service_unit_name: Option<String>,
    vm_name: Option<String>,
}

impl UnitNameDiscoveryArgsBuilder {
    /// Sets the SDV Package Name for this [UnitNameDiscoveryArgsBuilder].
    pub fn package_name(mut self, package_name: impl Into<String>) -> Self {
        self.package_name = package_name.into();
        self
    }

    /// Sets the Service Bundle Name for this [UnitNameDiscoveryArgsBuilder].
    pub fn service_bundle_name(mut self, service_bundle_name: impl Into<String>) -> Self {
        self.service_bundle_name = service_bundle_name.into();
        self
    }

    /// Sets the Service Unit Name for this [UnitNameDiscoveryArgsBuilder].
    pub fn service_unit_name(mut self, service_unit_name: impl Into<String>) -> Self {
        self.service_unit_name =
            Some(service_unit_name.into()).filter(|unit_name| !unit_name.is_empty());
        self
    }

    /// Sets the SDV VM Name for this [UnitNameDiscoveryArgsBuilder].
    pub fn vm_name(mut self, vm_name: impl Into<String>) -> Self {
        self.vm_name = Some(vm_name.into()).filter(|vm_name| !vm_name.is_empty());
        self
    }

    /// Builds a [UnitNameDiscoveryArgs] from this [UnitNameDiscoveryArgsBuilder].
    pub fn build(self) -> Result<UnitNameDiscoveryArgs, UnitNameDiscoveryArgsValidationError> {
        let package_name = PackageName::from_str(&self.package_name)?;
        let service_bundle_name = ServiceName::from_str(&self.service_bundle_name)?;
        let service_unit_name = self
            .service_unit_name
            .as_ref()
            .map(|unit_name| ServiceUnitName::from_str(unit_name))
            .transpose()?;
        let vm_name = self.vm_name.as_ref().map(|vm_name| VmName::from_str(vm_name)).transpose()?;
        Ok(UnitNameDiscoveryArgs { package_name, service_bundle_name, service_unit_name, vm_name })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdv_comms_id_validator::{AlphanumHyphenValidationError, PackageIdentifierValidationError};

    #[test]
    fn test_build_unda_should_succeed() {
        let result = UnitNameDiscoveryArgs::builder()
            .vm_name("vm")
            .package_name("com.goog.sdv")
            .service_bundle_name("WindowManager")
            .service_unit_name("unit")
            .build()
            .unwrap();

        assert_eq!(result.vm_name().unwrap().name(), "vm");
        assert_eq!(result.package_name().name(), "com.goog.sdv");
        assert_eq!(result.service_bundle_name().name(), "WindowManager");
        assert_eq!(result.service_unit_name().unwrap().name(), "unit");
    }

    #[test]
    fn test_build_unda_should_fail_when_missing_package_name() {
        let result = UnitNameDiscoveryArgs::builder()
            .service_bundle_name("WindowManager")
            .service_unit_name("unit")
            .build();
        assert_eq!(
            result,
            Err(UnitNameDiscoveryArgsValidationError::InvalidPackageName(
                PackageNameValidationError(PackageIdentifierValidationError::IsEmpty)
            ))
        );
    }

    #[test]
    fn test_build_unda_should_fail_when_missing_service_bundle_name() {
        let result = UnitNameDiscoveryArgs::builder()
            .package_name("com.goog.sdv")
            .service_unit_name("unit")
            .build();
        assert_eq!(
            result,
            Err(UnitNameDiscoveryArgsValidationError::InvalidServiceName(
                ServiceNameValidationError::IsEmpty
            ))
        );
    }

    #[test]
    fn test_build_unda_should_build_when_missing_service_unit_name() {
        let result = UnitNameDiscoveryArgs::builder()
            .vm_name("vm")
            .package_name("com.goog.sdv")
            .service_bundle_name("WindowManager")
            .service_unit_name("")
            .build()
            .unwrap();
        assert_eq!(result.vm_name().unwrap().name(), "vm");
        assert_eq!(result.package_name().name(), "com.goog.sdv");
        assert_eq!(result.service_bundle_name().name(), "WindowManager");
        assert_eq!(result.service_unit_name(), None);
    }

    #[test]
    fn test_build_unda_should_build_when_missing_vm_name() {
        let result = UnitNameDiscoveryArgs::builder()
            .package_name("com.goog.sdv")
            .service_bundle_name("WindowManager")
            .service_unit_name("unit")
            .vm_name("")
            .build()
            .unwrap();
        assert_eq!(result.package_name().name(), "com.goog.sdv");
        assert_eq!(result.service_bundle_name().name(), "WindowManager");
        assert_eq!(result.service_unit_name().unwrap().name(), "unit");
        assert_eq!(result.vm_name(), None);
    }

    #[test]
    fn test_build_unit_type_should_fail_when_invalid_package_name() {
        let result = UnitNameDiscoveryArgs::builder()
            .package_name("comgoogsdv")
            .service_bundle_name("WindowManager")
            .service_unit_name("unit")
            .build();
        assert_eq!(
            result,
            Err(UnitNameDiscoveryArgsValidationError::InvalidPackageName(
                PackageNameValidationError(PackageIdentifierValidationError::HasOneSegment)
            ))
        );
    }

    #[test]
    fn test_build_unit_type_should_fail_when_invalid_service_bundle_name() {
        let result = UnitNameDiscoveryArgs::builder()
            .package_name("com.goog.sdv")
            .service_bundle_name("Window__Manager")
            .service_unit_name("unit")
            .build();
        assert_eq!(
            result,
            Err(UnitNameDiscoveryArgsValidationError::InvalidServiceName(
                ServiceNameValidationError::ContainsConsecutiveUnderscores
            ))
        );
    }

    #[test]
    fn test_build_unit_type_should_fail_when_service_unit_name_starts_with_number() {
        let result = UnitNameDiscoveryArgs::builder()
            .package_name("com.goog.sdv")
            .service_bundle_name("WindowManager")
            .service_unit_name("0unit")
            .build();
        assert_eq!(
            result,
            Err(UnitNameDiscoveryArgsValidationError::InvalidUnitName(
                ServiceUnitNameValidationError(
                    AlphanumHyphenValidationError::StartsWithInvalidCharacter('0')
                )
            ))
        );
    }

    #[test]
    fn test_build_unit_type_should_fail_when_service_unit_name_contains_two_consequtive_underscores(
    ) {
        let result = UnitNameDiscoveryArgs::builder()
            .package_name("com.goog.sdv")
            .service_bundle_name("WindowManager")
            .service_unit_name("uni--t")
            .build();
        assert_eq!(
            result,
            Err(UnitNameDiscoveryArgsValidationError::InvalidUnitName(
                ServiceUnitNameValidationError(
                    AlphanumHyphenValidationError::ContainsConsecutiveHyphens
                )
            ))
        );
    }
}
