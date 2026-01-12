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

//! This file creates and represents the Service FQIN (Fully Qualified Instance Name).

use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinAidl;
use sdv_comms_common::ffi_str;
use sdv_status_rs::{derive, SdvResult, SdvStatus, SdvStatusCode};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

use crate::ffi::ASDV_ServiceFqin;
use crate::names::{InstanceName, PackageName, ServiceName, VmName};
use sdv_comms_id_validator::{
    InstanceNameValidationError, PackageNameValidationError, ServiceNameValidationError,
    VmNameValidationError, INSTANCE_NAME_MAX_LENGTH_BYTES, PACKAGE_NAME_MAX_LENGTH_BYTES,
    SERVICE_NAME_MAX_LENGTH_BYTES, VM_NAME_MAX_LENGTH_BYTES,
};

/// Max length of the Service FQIN in bytes.
///
/// Also include 3 delimiters: ':', '.' and '/'.
pub const SERVICE_FQIN_MAX_LENGTH_BYTES: usize = VM_NAME_MAX_LENGTH_BYTES
    + SERVICE_NAME_MAX_LENGTH_BYTES
    + PACKAGE_NAME_MAX_LENGTH_BYTES
    + INSTANCE_NAME_MAX_LENGTH_BYTES
    + 3;

/// Error that can arise during Service FQIN validation.
#[derive(Error, Debug, PartialEq)]
pub enum ServiceFqinValidationError {
    /// Indicates that Service FQIN is empty.
    #[error("should not be empty")]
    IsEmpty,
    /// Indicates that Service FQIN exceeds max length in bytes.
    #[error("should not be longer than {0} bytes, found {1} bytes")]
    ExceedsMaxLength(usize, usize),
    /// Indicates that Service FQIN contains invalid count of delimiters.
    #[error("delimiter '{0}' should occur once, found {1} times")]
    InvalidDelimitersCount(char, usize),
    /// Indicates that Service FQIN contains invalid count of dots.
    /// At least one dot for the Package Name and
    /// one dot for separating Package Name from Service Name.
    #[error("delimiter '.' should occur at least 2 times, found {0} times")]
    InvalidDotsCount(usize),
    /// Indicates that Service FQIN contains delimiters in the wrong order.
    #[error("delimiter '{0}' should occur before delimiter '{1}'")]
    InvalidDelimitersOrder(char, char),
    /// Indicates VM Name error.
    #[error("invalid VM Name: {0}")]
    InvalidVmName(#[from] VmNameValidationError),
    /// Indicates Package Name error.
    #[error("invalid Package Name: {0}")]
    InvalidPackageName(#[from] PackageNameValidationError),
    /// Indicates Service Name error.
    #[error("invalid Service Name: {0}")]
    InvalidServiceName(#[from] ServiceNameValidationError),
    /// Indicates Instance Name error.
    #[error("invalid Instance Name: {0}")]
    InvalidInstanceName(#[from] InstanceNameValidationError),
}

/// Fully Qualified Instance Name of the Service.
/// TODO(b/335912231): Replace with an AIDL object here.
#[derive(Clone, Debug, PartialEq)]
pub struct ServiceFqin {
    vm_name: VmName,
    package_name: PackageName,
    service_name: ServiceName,
    instance_name: InstanceName,
}

impl ServiceFqin {
    /// Returns builder that should be used to create `ServiceFqin` instance.
    pub fn builder() -> ServiceFqinBuilder {
        ServiceFqinBuilder {
            vm_name: String::new(),
            package_name: String::new(),
            service_name: String::new(),
            instance_name: String::new(),
        }
    }

    /// Returns VM Name.
    pub fn vm_name(&self) -> &VmName {
        &self.vm_name
    }

    /// Returns Package Name.
    pub fn package_name(&self) -> &PackageName {
        &self.package_name
    }

    /// Returns Service Name.
    pub fn service_name(&self) -> &ServiceName {
        &self.service_name
    }

    /// Returns Instance Name.
    pub fn instance_name(&self) -> &InstanceName {
        &self.instance_name
    }

    /// Clones this ServiceFqin instance into an ASDV_ServiceFqin.
    pub fn clone_into(&self, asdv_fqin: &mut ASDV_ServiceFqin) -> SdvResult<()> {
        ffi_str::str_to_bytes(self.vm_name().name(), &mut asdv_fqin.sdv_vm_name)?;
        ffi_str::str_to_bytes(self.package_name().name(), &mut asdv_fqin.sdv_package_name)?;
        ffi_str::str_to_bytes(self.service_name().name(), &mut asdv_fqin.service_bundle_name)?;
        ffi_str::str_to_bytes(self.instance_name().name(), &mut asdv_fqin.service_instance_name)?;
        Ok(())
    }
}

impl FromStr for ServiceFqin {
    type Err = ServiceFqinValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(ServiceFqinValidationError::IsEmpty);
        }

        if s.len() > SERVICE_FQIN_MAX_LENGTH_BYTES {
            return Err(ServiceFqinValidationError::ExceedsMaxLength(
                SERVICE_FQIN_MAX_LENGTH_BYTES,
                s.len(),
            ));
        }

        const COLON: char = ':';
        const DOT: char = '.';
        const SLASH: char = '/';

        let mut colons_indices = Vec::new();
        let mut dots_indices = Vec::new();
        let mut slashes_indices = Vec::new();
        for (index, ch) in s.char_indices() {
            match ch {
                ':' => colons_indices.push(index),
                '.' => dots_indices.push(index),
                '/' => slashes_indices.push(index),
                _ => {}
            }
        }

        if colons_indices.len() != 1 {
            return Err(ServiceFqinValidationError::InvalidDelimitersCount(
                COLON,
                colons_indices.len(),
            ));
        }
        // At least one dot for the Package Name and
        // one dot for separating Package Name from Service Name.
        if dots_indices.len() < 2 {
            return Err(ServiceFqinValidationError::InvalidDotsCount(dots_indices.len()));
        }
        if slashes_indices.len() != 1 {
            return Err(ServiceFqinValidationError::InvalidDelimitersCount(
                SLASH,
                slashes_indices.len(),
            ));
        }

        let colon_index = colons_indices[0];
        let dot_index = dots_indices[dots_indices.len() - 1];
        let slash_index = slashes_indices[0];

        if colon_index > dots_indices[0] {
            return Err(ServiceFqinValidationError::InvalidDelimitersOrder(DOT, COLON));
        }

        if dot_index > slash_index {
            return Err(ServiceFqinValidationError::InvalidDelimitersOrder(SLASH, DOT));
        }

        ServiceFqin::builder()
            .vm_name(&s[..colon_index])
            .package_name(&s[colon_index + 1..dot_index])
            .service_name(&s[dot_index + 1..slash_index])
            .instance_name(&s[slash_index + 1..])
            .build()
    }
}

impl TryFrom<&str> for ServiceFqin {
    type Error = ServiceFqinValidationError;

    fn try_from(string: &str) -> Result<Self, ServiceFqinValidationError> {
        ServiceFqin::from_str(string)
    }
}

impl TryFrom<ServiceFqinAidl> for ServiceFqin {
    type Error = ServiceFqinValidationError;

    fn try_from(aidl: ServiceFqinAidl) -> Result<Self, ServiceFqinValidationError> {
        ServiceFqin::builder()
            .vm_name(aidl.sdvVmName)
            .package_name(aidl.sdvPackageName)
            .service_name(aidl.serviceBundleName)
            .instance_name(aidl.serviceInstanceName)
            .build()
    }
}

/// Converts ASDV_ServiceFqin to ServiceFqin struct.
impl TryFrom<&ASDV_ServiceFqin> for ServiceFqin {
    type Error = SdvStatus;

    fn try_from(asdv_fqin: &ASDV_ServiceFqin) -> SdvResult<ServiceFqin> {
        ServiceFqin::builder()
            .vm_name(ffi_str::bytes_to_str(&asdv_fqin.sdv_vm_name)?)
            .package_name(ffi_str::bytes_to_str(&asdv_fqin.sdv_package_name)?)
            .service_name(ffi_str::bytes_to_str(&asdv_fqin.service_bundle_name)?)
            .instance_name(ffi_str::bytes_to_str(&asdv_fqin.service_instance_name)?)
            .build()
            .map_err(derive!(SdvStatusCode::InvalidArgument))
    }
}

// TODO: Write tests for this.
impl From<ServiceFqin> for ServiceFqinAidl {
    fn from(value: ServiceFqin) -> Self {
        ServiceFqinAidl {
            sdvVmName: value.vm_name().to_string(),
            sdvPackageName: value.package_name().to_string(),
            serviceBundleName: value.service_name().to_string(),
            serviceInstanceName: value.instance_name().to_string(),
        }
    }
}

impl From<ServiceFqin> for String {
    fn from(val: ServiceFqin) -> Self {
        format!("{val}")
    }
}

impl fmt::Display for ServiceFqin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}.{}/{}",
            self.vm_name, self.package_name, self.service_name, self.instance_name
        )
    }
}

