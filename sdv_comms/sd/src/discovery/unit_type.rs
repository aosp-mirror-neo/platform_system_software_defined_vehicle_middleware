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

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::UnitType::UnitType as UnitTypeAidl;
use sdv_comms_common::ffi_str;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

use crate::ffi::ASDV_UnitType;

use sdv_comms_id::names::{PackageName, ServiceName, UnitTypeName};
use sdv_comms_id_validator::{
    PackageNameValidationError, ServiceNameValidationError, UnitTypeNameValidationError,
    PACKAGE_NAME_MAX_LENGTH_BYTES, SERVICE_NAME_MAX_LENGTH_BYTES, UNIT_TYPE_NAME_MAX_LENGTH_BYTES,
};

/// The maximum length of the Unit Type in bytes.
///
/// Also includes 2 delimiters: '.' and '#'.
pub const UNIT_TYPE_MAX_LENGTH_BYTES: usize = SERVICE_NAME_MAX_LENGTH_BYTES
    + PACKAGE_NAME_MAX_LENGTH_BYTES
    + UNIT_TYPE_NAME_MAX_LENGTH_BYTES
    + 2;

/// Error that can arise during Unit Type validation.
#[derive(Error, Debug, PartialEq)]
pub enum UnitTypeValidationError {
    /// Indicates that Unit Type is empty.
    #[error("should not be empty")]
    IsEmpty,
    /// Indicates that Unit Type exceeds max length in bytes.
    #[error("should not be longer than {0} bytes, found {1} bytes")]
    ExceedsMaxLength(usize, usize),
    /// Indicates that Unit Type contains invalid count of delimiters.
    #[error("delimiter '{0}' should occur once, found {1} times")]
    InvalidDelimitersCount(char, usize),
    /// Indicates that Unit Type contains invalid count of dots.
    /// At least one dot for separating Package Name from Service Bundle Name.
    #[error("delimiter '.' should occur at least 2 times, found {0} times")]
    InvalidDotsCount(usize),
    /// Indicates that Unit Type contains delimiters in the wrong order.
    #[error("delimiter '{0}' should occur before delimiter '{1}'")]
    InvalidDelimitersOrder(char, char),
    /// Indicates Package Name error.
    #[error("invalid Package Name: {0}")]
    InvalidPackageName(#[from] PackageNameValidationError),
    /// Indicates Unit Type Name error.
    #[error("invalid Unit Type Name: {0}")]
    InvalidUnitTypeName(#[from] UnitTypeNameValidationError),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct UnitType {
    package_name: PackageName,
    #[deprecated]
    service_bundle_name: ServiceName,
    unit_type_name: UnitTypeName,
}

impl UnitType {
    /// Returns builder that should be used to create `UnitType` instance.
    pub fn builder() -> UnitTypeBuilder {
        UnitTypeBuilder {
            package_name: String::new(),
            service_bundle_name: String::new(),
            unit_type_name: String::new(),
        }
    }
    /// Returns SDV Package Name.
    pub fn package_name(&self) -> &PackageName {
        &self.package_name
    }
    /// Returns SDV Service Bundle Name.
    #[deprecated]
    pub fn service_bundle_name(&self) -> &ServiceName {
        &self.service_bundle_name
    }
    /// Returns Unit Type Name.
    pub fn unit_type_name(&self) -> &UnitTypeName {
        &self.unit_type_name
    }

    /// Clones this UnitType instance into an ASDV_UnitType.
    pub fn clone_into(&self, asdv_ut: &mut ASDV_UnitType) -> SdvResult<()> {
        ffi_str::str_to_bytes(self.package_name().name(), &mut asdv_ut.sdv_package_name)?;
        // service bundle name is deprecated, so it is not cloned
        ffi_str::str_to_bytes("", &mut asdv_ut.service_bundle_name)?;
        ffi_str::str_to_bytes(self.unit_type_name().name(), &mut asdv_ut.unit_type_name)?;
        Ok(())
    }
}

impl FromStr for UnitType {
    type Err = UnitTypeValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(UnitTypeValidationError::IsEmpty);
        }

        if s.len() > UNIT_TYPE_MAX_LENGTH_BYTES {
            return Err(UnitTypeValidationError::ExceedsMaxLength(
                UNIT_TYPE_MAX_LENGTH_BYTES,
                s.len(),
            ));
        }

        if !s.contains('.') {
            return Err(UnitTypeValidationError::InvalidDotsCount(0));
        }

        let package_name;
        let unit_type_name;

        if let Some(hash_index) = s.rfind('#') {
            // Backwards compatibility: 'a.B#c.D' is treated as 'a.c.D'
            let prefix = &s[..hash_index];
            unit_type_name = &s[hash_index + 1..];
            if let Some(dot_index) = prefix.rfind('.') {
                package_name = &prefix[..dot_index];
            } else {
                package_name = prefix;
            }
        } else if let Some(dot_index) = s.rfind('.') {
            package_name = &s[..dot_index];
            unit_type_name = &s[dot_index + 1..];
        } else {
            return Err(UnitTypeValidationError::InvalidDotsCount(0));
        }

        if !package_name.contains('.') {
            // This handles cases like "PackageName.UnitType" where PackageName has no dot.
            return Err(UnitTypeValidationError::InvalidDotsCount(1));
        }

        UnitType::builder().package_name(package_name).unit_type_name(unit_type_name).build()
    }
}

