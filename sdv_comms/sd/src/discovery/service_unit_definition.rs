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

use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::{
    discovery::ServiceUnitDefinition::ServiceUnitDefinition as ServiceUnitDefinitionAidl,
};
use sdv_comms_id::{ServiceIdentity, UnitIdentifier};
use sdv_status_rs::SdvStatus;

use crate::discovery::UnitType;
use crate::metadata::InferredMetadata;
use crate::metadata::UnitMetadata;

#[derive(Clone, Debug)]
pub struct ServiceUnitDefinition {
    aidl: ServiceUnitDefinitionAidl,

    // TODO(b/359964171): Remove these fields once we have a way to obtain them from the AIDL
    // cheaply, without having to cache them in the ServiceUnitDefinition. Currently we need to
    // cache these values because the conversion is non-trivial.
    unit_type: UnitType,
    identity: ServiceIdentity,
    unit_metadata: UnitMetadata,
    inferred_metadata: InferredMetadata,
}

impl ServiceUnitDefinition {
    /// Returns Service Unit Name.
    pub fn get_service_unit_name(&self) -> &str {
        &self.aidl.serviceUnitName
    }

    /// Returns Unit Type.
    pub fn get_unit_type(&self) -> &UnitType {
        &self.unit_type
    }

    /// Returns ServiceIdentity of owning Service Bundle.
    pub fn get_identity(&self) -> &ServiceIdentity {
        &self.identity
    }

    /// Returns UnitIdentifier for this ServiceUnitDefinition.
    pub fn get_unit_identifier(&self) -> UnitIdentifier {
        // TODO: Get rid of this clone (probably by updating UnitIdentifier::new.)
        UnitIdentifier::new(self.get_identity().clone(), self.get_service_unit_name().to_string())
    }

    /// Returns [UnitMetadata] for this ServiceUnitDefinition.
    pub fn get_unit_metadata(&self) -> &UnitMetadata {
        &self.unit_metadata
    }

    /// Returns [InferredMetadata] for this ServiceUnitDefinition.
    pub fn get_inferred_metadata(&self) -> &InferredMetadata {
        &self.inferred_metadata
    }
}

impl TryFrom<ServiceUnitDefinitionAidl> for ServiceUnitDefinition {
    type Error = SdvStatus;

    fn try_from(aidl: ServiceUnitDefinitionAidl) -> Result<Self, SdvStatus> {
        let unit_type = UnitType::try_from(aidl.unitType.clone())?;
        let identity = ServiceIdentity::try_from(aidl.identity.clone())?;
        let unit_metadata = UnitMetadata::try_from(aidl.appMetadata.clone())?;
        let inferred_metadata = InferredMetadata::try_from(aidl.inferredMetadata.clone())?;
        Ok(ServiceUnitDefinition { aidl, unit_type, identity, unit_metadata, inferred_metadata })
    }
}

impl From<ServiceUnitDefinition> for ServiceUnitDefinitionAidl {
    fn from(sud: ServiceUnitDefinition) -> Self {
        sud.aidl
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::ASDV_RelativeLocation;
    use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinAidl;
    use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::EcPublicKey::EcPublicKey as EcPublicKeyAidl;
    use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::ServiceIdentity as ServiceIdentityAidl;
    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::{
        ApplicationMetadata::{ApplicationMetadata as ApplicationMetadataAidl},
        InferredMetadata::{
            InferredMetadata as InferredMetadataAidl,
            RelativeLocation::RelativeLocation as RelativeLocationAidl
        }
    };
    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::UnitType::UnitType as UnitTypeAidl;
    use sdv_comms_id::{ServiceFqin, SERVICE_PUBLIC_KEY_SIZE_BYTES};
    use std::str::FromStr;

    #[test]
    fn test_convert_from_aidl_success() {
        let fqin = ServiceFqin::from_str("vm-name:com.test.pkg.TestBundleName/default").unwrap();
        let unit_type = UnitType::from_str("com.google.sdv.WindowManager#Temperature").unwrap();
        let identity_aidl = ServiceIdentityAidl {
            sid: 1234,
            publicKey: EcPublicKeyAidl { keyValue: [45_u8; SERVICE_PUBLIC_KEY_SIZE_BYTES] },
            name: fqin.clone().into(),
        };
        let aidl = ServiceUnitDefinitionAidl {
            serviceUnitName: String::from("service_unit_name"),
            identity: identity_aidl.clone(),
            unitType: unit_type.clone().into(),
            appMetadata: ApplicationMetadataAidl {
                valueHolder: [1, 1, 1].to_vec(),
                ..Default::default()
            },
            inferredMetadata: InferredMetadataAidl {
                location: RelativeLocationAidl::LOCATION_REMOTE,
            },
        };

        let conv_result = ServiceUnitDefinition::try_from(aidl);

        assert!(conv_result.is_ok());
        let sud = conv_result.unwrap();
        assert_eq!(sud.get_unit_type().clone(), unit_type);
        assert_eq!(sud.get_identity().clone(), ServiceIdentity::try_from(identity_aidl).unwrap());
        assert_eq!(sud.get_service_unit_name(), "service_unit_name");
        assert_eq!(
            sud.get_inferred_metadata().get_location(),
            ASDV_RelativeLocation::ASDV_LOCATION_REMOTE
        );
    }

    #[test]
    fn test_convert_to_aidl_success() {
        let fqin = ServiceFqin::from_str("vm-name:com.test.pkg.TestBundleName/default").unwrap();
        let unit_type = UnitType::from_str("com.google.sdv.WindowManager#Temperature").unwrap();
        let identity_aidl = ServiceIdentityAidl {
            sid: 1234,
            publicKey: EcPublicKeyAidl { keyValue: [45_u8; SERVICE_PUBLIC_KEY_SIZE_BYTES] },
            name: fqin.clone().into(),
        };
        let aidl = ServiceUnitDefinitionAidl {
            serviceUnitName: String::from("service_unit_name"),
            identity: identity_aidl.clone(),
            unitType: unit_type.clone().into(),
            appMetadata: ApplicationMetadataAidl {
                valueHolder: [1, 1, 1].to_vec(),
                ..Default::default()
            },
            inferredMetadata: InferredMetadataAidl {
                location: RelativeLocationAidl::LOCATION_REMOTE,
            },
        };

        let conv_result = ServiceUnitDefinition::try_from(aidl.clone());
        assert!(conv_result.is_ok());

        let conv_result: ServiceUnitDefinitionAidl = conv_result.unwrap().into();
        assert_eq!(conv_result, aidl);
    }
}
