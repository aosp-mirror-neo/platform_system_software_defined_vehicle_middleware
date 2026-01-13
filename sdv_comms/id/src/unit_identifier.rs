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

//! This file declares the Unit Identifier object.

use google_sdv_identity::aidl::google::sdv::identity::UnitIdentifier::UnitIdentifier as UnitIdentifierAidl;
use sdv_comms_common::ffi_str;

use sdv_status_rs::{SdvResult, SdvStatus};

use crate::ffi::ASDV_UnitIdentifier;
use crate::ServiceIdentity;

/// Defines the Unit Identifier for the Service Unit.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitIdentifier {
    service_identity: ServiceIdentity,
    unit_name: String,
}

impl UnitIdentifier {
    /// Creates a new unit identifier.
    pub fn new(service_identity: ServiceIdentity, unit_name: String) -> Self {
        Self { service_identity, unit_name }
    }

    /// Returns the globally unique 64 bit identifier of the corresponding ServiceIdentity.
    pub fn sid(&self) -> u64 {
        self.service_identity.sid()
    }

    /// Returns the complete corresponding ServiceIdentity.
    pub fn service_identity(&self) -> &ServiceIdentity {
        &self.service_identity
    }

    /// Returns Unit Name.
    pub fn unit_name(&self) -> &str {
        &self.unit_name
    }

    /// Clones this UnitIdentifier instance into an ASDV_UnitIdentifier.
    pub fn clone_into(&self, asdv_unit_identifier: &mut ASDV_UnitIdentifier) -> SdvResult<()> {
        self.service_identity.clone_into(&mut asdv_unit_identifier.service_identity)?;
        ffi_str::str_to_bytes(self.unit_name(), &mut asdv_unit_identifier.unit_name)?;
        Ok(())
    }
}

impl TryFrom<UnitIdentifierAidl> for UnitIdentifier {
    type Error = SdvStatus;

    fn try_from(aidl: UnitIdentifierAidl) -> Result<Self, Self::Error> {
        Ok(UnitIdentifier {
            service_identity: aidl.service_identity.try_into()?,
            unit_name: aidl.unit_name,
        })
    }
}

impl From<&UnitIdentifier> for UnitIdentifierAidl {
    fn from(val: &UnitIdentifier) -> Self {
        let mut aidl =
            UnitIdentifierAidl { unit_name: val.unit_name.to_string(), ..Default::default() };
        // TODO: Update ServiceIdentity->Aidl to work with borrowed object and remove clone().
        aidl.service_identity = val.service_identity.clone().into();

        aidl
    }
}

/// Converts ASDV_UnitIdentifier to UnitIdentifier.
impl TryFrom<&ASDV_UnitIdentifier> for UnitIdentifier {
    type Error = SdvStatus;

    fn try_from(asdv_unit_identifier: &ASDV_UnitIdentifier) -> SdvResult<UnitIdentifier> {
        Ok(UnitIdentifier {
            service_identity: ServiceIdentity::try_from(&asdv_unit_identifier.service_identity)?,
            unit_name: ffi_str::bytes_to_str(&asdv_unit_identifier.unit_name)?.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinAidl;
    use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::EcPublicKey::EcPublicKey as EcPublicKeyAidl;
    use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::ServiceIdentity as ServiceIdentityAidl;

    use crate::SERVICE_PUBLIC_KEY_SIZE_BYTES;

    fn get_valid_aidl_unit_identifier() -> UnitIdentifierAidl {
        UnitIdentifierAidl {
            service_identity: ServiceIdentityAidl {
                sid: 12345_i64,
                name: ServiceFqinAidl {
                    sdvVmName: "vm-test".to_string(),
                    sdvPackageName: "com.google.sdv".to_string(),
                    serviceBundleName: "TestBundleName".to_string(),
                    serviceInstanceName: "default".to_string(),
                },
                publicKey: EcPublicKeyAidl { keyValue: [45_u8; SERVICE_PUBLIC_KEY_SIZE_BYTES] },
            },
            unit_name: "UnitName".to_string(),
        }
    }

    #[test]
    fn test_try_from_aidl_unit_identifier_succeeds() {
        let aidl = get_valid_aidl_unit_identifier();
        let si = ServiceIdentity::try_from(aidl.service_identity.clone()).unwrap();
        let uid = UnitIdentifier::try_from(aidl).unwrap();

        assert_eq!(uid.sid(), 12345_u64);
        assert_eq!(*uid.service_identity(), si);
        assert_eq!(uid.unit_name(), "UnitName");
    }

    #[test]
    fn test_try_into_aidl_unit_identifier_succeeds() {
        let aidl = get_valid_aidl_unit_identifier();
        let uid = UnitIdentifier::try_from(aidl.clone()).unwrap();
        let new_aidl: UnitIdentifierAidl = UnitIdentifierAidl::from(&uid);

        assert_eq!(aidl.service_identity.sid, new_aidl.service_identity.sid);
        assert_eq!(aidl.unit_name, new_aidl.unit_name);
    }

    #[test]
    fn test_clone_into_try_from_asdv_unit_identifier_succeeds() {
        let aidl = get_valid_aidl_unit_identifier();
        let uid1 = UnitIdentifier::try_from(aidl).unwrap();

        let mut asdv = ASDV_UnitIdentifier::default();
        assert!(uid1.clone_into(&mut asdv).is_ok());

        let uid2 = UnitIdentifier::try_from(&asdv).unwrap();

        assert_eq!(uid1, uid2);
    }
}
