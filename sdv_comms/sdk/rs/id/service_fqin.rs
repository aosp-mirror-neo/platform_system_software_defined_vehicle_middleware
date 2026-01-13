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
use sdv_comms_bindgen::sdv_status_t;
use sdv_comms_bindgen::ASDV_ServiceFqin;
use sdv_comms_bindgen::{
    ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
    ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
    ASDV_SERVICE_FQIN_MAX_SIZE_BYTES_INCLUDING_NUL,
    ASDV_SERVICE_INSTANCE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
    ASDV_VM_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
};
use sdv_comms_common::ffi_str::{bytes_to_str, str_to_bytes};
use sdv_status_rs::{SdvStatus, SdvStatusCode};
use std::{
    cmp, fmt,
    hash::{Hash, Hasher},
    str,
};

/// Describes the Service Fully Qualified Name.
///
/// This name comprises the following components:
///  1) SDV VM Name: The SDV VM Instance on which this Service Bundle is executing.
///  2) SDV Package Name: The Package Name within which the Service Bundle was shipped.
///  3) Service Bundle Name: The Service Bundle Name as declared in the Package Manifest.
///  4) Service Instance Name: The Service Instance Name as assigned by the Orchestrator
///     when the Service Bundle was loaded.
#[repr(transparent)]
#[derive(Clone, Debug)]
pub struct ServiceFqin(ASDV_ServiceFqin);

// SAFETY: [ServiceFqin] is a transparent wrapper around ASDV_ServiceFqin, and meets safety
// requirements for [TransparentWrapper].
unsafe impl TransparentWrapper<ASDV_ServiceFqin> for ServiceFqin {}

impl ServiceFqin {
    /// Returns builder that should be used to create `ServiceFqin` instance.
    pub fn builder() -> ServiceFqinBuilder {
        ServiceFqinBuilder {
            sdv_vm_name: String::new(),
            sdv_package_name: String::new(),
            service_bundle_name: String::new(),
            service_instance_name: String::new(),
        }
    }

    /// Returns a str for the SDV VM Name of this Service FQIN.
    pub fn get_sdv_vm_name(&self) -> &str {
        bytes_to_str(&self.0.sdv_vm_name).expect("Invalid vm name")
    }

    /// Returns a str for the SDV Package Name of this Service FQIN.
    pub fn get_sdv_package_name(&self) -> &str {
        bytes_to_str(&self.0.sdv_package_name).expect("Invalid package name")
    }

    /// Returns a str for the Service Bundle Name of this Service FQIN.
    pub fn get_service_bundle_name(&self) -> &str {
        bytes_to_str(&self.0.service_bundle_name).expect("Invalid service bundle name")
    }

    /// Returns a str for the Service Instance Name of this Service FQIN.
    pub fn get_service_instance_name(&self) -> &str {
        bytes_to_str(&self.0.service_instance_name).expect("Invalid instance name")
    }
}

impl str::FromStr for ServiceFqin {
    type Err = SdvStatus;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut fqin_bytes = [0; ASDV_SERVICE_FQIN_MAX_SIZE_BYTES_INCLUDING_NUL as usize];

        str_to_bytes(s, &mut fqin_bytes)?;

        let mut service_fqin = ASDV_ServiceFqin {
            sdv_vm_name: [0; ASDV_VM_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as usize],
            sdv_package_name: [0; ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as usize],
            service_bundle_name: [0; ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
                as usize],
            service_instance_name: [0; ASDV_SERVICE_INSTANCE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
                as usize],
        };

        // SAFETY: both `fqin_bytes` and `service_fqin` are valid pointers and live for the duration
        // of the call. `fqin_bytes` is a pointer to a NUL-terminated ASCII encoded sequence of bytes.
        let status: sdv_status_t = unsafe {
            sdv_comms_bindgen::ASDV_ServiceFqin_fromString(fqin_bytes.as_ptr(), &mut service_fqin)
        };

        match SdvStatusCode::from(status) {
            SdvStatusCode::Ok => Ok(ServiceFqin(service_fqin)),
            status_code => Err(SdvStatus::new(status_code)),
        }
    }
}

impl fmt::Display for ServiceFqin {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut buf = [0; ASDV_SERVICE_FQIN_MAX_SIZE_BYTES_INCLUDING_NUL as usize];
        let mut buf_size: u32 = buf.len() as u32;

