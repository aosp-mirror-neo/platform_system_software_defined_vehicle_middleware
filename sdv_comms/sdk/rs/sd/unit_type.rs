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
use sdv_comms_bindgen::{
    sdv_status_t, ASDV_UnitType, ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
    ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
    ASDV_UNIT_TYPE_MAX_SIZE_BYTES_INCLUDING_NUL, ASDV_UNIT_TYPE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
};
use sdv_comms_common::ffi_str::{bytes_to_str, str_to_bytes};
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

/// Defines a Unit Type.
///
/// This name comprises the following components:
///  1) SDV Package Name: The Package Name within which the Service Bundle was shipped.
///  2) Service Bundle Name: (Deprecated; always empty) The Service Bundle Name as declared in the Package Manifest.
///  3) Unit Type Name: The Unit Type Name as declared in the Service Bundle definition.
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct UnitType(ASDV_UnitType);

// SAFETY: [UnitType] is a transparent wrapper around ASDV_UnitType, and meets safety
// requirements for [TransparentWrapper].
unsafe impl TransparentWrapper<ASDV_UnitType> for UnitType {}

impl UnitType {
    /// Returns builder that should be used to create `UnitType` instance.
    pub fn builder() -> UnitTypeBuilder {
        UnitTypeBuilder::default()
    }

    /// Returns a str for the SDV Package Name of this Unit Type.
    pub fn get_sdv_package_name(&self) -> &str {
        bytes_to_str(&self.0.sdv_package_name).expect("Invalid package name")
    }

    /// Returns a str for the Service Bundle Name of this Unit Type.
    #[deprecated(note = "Service bundle as part of unit types is deprecated and always empty.")]
    pub fn get_service_bundle_name(&self) -> &str {
        "" // service bundle as part of unit types is deprecated
    }

    /// Returns a str for the Unit Type Name of this Unit Type.
    pub fn get_unit_type_name(&self) -> &str {
        bytes_to_str(&self.0.unit_type_name).expect("Invalid unit type name")
    }
}

impl FromStr for UnitType {
    type Err = SdvStatus;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut ut_bytes = [0; ASDV_UNIT_TYPE_MAX_SIZE_BYTES_INCLUDING_NUL as usize];

        str_to_bytes(s, &mut ut_bytes)?;

        let mut unit_type = ASDV_UnitType {
            sdv_package_name: [0; ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as usize],
            service_bundle_name: [0; ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
                as usize],
            unit_type_name: [0; ASDV_UNIT_TYPE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as usize],
        };

        // SAFETY: both `ut_bytes` and `unit_type` are valid pointers and live for the duration
        // of the call. `ut_bytes` is a pointer to a NUL-terminated ASCII encoded sequence of bytes.
        let status: sdv_status_t = unsafe {
            sdv_comms_bindgen::ASDV_UnitType_fromString(ut_bytes.as_ptr(), &mut unit_type)
        };

        match SdvStatusCode::from(status) {
            SdvStatusCode::Ok => Ok(UnitType(unit_type)),
            status_code => Err(SdvStatus::new(status_code)),
        }
    }
}

impl fmt::Display for UnitType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut buf = [0; ASDV_UNIT_TYPE_MAX_SIZE_BYTES_INCLUDING_NUL as usize];
        let mut buf_size: u32 = buf.len() as u32;

        // SAFETY: `UnitType`, `buf` and `buf_size` are valid pointers that live for
        // the duration of the call.
        let status: sdv_status_t = unsafe {
            sdv_comms_bindgen::ASDV_UnitType_toString(
                &self.0,
                buf.as_mut_ptr().cast(),
                &mut buf_size,
            )
        };

        SdvStatusCode::from(status).expect("UnitType::to_string() failed");

        let str = bytes_to_str(buf.as_slice()).expect("Invalid UnitType");

        write!(f, "{str}")
    }
}

impl Hash for UnitType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get_sdv_package_name().hash(state);
        self.get_service_bundle_name().hash(state);
        self.get_unit_type_name().hash(state);
    }
}

/// Unit Type builder that is used to create a valid `UnitType` instance.
#[derive(Clone, Debug, Default)]
pub struct UnitTypeBuilder {
    sdv_package_name: String,
    #[allow(dead_code)]
    service_bundle_name: String,
    unit_type_name: String,
}

impl UnitTypeBuilder {
    /// Sets SDV Package Name.
    pub fn set_sdv_package_name(mut self, sdv_package_name: impl Into<String>) -> UnitTypeBuilder {
        self.sdv_package_name = sdv_package_name.into();
        self
    }

    /// Sets Service Bundle Name.
    #[deprecated]
    pub fn set_service_bundle_name(
        self,
        _service_bundle_name: impl Into<String>,
    ) -> UnitTypeBuilder {
        // service bundle name as part of unit types is deprecated
        self
    }