/// Service FQIN builder that is used to create a valid `ServiceFqin` instance.
pub struct ServiceFqinBuilder {
    vm_name: String,
    package_name: String,
    service_name: String,
    instance_name: String,
}

impl ServiceFqinBuilder {
    /// Sets VM Name.
    pub fn vm_name(mut self, vm_name: impl Into<String>) -> ServiceFqinBuilder {
        self.vm_name = vm_name.into();
        self
    }

    /// Sets Package Name.
    pub fn package_name(mut self, package_name: impl Into<String>) -> ServiceFqinBuilder {
        self.package_name = package_name.into();
        self
    }

    /// Sets Service Name.
    pub fn service_name(mut self, service_name: impl Into<String>) -> ServiceFqinBuilder {
        self.service_name = service_name.into();
        self
    }

    /// Sets Instance Name.
    pub fn instance_name(mut self, instance_name: impl Into<String>) -> ServiceFqinBuilder {
        self.instance_name = instance_name.into();
        self
    }

    /// Validates all parts of the FQIN and if successful returns `ServiceFqin`.
    pub fn build(self) -> Result<ServiceFqin, ServiceFqinValidationError> {
        let vm_name = VmName::from_str(&self.vm_name)?;
        let package_name = PackageName::from_str(&self.package_name)?;
        let service_name = ServiceName::from_str(&self.service_name)?;
        let instance_name = InstanceName::from_str(&self.instance_name)?;
        Ok(ServiceFqin { vm_name, package_name, service_name, instance_name })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use sdv_comms_id_validator::{AlphanumHyphenValidationError, PackageIdentifierValidationError};

    #[test]
    fn test_service_fqin_from_valid_str() {
        let result =
            ServiceFqin::from_str("vm-front-left:com.goog.sdv.WindowManager/main").unwrap();

        assert_eq!(result.vm_name().name(), "vm-front-left");
        assert_eq!(result.package_name().name(), "com.goog.sdv");
        assert_eq!(result.service_name().name(), "WindowManager");
        assert_eq!(result.instance_name().name(), "main");
    }

    #[test]
    fn test_service_fqin_from_empty_str() {
        let result = ServiceFqin::from_str("");
        assert_eq!(result, Err(ServiceFqinValidationError::IsEmpty));
    }

    #[test]
    fn test_service_fqin_from_str_exceeds_max_length() {
        // FQIN with maximum allowed length.
        let s = "vm-has-31-symbols-9012345678901:com.google.sdv.with_127_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234.ServiceWith_47_Symbols0123456789012345678901234/instance-has-47-symbols-90123456789012345678901";
        assert_eq!(s.len(), 255);
        assert!(ServiceFqin::from_str(s).is_ok());

        let result = ServiceFqin::from_str(&format!("{}{}", s, 'x'));
        assert_eq!(result, Err(ServiceFqinValidationError::ExceedsMaxLength(255, 256)));
    }

    #[test]
    fn test_service_fqin_from_str_with_invalid_colon_count() {
        let result = ServiceFqin::from_str("vm-front-leftcom.goog.sdv.WindowManager/main");
        assert_eq!(result, Err(ServiceFqinValidationError::InvalidDelimitersCount(':', 0)));

        let result = ServiceFqin::from_str("vm-front-left:com:goog.sdv.WindowManager/main");
        assert_eq!(result, Err(ServiceFqinValidationError::InvalidDelimitersCount(':', 2)));
    }

    #[test]
    fn test_service_fqin_from_str_with_invalid_dot_count() {
        let result = ServiceFqin::from_str("vm-front-left:comgoogsdvWindowManager/main");
        assert_eq!(result, Err(ServiceFqinValidationError::InvalidDotsCount(0)));

        let result = ServiceFqin::from_str("vm-front-left:comgoogsdv.WindowManager/main");
        assert_eq!(result, Err(ServiceFqinValidationError::InvalidDotsCount(1)));
    }

    #[test]
    fn test_service_fqin_from_str_with_invalid_slash_count() {
        let result = ServiceFqin::from_str("vm-front-left:com.goog.sdv.WindowManagermain");
        assert_eq!(result, Err(ServiceFqinValidationError::InvalidDelimitersCount('/', 0)));

        let result = ServiceFqin::from_str("vm-front-left:com.goog.sdv.WindowManager/mai/n");
        assert_eq!(result, Err(ServiceFqinValidationError::InvalidDelimitersCount('/', 2)));
    }

    #[test]
    fn test_service_fqin_from_str_with_invalid_colon_order() {
        // Move ':' from left to right.
        let result = ServiceFqin::from_str("vm-front-left.com:goog.sdv.WindowManager/main");
        assert_eq!(result, Err(ServiceFqinValidationError::InvalidDelimitersOrder('.', ':')));

        let result = ServiceFqin::from_str("vm-front-left.com.goog:sdv.WindowManager/main");
        assert_eq!(result, Err(ServiceFqinValidationError::InvalidDelimitersOrder('.', ':')));

        let result = ServiceFqin::from_str("vm-front-left.com.goog.sdv.WindowManager/ma:in");
        assert_eq!(result, Err(ServiceFqinValidationError::InvalidDelimitersOrder('.', ':')));

        // Now move '/' from right to left.
        let result = ServiceFqin::from_str("vm-front-left.com.goog/sdv.WindowManager.ma:in");
        assert_eq!(result, Err(ServiceFqinValidationError::InvalidDelimitersOrder('.', ':')));

        let result = ServiceFqin::from_str("vm-front-left/com.goog.sdv.WindowManager:main");
        assert_eq!(result, Err(ServiceFqinValidationError::InvalidDelimitersOrder('.', ':')));
    }

    #[test]
    fn test_service_fqin_from_str_with_invalid_slash_order() {
        // Move '/' from right to left.
        let result = ServiceFqin::from_str("vm-front-left:com.goog.sdv/WindowManager.main");
        assert_eq!(result, Err(ServiceFqinValidationError::InvalidDelimitersOrder('/', '.')));

        let result = ServiceFqin::from_str("vm-front-left:com.goog/sdv.WindowManager.main");
        assert_eq!(result, Err(ServiceFqinValidationError::InvalidDelimitersOrder('/', '.')));

        let result = ServiceFqin::from_str("vm-front-left/com:goog.sdv.WindowManager.main");
        assert_eq!(result, Err(ServiceFqinValidationError::InvalidDelimitersOrder('/', '.')));
    }

    #[test]
    fn test_service_fqin_formatting() {
        let s = "vm-front-left:com.goog.sdv.WindowManager/main";
        let result = ServiceFqin::from_str(s).unwrap();
        assert_eq!(format!("{result}"), s);
    }

    #[test]
    fn test_valid_service_fqin() {
        let result = ServiceFqin::builder()
            .vm_name("vm-front-left")
            .package_name("com.goog.sdv")
            .service_name("WindowManager")
            .instance_name("main")
            .build()
            .unwrap();

        assert_eq!(result.vm_name().name(), "vm-front-left");
        assert_eq!(result.package_name().name(), "com.goog.sdv");
        assert_eq!(result.service_name().name(), "WindowManager");
        assert_eq!(result.instance_name().name(), "main");
    }

    #[test]
    fn test_service_fqin_with_missing_vm_name() {
        let result = ServiceFqin::builder()
            .package_name("com.goog.sdv")
            .service_name("WindowManager")
            .instance_name("main")
            .build();
        assert_eq!(
            result,
            Err(ServiceFqinValidationError::InvalidVmName(VmNameValidationError(
                AlphanumHyphenValidationError::IsEmpty
            )))
        );
    }

    #[test]
    fn test_service_fqin_with_missing_package_name() {
        let result = ServiceFqin::builder()
            .vm_name("vm-front-left")
            .service_name("WindowManager")
            .instance_name("main")
            .build();
        assert_eq!(
            result,
            Err(ServiceFqinValidationError::InvalidPackageName(PackageNameValidationError(
                PackageIdentifierValidationError::IsEmpty
            )))
        );
    }

    #[test]
    fn test_service_fqin_with_missing_service_name() {
        let result = ServiceFqin::builder()
            .vm_name("vm-front-left")
            .package_name("com.goog.sdv")
            .instance_name("main")
            .build();
        assert_eq!(
            result,
            Err(ServiceFqinValidationError::InvalidServiceName(
                ServiceNameValidationError::IsEmpty
            ))
        );
    }

    #[test]
    fn test_service_fqin_with_missing_instance_name() {
        let result = ServiceFqin::builder()
            .vm_name("vm-front-left")
            .package_name("com.goog.sdv")
            .service_name("WindowManager")
            .build();
        assert_eq!(
            result,
            Err(ServiceFqinValidationError::InvalidInstanceName(InstanceNameValidationError(
                AlphanumHyphenValidationError::IsEmpty
            )))
        );
    }

    #[test]
    fn test_service_fqin_with_invalid_vm_name() {
        let result = ServiceFqin::builder()
            .vm_name("vm--front--left")
            .package_name("com.goog.sdv")
            .service_name("WindowManager")
            .instance_name("main")
            .build();
        assert_eq!(
            result,
            Err(ServiceFqinValidationError::InvalidVmName(VmNameValidationError(
                AlphanumHyphenValidationError::ContainsConsecutiveHyphens
            )))
        );
    }

    #[test]
    fn test_service_fqin_with_invalid_package_name() {
        let result = ServiceFqin::builder()
            .vm_name("vm-front-left")
            .package_name("comgoogsdv")
            .service_name("WindowManager")
            .instance_name("main")
            .build();
        assert_eq!(
            result,
            Err(ServiceFqinValidationError::InvalidPackageName(PackageNameValidationError(
                PackageIdentifierValidationError::HasOneSegment
            )))
        );
    }

    #[test]
    fn test_service_fqin_with_invalid_service_name() {
        let result = ServiceFqin::builder()
            .vm_name("vm-front-left")
            .package_name("com.goog.sdv")
            .service_name("Window__Manager")
            .instance_name("main")
            .build();
        assert_eq!(
            result,
            Err(ServiceFqinValidationError::InvalidServiceName(
                ServiceNameValidationError::ContainsConsecutiveUnderscores
            ))
        );
    }

    #[test]
    fn test_service_fqin_with_invalid_instance_name() {
        let result = ServiceFqin::builder()
            .vm_name("vm-front-left")
            .package_name("com.goog.sdv")
            .service_name("WindowManager")
            .instance_name("0main")
            .build();
        assert_eq!(
            result,
            Err(ServiceFqinValidationError::InvalidInstanceName(InstanceNameValidationError(
                AlphanumHyphenValidationError::StartsWithInvalidCharacter('0')
            )))
        );
    }

    #[test]
    fn test_service_fqin_try_from_aidl_success() {
        let aidl = ServiceFqinAidl {
            sdvVmName: "vm-test".to_string(),
            sdvPackageName: "com.google.sdv".to_string(),
            serviceBundleName: "TestBundleName".to_string(),
            serviceInstanceName: "default".to_string(),
        };

        let fqin = ServiceFqin::try_from(aidl);

        assert_eq!(
            fqin,
            Ok(ServiceFqin::builder()
                .vm_name("vm-test")
                .package_name("com.google.sdv")
                .service_name("TestBundleName")
                .instance_name("default")
                .build()
                .unwrap())
        );
    }

    #[test]
    fn test_service_fqin_try_from_aidl_failed_with_missing_vm_name() {
        let aidl = ServiceFqinAidl {
            sdvVmName: "".to_string(),
            sdvPackageName: "com.goog.sdv".to_string(),
            serviceBundleName: "WindowManager".to_string(),
            serviceInstanceName: "main".to_string(),
        };

        let fqin = ServiceFqin::try_from(aidl);

        assert_eq!(
            fqin,
            Err(ServiceFqinValidationError::InvalidVmName(VmNameValidationError(
                AlphanumHyphenValidationError::IsEmpty
            )))
        );
    }

    #[test]
    fn test_service_fqin_try_from_aidl_failed_with_missing_package_name() {
        let aidl = ServiceFqinAidl {
            sdvVmName: "vm-front-left".to_string(),
            sdvPackageName: "".to_string(),
            serviceBundleName: "WindowManager".to_string(),
            serviceInstanceName: "main".to_string(),
        };

        let fqin = ServiceFqin::try_from(aidl);

        assert_eq!(
            fqin,
            Err(ServiceFqinValidationError::InvalidPackageName(PackageNameValidationError(
                PackageIdentifierValidationError::IsEmpty
            )))
        );
    }

    #[test]
    fn test_service_fqin_try_from_aidl_failed_with_missing_service_name() {
        let aidl = ServiceFqinAidl {
            sdvVmName: "vm-front-left".to_string(),
            sdvPackageName: "com.goog.sdv".to_string(),
            serviceBundleName: "".to_string(),
            serviceInstanceName: "main".to_string(),
        };

        let fqin = ServiceFqin::try_from(aidl);

        assert_eq!(
            fqin,
            Err(ServiceFqinValidationError::InvalidServiceName(
                ServiceNameValidationError::IsEmpty
            ))
        );
    }

    #[test]
    fn test_service_fqin_try_from_aidl_failed_with_missing_instance_name() {
        let aidl = ServiceFqinAidl {
            sdvVmName: "vm-front-left".to_string(),
            sdvPackageName: "com.goog.sdv".to_string(),
            serviceBundleName: "WindowManager".to_string(),
            serviceInstanceName: "".to_string(),
        };

        let fqin = ServiceFqin::try_from(aidl);

        assert_eq!(
            fqin,
            Err(ServiceFqinValidationError::InvalidInstanceName(InstanceNameValidationError(
                AlphanumHyphenValidationError::IsEmpty
            )))
        );
    }

    #[test]
    fn test_service_fqin_try_from_aidl_failed_with_invalid_vm_name() {
        let aidl = ServiceFqinAidl {
            sdvVmName: "vm--front--left".to_string(),
            sdvPackageName: "com.goog.sdv".to_string(),
            serviceBundleName: "WindowManager".to_string(),
            serviceInstanceName: "main".to_string(),
        };

        let fqin = ServiceFqin::try_from(aidl);

        assert_eq!(
            fqin,
            Err(ServiceFqinValidationError::InvalidVmName(VmNameValidationError(
                AlphanumHyphenValidationError::ContainsConsecutiveHyphens
            )))
        );
    }

    #[test]
    fn test_service_fqin_try_from_aidl_failed_with_invalid_package_name() {
        let aidl = ServiceFqinAidl {
            sdvVmName: "vm-front-left".to_string(),
            sdvPackageName: "comgoogsdv".to_string(),
            serviceBundleName: "WindowManager".to_string(),
            serviceInstanceName: "main".to_string(),
        };

        let fqin = ServiceFqin::try_from(aidl);

        assert_eq!(
            fqin,
            Err(ServiceFqinValidationError::InvalidPackageName(PackageNameValidationError(
                PackageIdentifierValidationError::HasOneSegment
            )))
        );
    }

    #[test]
    fn test_service_fqin_try_from_aidl_failed_with_invalid_service_name() {
        let aidl = ServiceFqinAidl {
            sdvVmName: "vm-front-left".to_string(),
            sdvPackageName: "com.goog.sdv".to_string(),
            serviceBundleName: "Window__Manager".to_string(),
            serviceInstanceName: "main".to_string(),
        };

        let fqin = ServiceFqin::try_from(aidl);

        assert_eq!(
            fqin,
            Err(ServiceFqinValidationError::InvalidServiceName(
                ServiceNameValidationError::ContainsConsecutiveUnderscores
            ))
        );
    }

    #[test]
    fn test_service_fqin_try_from_aidl_failed_with_invalid_instance_name() {
        let aidl = ServiceFqinAidl {
            sdvVmName: "vm-front-left".to_string(),
            sdvPackageName: "com.goog.sdv".to_string(),
            serviceBundleName: "WindowManager".to_string(),
            serviceInstanceName: "0main".to_string(),
        };

        let fqin = ServiceFqin::try_from(aidl);

        assert_eq!(
            fqin,
            Err(ServiceFqinValidationError::InvalidInstanceName(InstanceNameValidationError(
                AlphanumHyphenValidationError::StartsWithInvalidCharacter('0')
            )))
        );
    }

    #[test]
    fn test_service_fqin_into_aidl_success() {
        let fqin = ServiceFqin::builder()
            .vm_name("vm-test")
            .package_name("com.google.sdv")
            .service_name("TestBundleName")
            .instance_name("default")
            .build()
            .unwrap();

        let aidl: ServiceFqinAidl = fqin.into();

        assert_eq!(
            aidl,
            ServiceFqinAidl {
                sdvVmName: "vm-test".to_string(),
                sdvPackageName: "com.google.sdv".to_string(),
                serviceBundleName: "TestBundleName".to_string(),
                serviceInstanceName: "default".to_string(),
            }
        );
    }

    #[test]
    fn try_from_str() {
        assert_eq!(
            ServiceFqin::try_from("vm-front-left:com.goog.sdv.WindowManager/main").unwrap(),
            ServiceFqin::builder()
                .vm_name("vm-front-left")
                .package_name("com.goog.sdv")
                .service_name("WindowManager")
                .instance_name("main")
                .build()
                .unwrap()
        );
    }

    #[test]
    fn into_string() {
        let service_fqin = ServiceFqin::builder()
            .vm_name("vm-front-left")
            .package_name("com.goog.sdv")
            .service_name("WindowManager")
            .instance_name("main")
            .build()
            .unwrap();

        let string: String = service_fqin.into();

        assert_eq!(string, "vm-front-left:com.goog.sdv.WindowManager/main");
    }
}
