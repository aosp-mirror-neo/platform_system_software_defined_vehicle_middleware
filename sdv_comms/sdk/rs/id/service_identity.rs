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

use crate::id::ServiceFqin;
use bytemuck::TransparentWrapper;
use sdv_comms_bindgen::ASDV_ServiceIdentity;
use sdv_comms_bindgen::ASDV_ServicePublicKey;
use sdv_comms_bindgen::ASDV_UnitIdentifier;
use sdv_comms_bindgen::ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL;
use sdv_comms_common::ffi_str::bytes_to_str;
use sdv_status_rs::SdvStatusCode;

/// Defines the Service Identity of a Service Bundle.
/// The Identity of a Service Bundle consists of two components:
///  1) SID (Service Identifier): A vehicle wide unique 64 bit number assigned to the running
///     instance of the Service Bundle.
///  2) FQIN (Fully Qualified Instance Name): The full human readable name of the running instance
///     of the Service Bundle.
#[repr(transparent)]
#[derive(Debug)]
pub struct ServiceIdentity(ASDV_ServiceIdentity);

// SAFETY: [ServiceIdentity] is a transparent wrapper around ASDV_ServiceIdentity.
unsafe impl TransparentWrapper<ASDV_ServiceIdentity> for ServiceIdentity {}

impl ServiceIdentity {
    /// Returns the globally unique 64 bit identifier of this ServiceIdentity.
    pub fn get_sid(&self) -> u64 {
        self.0.sid
    }

    /// Returns the Fully Qualified Instance Name of the Service Bundle identified by
    /// this ServiceIdentity.
    pub fn get_fqin(&self) -> &ServiceFqin {
        ServiceFqin::wrap_ref(&self.0.fqin)
    }

    // TODO: b/400635525 - Remove this once API breaks are possible.
    /// Unused
    pub fn get_public_key(&self) -> &ServicePublicKey {
        ServicePublicKey::wrap_ref(&self.0.public_key)
    }
}

impl PartialEq for ServiceIdentity {
    fn eq(&self, other: &Self) -> bool {
        let mut are_equal = false;

        // SAFETY: `self` and `other` hold valid ASDV_ServiceIdentity instances with valid
        // `ASDV_ServiceIdentity` fields, `are_equal` points to a boolean. All parameters outlive
        // the function call.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServiceIdentity_equals(&self.0, &other.0, &mut are_equal)
        };
        let status_code = SdvStatusCode::from(status);
        status_code.expect("ServiceIdentity::eq() failed");

        are_equal
    }
}

#[allow(dead_code)]
pub struct ServicePublicKey(ASDV_ServicePublicKey);

// SAFETY: [ServiceIdentity] is a transparent wrapper around public key.
unsafe impl TransparentWrapper<ASDV_ServicePublicKey> for ServicePublicKey {}

impl PartialEq for ServicePublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// Defines an identifier used to uniquely identify a Service Unit.
/// The Unit Identifier consists of the following components:
///  1) Service Identity
///  2) Unit Name: The Service Bundle specific name of the individual Service Unit being identified
#[repr(transparent)]
#[derive(Debug, Clone, PartialEq)]
pub struct UnitIdentifier(pub(crate) ASDV_UnitIdentifier);

// SAFETY: [UnitIdentifier] is a transparent wrapper around ASDV_UnitIdentifier.
unsafe impl TransparentWrapper<ASDV_UnitIdentifier> for UnitIdentifier {}

impl UnitIdentifier {
    /// Creates a new UnitIdentitier instance for the provided ServiceIdentity and unit name.
    pub fn new(identity: ServiceIdentity, service_unit_name: impl Into<String>) -> Self {
        let service_unit_name = service_unit_name.into();
        let mut asdv_unit_identifier = ASDV_UnitIdentifier {
            service_identity: identity.0,
            unit_name: [0; ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as usize],
        };
        asdv_unit_identifier.unit_name[..service_unit_name.len()]
            .copy_from_slice(service_unit_name.as_bytes());
        UnitIdentifier(asdv_unit_identifier)
    }

    /// Returns the globally unique 64 bit identifier of this UnitIdentifier.
    pub fn get_sid(&self) -> u64 {
        self.0.service_identity.sid
    }

    /// Returns the Fully Qualified Instance Name of this UnitIdentifier.
    pub fn get_fqin(&self) -> &ServiceFqin {
        ServiceFqin::wrap_ref(&self.0.service_identity.fqin)
    }