        // SAFETY: `ServiceFqin`, `buf` and `buf_size` are valid pointers that live for
        // the duration of the call.
        let status: sdv_status_t = unsafe {
            sdv_comms_bindgen::ASDV_ServiceFqin_toString(
                &self.0,
                buf.as_mut_ptr().cast(),
                &mut buf_size,
            )
        };

        let status_code = SdvStatusCode::from(status);
        status_code.expect("ServiceFqin::to_string() failed");

        let str = bytes_to_str(buf.as_slice()).expect("Invalid ServiceFqin");

        write!(f, "{str}")
    }
}

impl cmp::PartialEq for ServiceFqin {
    fn eq(&self, other: &ServiceFqin) -> bool {
        let mut are_equal = false;

        // SAFETY: `self` and `other` hold valid ASDV_ServiceFqin instances with nul-terminated
        // fields, `are_equal` points to a boolean. All parameters outlive the function call.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServiceFqin_equals(&self.0, &other.0, &mut are_equal)
        };
        let status_code = SdvStatusCode::from(status);
        status_code.expect("ServiceFqin::eq() failed");
        are_equal
    }
}

// Implementing `Eq` is safe because `PartialEq` for `ServiceFqin` already guarantees:
//
// * Reflexivity: `a == a` is true.
// * Symmetry: `a == b` implies `b == a`.
// * Transitivity: `a == b` and `b == c` implies `a == c` .
impl cmp::Eq for ServiceFqin {}

impl Hash for ServiceFqin {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get_sdv_vm_name().hash(state);
        self.get_sdv_package_name().hash(state);
        self.get_service_bundle_name().hash(state);
        self.get_service_instance_name().hash(state);
    }
}

/// Service FQIN builder that is used to create a valid `ServiceFqin` instance.
pub struct ServiceFqinBuilder {
    sdv_vm_name: String,
    sdv_package_name: String,
    service_bundle_name: String,
    service_instance_name: String,
}

impl ServiceFqinBuilder {
    /// Sets SDV VM Name.
    pub fn sdv_vm_name(mut self, sdv_vm_name: impl Into<String>) -> ServiceFqinBuilder {
        self.sdv_vm_name = sdv_vm_name.into();
        self
    }

    /// Sets SDV Package Name.
    pub fn sdv_package_name(mut self, sdv_package_name: impl Into<String>) -> ServiceFqinBuilder {
        self.sdv_package_name = sdv_package_name.into();
        self
    }

    /// Sets Service Bundle Name.
    pub fn service_bundle_name(
        mut self,
        service_bundle_name: impl Into<String>,
    ) -> ServiceFqinBuilder {
        self.service_bundle_name = service_bundle_name.into();
        self
    }

    /// Sets Service Instance Name.
    pub fn service_instance_name(
        mut self,
        service_instance_name: impl Into<String>,
    ) -> ServiceFqinBuilder {
        self.service_instance_name = service_instance_name.into();
        self
    }