/// Converts ASDV_UnitType to UnitType struct.
impl TryFrom<&ASDV_UnitType> for UnitType {
    type Error = SdvStatus;

    fn try_from(asdv_ut: &ASDV_UnitType) -> SdvResult<UnitType> {
        UnitType::builder()
            .package_name(ffi_str::bytes_to_str(&asdv_ut.sdv_package_name)?)
            .unit_type_name(ffi_str::bytes_to_str(&asdv_ut.unit_type_name)?)
            .build()
            .map_err(|_| {
                SdvStatus::with_message(
                    SdvStatusCode::InvalidArgument,
                    "One or more arguments in ASDV_UnitType are invalid strings",
                )
            })
    }
}

impl TryFrom<UnitTypeAidl> for UnitType {
    type Error = SdvStatus;

    fn try_from(aidl: UnitTypeAidl) -> Result<Self, Self::Error> {
        UnitType::builder()
            .package_name(aidl.sdvPackageName)
            .unit_type_name(aidl.typeName)
            .build()
            .map_err(|_| {
                SdvStatus::with_message(
                    SdvStatusCode::InvalidArgument,
                    "One or more arguments in UnitTypeAidl are invalid",
                )
            })
    }
}

impl From<UnitType> for UnitTypeAidl {
    fn from(unit_type: UnitType) -> Self {
        UnitTypeAidl {
            sdvPackageName: unit_type.package_name().to_string(),
            serviceBundleName: "".to_string(), // service bundle name is deprecated
            typeName: unit_type.unit_type_name().to_string(),
        }
    }
}

impl fmt::Display for UnitType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.package_name, self.unit_type_name)
    }
}

/// Unit Type builder that is used to create a valid `Unit Type` instance.
pub struct UnitTypeBuilder {
    package_name: String,
    #[deprecated]
    service_bundle_name: String,
    unit_type_name: String,
}

impl UnitTypeBuilder {
    /// Sets Package Name.
    pub fn package_name(mut self, package_name: impl Into<String>) -> UnitTypeBuilder {
        self.package_name = package_name.into();
        self
    }

    /// Sets Service Bundle Name.
    #[deprecated]
    pub fn service_bundle_name(self, _service_bundle_name: impl Into<String>) -> UnitTypeBuilder {
        self
    }