    /// Returns the service unit name of this UnitIdentifier.
    pub fn get_unit_name(&self) -> &str {
        bytes_to_str(&self.0.unit_name).expect("Invalid unit name")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdv_comms_bindgen::ASDV_ServiceFqin;
    use sdv_comms_bindgen::{
        ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
        ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
        ASDV_SERVICE_INSTANCE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
        ASDV_SERVICE_PUBLIC_KEY_SIZE_BYTES, ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
        ASDV_VM_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
    };

    const SID: u64 = 42;
    const UNIT_NAME: &str = "unit_name";
    const VM_NAME: &str = "vm";
    const PACKAGE_NAME: &str = "com.google.sdv";
    const SERVICE_BUNDLE_NAME: &str = "WindowManager";
    const INSTANCE_NAME: &str = "sample";

    fn new_service_identity() -> ASDV_ServiceIdentity {
        ASDV_ServiceIdentity {
            sid: SID,
            fqin: new_service_fqin_for_test(),
            public_key: new_public_key_for_test(),
        }
    }

    fn new_service_fqin_for_test() -> ASDV_ServiceFqin {
        let mut asdv_service_fqin = ASDV_ServiceFqin {
            sdv_vm_name: [0; ASDV_VM_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as usize],
            sdv_package_name: [0; ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as usize],
            service_bundle_name: [0; ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
                as usize],
            service_instance_name: [0; ASDV_SERVICE_INSTANCE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL
                as usize],
        };
        let vm_name = VM_NAME;
        asdv_service_fqin.sdv_vm_name[..vm_name.len()].copy_from_slice(vm_name.as_bytes());
        let package_name = PACKAGE_NAME;
        asdv_service_fqin.sdv_package_name[..package_name.len()]
            .copy_from_slice(package_name.as_bytes());
        let service_name = SERVICE_BUNDLE_NAME;
        asdv_service_fqin.service_bundle_name[..service_name.len()]
            .copy_from_slice(service_name.as_bytes());
        let instance_name = INSTANCE_NAME;
        asdv_service_fqin.service_instance_name[..instance_name.len()]
            .copy_from_slice(instance_name.as_bytes());
        asdv_service_fqin
    }

    fn new_public_key_for_test() -> ASDV_ServicePublicKey {
        [0; ASDV_SERVICE_PUBLIC_KEY_SIZE_BYTES as usize]
    }

    #[test]
    fn test_unit_identifier_new() {
        let service_identity = ServiceIdentity(new_service_identity());
        let unit_identifier = UnitIdentifier::new(service_identity, UNIT_NAME);

        assert_eq!(SID, unit_identifier.get_sid());
        assert_eq!(UNIT_NAME, unit_identifier.get_unit_name());
        let fqin = unit_identifier.get_fqin();
        assert_eq!(VM_NAME, fqin.get_sdv_vm_name());
        assert_eq!(PACKAGE_NAME, fqin.get_sdv_package_name());
        assert_eq!(SERVICE_BUNDLE_NAME, fqin.get_service_bundle_name());
        assert_eq!(INSTANCE_NAME, fqin.get_service_instance_name());
    }

    #[test]
    fn test_unit_identifier_get_sid() {
        let mut asdv_unit_identifier = ASDV_UnitIdentifier {
            service_identity: new_service_identity(),
            unit_name: [0; ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as usize],
        };
        asdv_unit_identifier.unit_name[..UNIT_NAME.len()].copy_from_slice(UNIT_NAME.as_bytes());
        let unit_identifier = UnitIdentifier(asdv_unit_identifier);
        assert_eq!(SID, unit_identifier.get_sid());
    }

    #[test]
    fn test_unit_identifier_get_unit_name() {
        let mut asdv_unit_identifier = ASDV_UnitIdentifier::default();
        asdv_unit_identifier.unit_name[..UNIT_NAME.len()].copy_from_slice(UNIT_NAME.as_bytes());
        let unit_identifier = UnitIdentifier(asdv_unit_identifier);
        assert_eq!(UNIT_NAME, unit_identifier.get_unit_name());
    }

    #[test]
    fn test_service_identity_equals_true() {
        let service_identity_lhs = ServiceIdentity(ASDV_ServiceIdentity {
            sid: 0,
            fqin: new_service_fqin_for_test(),
            public_key: new_public_key_for_test(),
        });
        let service_identity_rhs = ServiceIdentity(ASDV_ServiceIdentity {
            sid: 0,
            fqin: new_service_fqin_for_test(),
            public_key: new_public_key_for_test(),
        });
        assert_eq!(service_identity_lhs, service_identity_rhs);
    }

    #[test]
    fn test_service_identity_equals_false_sid() {
        let service_identity_lhs = ServiceIdentity(ASDV_ServiceIdentity {
            sid: 0,
            fqin: new_service_fqin_for_test(),
            public_key: new_public_key_for_test(),
        });
        let service_identity_rhs = ServiceIdentity(ASDV_ServiceIdentity {
            sid: 1,
            fqin: new_service_fqin_for_test(),
            public_key: new_public_key_for_test(),
        });
        assert_ne!(service_identity_lhs, service_identity_rhs);
    }

    #[test]
    fn test_service_identity_equals_false_fqin() {
        let service_identity_lhs = ServiceIdentity(ASDV_ServiceIdentity {
            sid: 0,
            fqin: new_service_fqin_for_test(),
            public_key: new_public_key_for_test(),
        });
        let mut service_identity_rhs = ServiceIdentity(ASDV_ServiceIdentity {
            sid: 0,
            fqin: new_service_fqin_for_test(),
            public_key: new_public_key_for_test(),
        });
        service_identity_rhs.0.fqin.service_instance_name[1] = b'i';
        assert_ne!(service_identity_lhs, service_identity_rhs);
    }

    #[test]
    fn test_service_identity_equals_false_public_key() {
        let service_identity_lhs = ServiceIdentity(ASDV_ServiceIdentity {
            sid: 0,
            fqin: new_service_fqin_for_test(),
            public_key: new_public_key_for_test(),
        });
        let mut service_identity_rhs = ServiceIdentity(ASDV_ServiceIdentity {
            sid: 0,
            fqin: new_service_fqin_for_test(),
            public_key: new_public_key_for_test(),
        });
        service_identity_rhs.0.public_key[1] = 1;
        assert_ne!(service_identity_lhs, service_identity_rhs);
    }
}