    /// Validates all parts of the FQIN and if successful returns `ServiceFqin`.
    pub fn build(self) -> Result<ServiceFqin, SdvStatus> {
        let mut service_fqin = ASDV_ServiceFqin {
            sdv_vm_name: [0; ASDV_VM_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as usize],
            sdv_package_name: [0; ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as usize],
            service_bundle_name: [0; ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
                as usize],
            service_instance_name: [0; ASDV_SERVICE_INSTANCE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
                as usize],
        };

        str_to_bytes(self.sdv_vm_name.as_str(), &mut service_fqin.sdv_vm_name)?;
        str_to_bytes(self.sdv_package_name.as_str(), &mut service_fqin.sdv_package_name)?;
        str_to_bytes(self.service_bundle_name.as_str(), &mut service_fqin.service_bundle_name)?;
        str_to_bytes(self.service_instance_name.as_str(), &mut service_fqin.service_instance_name)?;

        // SAFETY: The passed ASDV_ServiceFqin pointer is valid for the duration of the call.
        let status: sdv_status_t =
            unsafe { sdv_comms_bindgen::ASDV_ServiceFqin_validate(&service_fqin) };

        match SdvStatusCode::from(status) {
            SdvStatusCode::Ok => Ok(ServiceFqin(service_fqin)),
            status_code => Err(SdvStatus::new(status_code)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr;

    #[test]
    fn test_service_fqin_from_str_success() {
        let result = ServiceFqin::from_str("vm:com.google.sdv.WindowManager/sample");
        let fqin = result.unwrap();

        assert_eq!(fqin.get_sdv_vm_name(), "vm");
        assert_eq!(fqin.get_sdv_package_name(), "com.google.sdv");
        assert_eq!(fqin.get_service_bundle_name(), "WindowManager");
        assert_eq!(fqin.get_service_instance_name(), "sample");
    }

    #[test]
    fn test_service_fqin_from_str_longest_fqin() {
        // FQIN with maximum allowed length.
        let longest_fqin = "vm-has-31-symbols-9012345678901:com.google.sdv.with_127_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234.ServiceWith_47_Symbols0123456789012345678901234/instance-has-47-symbols-90123456789012345678901";

        // Take NUL terminator into account and make sure that `longest_fqin` length is maximum allowed.
        assert_eq!(longest_fqin.len(), ASDV_SERVICE_FQIN_MAX_SIZE_BYTES_INCLUDING_NUL as usize - 1);

        let result = ServiceFqin::from_str(longest_fqin);
        let fqin = result.unwrap();
        assert_eq!(fqin.get_sdv_vm_name(), "vm-has-31-symbols-9012345678901");
        assert_eq!(fqin.get_sdv_package_name(), "com.google.sdv.with_127_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234");
        assert_eq!(
            fqin.get_service_bundle_name(),
            "ServiceWith_47_Symbols0123456789012345678901234"
        );
        assert_eq!(
            fqin.get_service_instance_name(),
            "instance-has-47-symbols-90123456789012345678901"
        );

        // Add `x` to the end of `longest_fqin` thus it should exceed the maximum allowed length.
        let result = ServiceFqin::from_str(&format!("{}{}", longest_fqin, 'x'));
        let status = result.unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_service_fqin_from_str_empty_string() {
        let result = ServiceFqin::from_str("");
        let status = result.unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_service_fqin_from_str_invalid_fqin() {
        let result = ServiceFqin::from_str("vm:com.google.sdv.777WindowManager/sample");
        let status = result.unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_service_fqin_from_str_non_ascii() {
        let result = ServiceFqin::from_str("vm:cöm.göögle.sdv.WindöwMänäger/sämple");
        let status = result.unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_service_fqin_from_str_nul_in_the_middle() {
        let result = ServiceFqin::from_str("vm:com.google.sdv.Window\0Manager/sample");
        let status = result.unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::InvalidArgument);
    }

    #[test]
    fn test_service_fqin_to_string_success() {
        let fqin_string = "vm:com.google.sdv.WindowManager/sample";

        let result = ServiceFqin::from_str(fqin_string);
        let fqin = result.unwrap();

        assert_eq!(fqin.to_string(), fqin_string);
    }

    #[test]
    fn test_service_fqin_to_string_longest_fqin() {
        // FQIN with maximum allowed length.
        let longest_fqin = "vm-has-31-symbols-9012345678901:com.google.sdv.with_127_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234.ServiceWith_47_Symbols0123456789012345678901234/instance-has-47-symbols-90123456789012345678901";

        // Take NUL terminator into account and make sure that `longest_fqin` length is maximum allowed.
        assert_eq!(longest_fqin.len(), ASDV_SERVICE_FQIN_MAX_SIZE_BYTES_INCLUDING_NUL as usize - 1);

        let result = ServiceFqin::from_str(longest_fqin);
        let fqin = result.unwrap();

        assert_eq!(fqin.to_string(), longest_fqin);
    }

    #[test]
    fn test_service_fqin_equals_true() {
        let fqin_lhs = ServiceFqin::from_str("vm:com.google.sdv.WindowManager/sample1").unwrap();
        let fqin_rhs = ServiceFqin::from_str("vm:com.google.sdv.WindowManager/sample1").unwrap();
        assert_eq!(fqin_lhs, fqin_rhs);
    }

    #[test]
    fn test_service_fqin_equals_false() {
        let fqin_lhs = ServiceFqin::from_str("vm:com.google.sdv.WindowManager/sample1").unwrap();
        let fqin_rhs = ServiceFqin::from_str("vm:com.google.sdv.WindowManager/sample2").unwrap();
        assert_ne!(fqin_lhs, fqin_rhs);
    }

    #[test]
    fn test_service_fqin_builder_success() {
        let fqin = ServiceFqin::builder()
            .sdv_vm_name("vm")
            .sdv_package_name("com.google.sdv")
            .service_bundle_name("WindowManager")
            .service_instance_name("sample")
            .build()
            .unwrap();

        assert_eq!(fqin.get_sdv_vm_name(), "vm");
        assert_eq!(fqin.get_sdv_package_name(), "com.google.sdv");
        assert_eq!(fqin.get_service_bundle_name(), "WindowManager");
        assert_eq!(fqin.get_service_instance_name(), "sample");
    }

    #[test]
    fn test_service_fqin_builder_longest_fqin() {
        let fqin = ServiceFqin::builder()
            .sdv_vm_name("vm-has-31-symbols-9012345678901")
            .sdv_package_name("com.google.sdv.with_127_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234")
            .service_bundle_name("ServiceWith_47_Symbols0123456789012345678901234")
            .service_instance_name("instance-has-47-symbols-90123456789012345678901")
            .build()
            .unwrap();

        assert_eq!(fqin.get_sdv_vm_name(), "vm-has-31-symbols-9012345678901");
        assert_eq!(fqin.get_sdv_package_name(), "com.google.sdv.with_127_length_x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234");
        assert_eq!(
            fqin.get_service_bundle_name(),
            "ServiceWith_47_Symbols0123456789012345678901234"
        );
        assert_eq!(
            fqin.get_service_instance_name(),
            "instance-has-47-symbols-90123456789012345678901"
        );
    }

    #[test]
    fn test_service_fqin_builder_invalid_sdv_vm_name() {
        for sdv_vm_name in ["", "vm-non-ascїї", "vm-has-32-symbols-90123456789012"] {
            let err = ServiceFqin::builder()
                .sdv_vm_name(sdv_vm_name)
                .sdv_package_name("com.google.sdv")
                .service_bundle_name("WindowManager")
                .service_instance_name("sample")
                .build()
                .unwrap_err();
            assert_eq!(
                err.status_code(),
                SdvStatusCode::InvalidArgument,
                "Unexpected result for SDV VM Name: {sdv_vm_name:?}"
            );
        }
    }

    #[test]
    fn test_service_fqin_builder_invalid_sdv_package_name() {
        for sdv_package_name in [
            "",
            "non.ascїї.sdv",
            "com.google.sdv.with_128_length_x012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345"
        ] {
            let err = ServiceFqin::builder()
                .sdv_vm_name("vm")
                .sdv_package_name(sdv_package_name)
                .service_bundle_name("WindowManager")
                .service_instance_name("sample")
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
    fn test_service_fqin_builder_invalid_service_bundle_name() {
        for service_bundle_name in
            ["", "NonAscїїServїceName", "ServiceWith_48_Symbols01234567890123456789012345"]
        {
            let err = ServiceFqin::builder()
                .sdv_vm_name("vm")
                .sdv_package_name("com.google.sdv")
                .service_bundle_name(service_bundle_name)
                .service_instance_name("sample")
                .build()
                .unwrap_err();
            assert_eq!(
                err.status_code(),
                SdvStatusCode::InvalidArgument,
                "Unexpected result for Service Bundle Name: {service_bundle_name:?}"
            );
        }
    }

    #[test]
    fn test_service_fqin_builder_invalid_service_instance_name() {
        for service_instance_name in
            ["", "non-ascїї-instance-name", "instance-has-48-symbols-901234567890123456789012"]
        {
            let err = ServiceFqin::builder()
                .sdv_vm_name("vm")
                .sdv_package_name("com.google.sdv")
                .service_bundle_name("WindowManager")
                .service_instance_name(service_instance_name)
                .build()
                .unwrap_err();
            assert_eq!(
                err.status_code(),
                SdvStatusCode::InvalidArgument,
                "Unexpected result for Service Instance Name: {service_instance_name:?}"
            );
        }
    }
}