    /// Sets Unit Type Name.
    pub fn unit_type_name(mut self, unit_type_name: impl Into<String>) -> UnitTypeBuilder {
        self.unit_type_name = unit_type_name.into();
        self
    }

    /// Validates all parts of the Unit Type and if successful returns `UnitType`.
    pub fn build(self) -> Result<UnitType, UnitTypeValidationError> {
        let package_name = PackageName::from_str(&self.package_name)?;
        let service_bundle_name = ServiceName::default(); // service bundle name is deprecated
        let unit_type_name = UnitTypeName::from_str(&self.unit_type_name)?;
        Ok(UnitType { package_name, service_bundle_name, unit_type_name })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdv_comms_id_validator::PackageIdentifierValidationError;

    #[test]
    fn test_create_unit_type_should_succeed_from_valid_str_with_hash() {
        let result = UnitType::from_str("com.goog.sdv.WindowManager#Temperature").unwrap();

        assert_eq!(result.package_name().name(), "com.goog.sdv");
        assert_eq!(result.service_bundle_name().name(), "");
        assert_eq!(result.unit_type_name().name(), "Temperature");
    }

    #[test]
    fn test_create_unit_type_should_succeed_from_valid_str_with_dot() {
        let result = UnitType::from_str("com.goog.sdv.Temperature").unwrap();

        assert_eq!(result.package_name().name(), "com.goog.sdv");
        assert_eq!(result.service_bundle_name().name(), "");
        assert_eq!(result.unit_type_name().name(), "Temperature");
    }

    #[test]
    fn test_create_unit_type_should_fail_from_empty_str() {
        let result = UnitType::from_str("");
        assert_eq!(result, Err(UnitTypeValidationError::IsEmpty));
    }

    #[test]
    fn test_create_unit_type_should_fail_when_str_exceeds_max_size() {
        // ut with maximum allowed length.
        let s = "com.google.sdv.with_127_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234\
                 .ServiceWith_47_Symbols0123456789012345678901234\
                 #UnitTypeWith47symbolsintotal2354567890123456789000011110000111100001111000011110000111100001111";
        assert_eq!(s.len(), UNIT_TYPE_MAX_LENGTH_BYTES);
        assert!(UnitType::from_str(s).is_ok());

        let result = UnitType::from_str(&format!("{}{}", s, 'x'));
        assert_eq!(
            result,
            Err(UnitTypeValidationError::ExceedsMaxLength(
                UNIT_TYPE_MAX_LENGTH_BYTES,
                UNIT_TYPE_MAX_LENGTH_BYTES + 1
            ))
        );
    }

    #[test]
    fn test_create_unit_type_should_fail_when_invalid_dot_count() {
        let result = UnitType::from_str("comgoogsdvWindowManager#Temperature");
        assert_eq!(result, Err(UnitTypeValidationError::InvalidDotsCount(0)));

        let result = UnitType::from_str("comgoogsdv.WindowManager#Temperature");
        assert_eq!(result, Err(UnitTypeValidationError::InvalidDotsCount(1)));
    }

    #[test]
    fn test_create_unit_type_should_fail_when_invalid_hash_count() {
        let result = UnitType::from_str("com.goog.sdv.WindowManager#mai#n");
        assert_eq!(
            result,
            Err(UnitTypeValidationError::InvalidUnitTypeName(
                UnitTypeNameValidationError::TypeNameStartsWithInvalidCharacter('n')
            ))
        );
    }

    #[test]
    fn test_create_unit_type_should_fail_when_invalid_subpackage_name() {
        let result = UnitType::from_str("com.goog.sdv.WindowManager#Google.Temperature");
        assert_eq!(
            result,
            Err(UnitTypeValidationError::InvalidUnitTypeName(
                UnitTypeNameValidationError::SubpackageNameStartsWithInvalidCharacter('G')
            ))
        );
    }

    #[test]
    fn test_unit_type_subpackage_should_succeed() {
        let s = "com.goog.sdv.WindowManager#com.google.Temperature";
        let result = UnitType::from_str(s).unwrap();
        assert_eq!(format!("{result}"), "com.goog.sdv.com.google.Temperature");
    }

    #[test]
    fn test_unit_type_formatting() {
        let s = "com.goog.sdv.WindowManager#Temperature";
        let result = UnitType::from_str(s).unwrap();
        assert_eq!(format!("{result}"), "com.goog.sdv.Temperature");
    }

    #[test]
    fn test_build_unit_type_should_succeed() {
        let result = UnitType::builder()
            .package_name("com.goog.sdv")
            .service_bundle_name("WindowManager")
            .unit_type_name("Temperature")
            .build()
            .unwrap();

        assert_eq!(result.package_name().name(), "com.goog.sdv");
        assert_eq!(result.service_bundle_name().name(), "");
        assert_eq!(result.unit_type_name().name(), "Temperature");
    }

    #[test]
    fn test_build_unit_type_should_fail_when_missing_package_name() {
        let result = UnitType::builder()
            .service_bundle_name("WindowManager")
            .unit_type_name("Temperature")
            .build();
        assert_eq!(
            result,
            Err(UnitTypeValidationError::InvalidPackageName(PackageNameValidationError(
                PackageIdentifierValidationError::IsEmpty
            )))
        );
    }

    #[test]
    fn test_build_unit_type_should_fail_when_missing_unit_type_name() {
        let result = UnitType::builder()
            .package_name("com.goog.sdv")
            .service_bundle_name("WindowManager")
            .build();
        assert_eq!(
            result,
            Err(UnitTypeValidationError::InvalidUnitTypeName(UnitTypeNameValidationError::IsEmpty))
        );
    }

    #[test]
    fn test_build_unit_type_should_fail_when_invalid_package_name() {
        let result = UnitType::builder()
            .package_name("comgoogsdv")
            .service_bundle_name("WindowManager")
            .unit_type_name("Temperature")
            .build();
        assert_eq!(
            result,
            Err(UnitTypeValidationError::InvalidPackageName(PackageNameValidationError(
                PackageIdentifierValidationError::HasOneSegment
            )))
        );
    }

    #[test]
    fn test_build_unit_type_should_succeed_when_invalid_service_bundle_name() {
        let result = UnitType::builder()
            .package_name("com.goog.sdv")
            .service_bundle_name("Window__Manager")
            .unit_type_name("Temperature")
            .build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_unit_type_should_fail_when_unit_type_name_starts_with_number() {
        let result = UnitType::builder()
            .package_name("com.goog.sdv")
            .service_bundle_name("WindowManager")
            .unit_type_name("0Temperature")
            .build();
        assert_eq!(
            result,
            Err(UnitTypeValidationError::InvalidUnitTypeName(
                UnitTypeNameValidationError::TypeNameStartsWithInvalidCharacter('0')
            ))
        );
    }

    #[test]
    fn test_build_unit_type_should_fail_when_unit_type_name_starts_with_small_letter() {
        let result = UnitType::builder()
            .package_name("com.goog.sdv")
            .service_bundle_name("WindowManager")
            .unit_type_name("temperature")
            .build();
        assert_eq!(
            result,
            Err(UnitTypeValidationError::InvalidUnitTypeName(
                UnitTypeNameValidationError::TypeNameStartsWithInvalidCharacter('t')
            ))
        );
    }

    #[test]
    fn test_build_unit_type_should_fail_when_unit_type_name_contains_two_consecutive_underscores() {
        let result = UnitType::builder()
            .package_name("com.goog.sdv")
            .service_bundle_name("WindowManager")
            .unit_type_name("Te__mperature")
            .build();
        assert_eq!(
            result,
            Err(UnitTypeValidationError::InvalidUnitTypeName(
                UnitTypeNameValidationError::ContainsConsecutiveUnderscores
            ))
        );
    }
}