    /// Sets the Unit Type Name.
    pub fn set_unit_type_name(mut self, unit_type_name: impl Into<String>) -> UnitTypeBuilder {
        self.unit_type_name = unit_type_name.into();
        self
    }

    /// Validates all parts of the UnitType and if successful returns `UnitType`.
    pub fn build(self) -> SdvResult<UnitType> {
        let mut unit_type = ASDV_UnitType {
            sdv_package_name: [0; ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as usize],
            service_bundle_name: [0; ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
                as usize],
            unit_type_name: [0; ASDV_UNIT_TYPE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as usize],
        };

        // Handle qualified UnitType names.
        // If unit_type_name is "com.example.MyType", the part before the last dot ("com.example")
        // is prepended to the package name, and the part after the last dot ("MyType") becomes the unit type name.
        let (final_package_name, final_unit_type_name) =
            if let Some((package_prefix, unit_name)) = self.unit_type_name.rsplit_once('.') {
                // A qualified name was passed. Construct the full package name.
                let full_package = if self.sdv_package_name.is_empty() {
                    package_prefix.to_string()
                } else {
                    format!("{}.{}", self.sdv_package_name, package_prefix)
                };
                (full_package, unit_name.to_string())
            } else {
                // Not a qualified name, so we use the original values.
                (self.sdv_package_name, self.unit_type_name)
            };

        str_to_bytes(&final_package_name, &mut unit_type.sdv_package_name)?;
        str_to_bytes("", &mut unit_type.service_bundle_name)?;
        str_to_bytes(&final_unit_type_name, &mut unit_type.unit_type_name)?;

        // SAFETY: The passed ASDV_UnitType pointer is valid for the duration of the call.
        let status: sdv_status_t = unsafe { sdv_comms_bindgen::ASDV_UnitType_validate(&unit_type) };

        match SdvStatusCode::from(status) {
            SdvStatusCode::Ok => Ok(UnitType(unit_type)),
            status_code => Err(SdvStatus::new(status_code)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{collections::HashMap, str::FromStr};

    #[test]
    fn test_create_unit_type_should_succeed_from_valid_str_without_service_bundle() {
        let result = UnitType::from_str("com.google.sdv.Temperature");
        let unit_type = result.unwrap();

        assert_eq!(unit_type.get_sdv_package_name(), "com.google.sdv");
        assert_eq!(unit_type.get_service_bundle_name(), "");
        assert_eq!(unit_type.get_unit_type_name(), "Temperature");
    }

    #[test]
    fn test_create_unit_type_should_ignore_service_bundle() {
        let result = UnitType::from_str("com.google.sdv.WindowManager#Temperature");
        let unit_type = result.unwrap();

        assert_eq!(unit_type.get_sdv_package_name(), "com.google.sdv");
        assert_eq!(unit_type.get_service_bundle_name(), "");
        assert_eq!(unit_type.get_unit_type_name(), "Temperature");
    }

    #[test]
    fn test_create_unit_type_should_fail_from_empty_str() {
        let result = UnitType::from_str("");
        let status = result.unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_create_unit_type_should_fail_when_str_exceeds_max_size() {
        // ut with maximum allowed length.
        let s = "com.google.sdv.with_127_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234.ServiceWith_47_Symbols0123456789012345678901234#UnitTypeWith96symbolsintotal2354567890123456789012345678901234567890123456789012345678901234567";
        // Take NUL terminator into account and make sure that `s` length is maximum allowed.
        assert_eq!(s.len(), ASDV_UNIT_TYPE_MAX_SIZE_BYTES_INCLUDING_NUL as usize - 1);

        let result = UnitType::from_str(s);
        let ut = result.unwrap();

        assert_eq!(ut.get_sdv_package_name(), "com.google.sdv.with_127_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234");
        assert_eq!(ut.get_unit_type_name(), "UnitTypeWith96symbolsintotal2354567890123456789012345678901234567890123456789012345678901234567");

        // Add `x` to the end of `s` thus it should exceed the maximum allowed length.
        let result = UnitType::from_str(&format!("{}{}", s, 'x'));
        let status = result.unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_create_unit_type_should_fail_when_invalid_dot_count() {
        let result = UnitType::from_str("comgoogsdvWindowManager#Temperature");
        let status = result.unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::InvalidArgument);

        let result = UnitType::from_str("comgoogsdv.WindowManager#Temperature");
        let status = result.unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_create_unit_type_should_fail_when_invalid_hash_count() {
        let result = UnitType::from_str("com.google.sdv.WindowManager#mai#n");
        let status = result.unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_create_unit_type_should_fail_when_invalid_hash_order() {
        // Move '#' from right to left.
        let result = UnitType::from_str("com.google.sdv#WindowManager.Temperature");
        let status = result.unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::InvalidArgument);

        let result = UnitType::from_str("com.goog#sdv.WindowManager.Temperature");
        let status = result.unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_create_unit_type_should_fail_when_str_has_non_ascii() {
        let result = UnitType::from_str("cöm.google.sdv#WindöwManager.Temperature");
        let status = result.unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_create_unit_type_should_fail_when_str_has_nul_in_the_middle() {
        let result = UnitType::from_str("com.google.sdv.\0Temperature");
        let status = result.unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_unit_type_to_string_should_succeed() {
        let unit_type_string = "com.google.sdv.Temperature";

        let result = UnitType::from_str(unit_type_string);
        let unit_type = result.unwrap();

        assert_eq!(unit_type.to_string(), unit_type_string);
    }

    #[test]
    fn test_unit_type_as_hashmap_key_should_succeed() {
        let unit_type_string = "com.google.sdv.WindowManager#Temperature";

        let result = UnitType::from_str(unit_type_string);
        let unit_type = result.unwrap();

        let mut map = HashMap::new();
        assert_eq!(None, map.insert(unit_type, String::from("test")))
    }

    #[test]
    fn test_unit_type_to_max_string_length_should_succeed() {
        // Unit Type with maximum allowed length.
        let unit_type = "com.google.sdv.with_127_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234.UnitTypeWith96symbolsintotal2354567890123456789012345678901234567890123456789012345678901234567";

        // Take NUL terminator into account and make sure that `unit_type.` length is maximum allowed.
        assert_eq!(
            unit_type.len(),
            (ASDV_UNIT_TYPE_MAX_SIZE_BYTES_INCLUDING_NUL
                - ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL) as usize
                - 1
        );

        let result = UnitType::from_str(unit_type);
        let ut = result.unwrap();

        assert_eq!(ut.to_string(), unit_type);
    }

    #[test]
    fn test_build_unit_type_should_succeed() {
        let result = UnitType::builder()
            .set_sdv_package_name("com.goog.sdv")
            .set_service_bundle_name("WindowManager")
            .set_unit_type_name("Temperature")
            .build()
            .unwrap();

        assert_eq!(result.get_sdv_package_name(), "com.goog.sdv");
        assert_eq!(result.get_unit_type_name(), "Temperature");
    }

    #[test]
    fn test_build_unit_type_from_deprecated_input() {
        let result = UnitType::builder()
            .set_sdv_package_name("com.goog")
            .set_service_bundle_name("WindowManager")
            .set_unit_type_name("sdv.Temperature")
            .build()
            .unwrap();

        assert_eq!(result.get_sdv_package_name(), "com.goog.sdv");
        assert_eq!(result.get_unit_type_name(), "Temperature");
    }

    #[test]
    fn test_build_unit_type_should_succeed_when_strings_have_maximum_length() {
        let unit_type = UnitType::builder()
            .set_sdv_package_name("com.google.sdv.with_127_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234")
            .set_service_bundle_name("ServiceWith_47_Symbols0123456789012345678901234")
            .set_unit_type_name("UnitTypeWith47symbolsintotal2354567890123456789")
            .build()
            .unwrap();
        assert_eq!(unit_type.get_sdv_package_name(), "com.google.sdv.with_127_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234");
        assert_eq!(
            unit_type.get_unit_type_name(),
            "UnitTypeWith47symbolsintotal2354567890123456789"
        );
    }

    #[test]
    fn test_build_unit_type_should_fail_when_invalid_sdv_package_name() {
        for sdv_package_name in [
            "",
            "comgoogsdv",
            "non.ascїї.sdv",
            "com.google.sdv.with_128_length_x012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345"
        ] {
            let err = UnitType::builder()
                .set_sdv_package_name(sdv_package_name)
                .set_service_bundle_name("WindowManager")
                .set_unit_type_name("Temperature")
                .build()
                .unwrap_err();
            assert_eq!(
                err.status_code(),
                SdvStatusCode::InvalidArgument,
                "Unexpected result for SDV Package Name: {sdv_package_name:?}"
            );
        }
    }

    #[test]
    fn test_build_unit_type_should_fail_when_invalid_unit_type_name() {
        for unit_type_name in [
            "",
            "0Temperature",
            "temperature",
            "Te__mperature",
            "non-ascїї-unit_type-name",
            "UnitType_has_96_symbols_901234567890123456789012000011110000111100001111000011110000111100001111",
        ] {
            let err = UnitType::builder()
                .set_sdv_package_name("com.google.sdv")
                .set_service_bundle_name("WindowManager")
                .set_unit_type_name(unit_type_name)
                .build()
                .unwrap_err();
            assert_eq!(
                err.status_code(),
                SdvStatusCode::InvalidArgument,
                "Unexpected result for Unit Type Name: {unit_type_name:?}"
            );
        }
    }
}
