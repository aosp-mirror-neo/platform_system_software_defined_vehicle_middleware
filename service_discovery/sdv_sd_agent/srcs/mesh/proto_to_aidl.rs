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
use sdv_sd_mesh_api::discovery::{
    ServiceUnitDefinition as ServiceUnitDefinitionProto, UnitType as UnitTypeProto,
};

use sdv_sd_mesh_api::metadata::{
    application_metadata::typed_metadata::Metadata as MetadataProto,
    application_metadata::TypedMetadata as TypedMetadataProto,
    ApplicationMetadata as ApplicationMetadataProto,
    PublisherUnitMetadata as PublisherUnitMetadataProto,
    ServerUnitMetadata as ServerUnitMetadataProto, TransportMetadata as TransportMetadataProto,
};

use sdv_sd_mesh_api::identity::{
    service_identity::EcPublicKey as EcPublicKeyProto, ServiceFqin as ServiceFqinProto,
    ServiceIdentity as ServiceIdentityProto,
};

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
    UnitType::UnitType as UnitTypeAidl,
    //
};

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::{
    ApplicationMetadata::ApplicationMetadata as ApplicationMetadataAidl,
    ApplicationMetadata::TypedMetadata::TypedMetadata as TypedMetadataAidl,
    InferredMetadata::InferredMetadata as InferredMetadataAidl,
    InferredMetadata::RelativeLocation::RelativeLocation as RelativeLocationAidl,
    PublisherUnitMetadata::PublisherUnitMetadata as PublisherUnitMetadataAidl,
    ServerUnitMetadata::ServerUnitMetadata as ServerUnitMetadataAidl,
    TransportMetadata::TransportMetadata as TransportMetadataAidl,
};

use google_sdv_identity::aidl::google::sdv::identity::{
    ServiceFqin::ServiceFqin as ServiceFqinAidl,
    ServiceIdentity::EcPublicKey::EcPublicKey as EcPublicKeyAidl,
    ServiceIdentity::ServiceIdentity as ServiceIdentityAidl,
};

use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::{
    ServiceUnitDefinition::ServiceUnitDefinition as ServiceUnitDefinitionAidl, //
};

use thiserror::Error;

pub(crate) trait TryToAidl<T> {
    fn try_to_aidl(&self) -> Result<T, ConversionError>;
}

#[derive(Error, Debug)]
pub enum ConversionError {
    #[error("Invalid key error: {0}")]
    InvalidKey(#[from] std::array::TryFromSliceError),
    #[error("Unexpected metadata variant")]
    UnexpectedMetadataVariant,
}

impl TryToAidl<EcPublicKeyAidl> for EcPublicKeyProto {
    fn try_to_aidl(&self) -> Result<EcPublicKeyAidl, ConversionError> {
        let key_value: [u8; 32] = self.key_value.as_slice().try_into()?;

        Ok(EcPublicKeyAidl { keyValue: key_value })
    }
}
impl TryToAidl<ServiceFqinAidl> for ServiceFqinProto {
    fn try_to_aidl(&self) -> Result<ServiceFqinAidl, ConversionError> {
        Ok(ServiceFqinAidl {
            sdvVmName: self.sdv_vm_name.clone(),
            sdvPackageName: self.sdv_package_name.clone(),
            serviceBundleName: self.service_bundle_name.clone(),
            serviceInstanceName: self.service_instance_name.clone(),
        })
    }
}

impl TryToAidl<ServiceIdentityAidl> for ServiceIdentityProto {
    fn try_to_aidl(&self) -> Result<ServiceIdentityAidl, ConversionError> {
        Ok(ServiceIdentityAidl {
            sid: self.sid,
            publicKey: self.public_key.as_ref().unwrap_or_default().try_to_aidl()?,
            name: self.name.as_ref().unwrap_or_default().try_to_aidl()?,
        })
    }
}

impl TryToAidl<ServiceUnitDefinitionAidl> for ServiceUnitDefinitionProto {
    fn try_to_aidl(&self) -> Result<ServiceUnitDefinitionAidl, ConversionError> {
        Ok(ServiceUnitDefinitionAidl {
            identity: self.identity.as_ref().unwrap_or_default().try_to_aidl()?,
            serviceUnitName: self.service_unit_name.clone(),
            unitType: self.unit_type.as_ref().unwrap_or_default().try_to_aidl()?,
            appMetadata: self.app_metadata.as_ref().unwrap_or_default().try_to_aidl()?,
            inferredMetadata: InferredMetadataAidl {
                location: RelativeLocationAidl::LOCATION_UNKNOWN,
            },
        })
    }
}

impl TryToAidl<UnitTypeAidl> for UnitTypeProto {
    fn try_to_aidl(&self) -> Result<UnitTypeAidl, ConversionError> {
        Ok(UnitTypeAidl {
            sdvPackageName: self.sdv_package_name.clone(),
            serviceBundleName: self.service_bundle_name.clone(),
            typeName: self.type_name.clone(),
        })
    }
}

impl TryToAidl<ApplicationMetadataAidl> for ApplicationMetadataProto {
    fn try_to_aidl(&self) -> Result<ApplicationMetadataAidl, ConversionError> {
        Ok(ApplicationMetadataAidl {
            typedMetadata: self.typed_metadata.as_ref().unwrap_or_default().try_to_aidl()?,
            valueHolder: self.value_holder.clone(),
        })
    }
}

impl TryToAidl<TypedMetadataAidl> for TypedMetadataProto {
    fn try_to_aidl(&self) -> Result<TypedMetadataAidl, ConversionError> {
        match self.metadata.as_ref() {
            Some(MetadataProto::ServerUnit(data)) => {
                data.try_to_aidl().map(TypedMetadataAidl::ServerUnit)
            }
            Some(MetadataProto::PublisherUnit(data)) => {
                data.try_to_aidl().map(TypedMetadataAidl::PublisherUnit)
            }
            _ => Err(ConversionError::UnexpectedMetadataVariant),
        }
    }
}

impl TryToAidl<ServerUnitMetadataAidl> for ServerUnitMetadataProto {
    fn try_to_aidl(&self) -> Result<ServerUnitMetadataAidl, ConversionError> {
        Ok(ServerUnitMetadataAidl { version: self.version as i32 })
    }
}

impl TryToAidl<TransportMetadataAidl> for TransportMetadataProto {
    fn try_to_aidl(&self) -> Result<TransportMetadataAidl, ConversionError> {
        Ok(TransportMetadataAidl { value_holder: self.value_holder.clone() })
    }
}

impl TryToAidl<PublisherUnitMetadataAidl> for PublisherUnitMetadataProto {
    fn try_to_aidl(&self) -> Result<PublisherUnitMetadataAidl, ConversionError> {
        Ok(PublisherUnitMetadataAidl {
            version: self.version as i32,
            message_size: self.message_size,
            message_count: self.message_count,
            channel_name: self.channel_name.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::aidl_to_proto::ToProto;

    #[test]
    fn test_convert_ec_public_key() {
        let original_proto =
            EcPublicKeyProto { key_value: [1u8; 32].to_vec(), ..Default::default() };
        let aidl = original_proto.try_to_aidl().expect("Failed to convert proto to aidl");

        assert_eq!(aidl.to_proto(), original_proto);
    }

    #[test]
    fn test_convert_service_fqin() {
        let original_proto = ServiceFqinProto {
            sdv_vm_name: "vm_name".to_string(),
            sdv_package_name: "package_name".to_string(),
            service_bundle_name: "service_name".to_string(),
            service_instance_name: "instance_name".to_string(),
            ..Default::default()
        };
        let aidl = original_proto.try_to_aidl().expect("Failed to convert proto to aidl");

        assert_eq!(aidl.to_proto(), original_proto);
    }

    #[test]
    fn test_convert_service_identity() {
        let original_proto = ServiceIdentityProto {
            sid: 123,
            public_key: Some(EcPublicKeyProto {
                key_value: [1u8; 32].to_vec(),
                ..Default::default()
            })
            .into(),
            name: Some(ServiceFqinProto {
                sdv_vm_name: "vm_name".to_string(),
                sdv_package_name: "package_name".to_string(),
                service_bundle_name: "service_name".to_string(),
                service_instance_name: "instance_name".to_string(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        };
        let aidl = original_proto.try_to_aidl().expect("Failed to convert proto to aidl");

        assert_eq!(aidl.to_proto(), original_proto);
    }

    #[test]
    fn test_convert_service_unit_definition() {
        let original_proto = ServiceUnitDefinitionProto {
            identity: Some(ServiceIdentityProto {
                sid: 123,
                public_key: Some(EcPublicKeyProto {
                    key_value: [1u8; 32].to_vec(),
                    ..Default::default()
                })
                .into(),
                name: Some(ServiceFqinProto {
                    sdv_vm_name: "vm_name".to_string(),
                    sdv_package_name: "package_name".to_string(),
                    service_bundle_name: "service_name".to_string(),
                    service_instance_name: "instance_name".to_string(),
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            })
            .into(),
            service_unit_name: "unit_name".to_string(),
            unit_type: Some(UnitTypeProto {
                sdv_package_name: "package_name".to_string(),
                service_bundle_name: "service_name".to_string(),
                type_name: "type_name".to_string(),
                ..Default::default()
            })
            .into(),
            app_metadata: Some(ApplicationMetadataProto {
                typed_metadata: Some(TypedMetadataProto {
                    metadata: Some(MetadataProto::ServerUnit(ServerUnitMetadataProto {
                        version: 123,
                        ..Default::default()
                    })),
                    ..Default::default()
                })
                .into(),
                value_holder: "value_holder".to_string().into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        };
        let aidl = original_proto.try_to_aidl().expect("Failed to convert proto to aidl");

        assert_eq!(aidl.to_proto(), original_proto);
    }

    #[test]
    fn test_convert_unit_type() {
        let original_proto = UnitTypeProto {
            sdv_package_name: "package_name".to_string(),
            service_bundle_name: "service_name".to_string(),
            type_name: "type_name".to_string(),
            ..Default::default()
        };
        let aidl = original_proto.try_to_aidl().expect("Failed to convert proto to aidl");

        assert_eq!(aidl.to_proto(), original_proto);
    }

    #[test]
    fn test_convert_server_unit_metadata() {
        let original_proto = ServerUnitMetadataProto { version: 123, ..Default::default() };
        let aidl = original_proto.try_to_aidl().expect("Failed to convert proto to aidl");

        assert_eq!(aidl.to_proto(), original_proto);
    }

    #[test]
    fn test_convert_transport_metadata() {
        let original_proto = TransportMetadataProto {
            value_holder: "value_holder".to_string().into(),
            ..Default::default()
        };
        let aidl = original_proto.try_to_aidl().expect("Failed to convert proto to aidl");

        assert_eq!(aidl.to_proto(), original_proto);
    }

    #[test]
    fn test_convert_publisher_unit_metadata() {
        let original_proto = PublisherUnitMetadataProto { version: 123, ..Default::default() };
        let aidl = original_proto.try_to_aidl().expect("Failed to convert proto to aidl");

        assert_eq!(aidl.to_proto(), original_proto);
    }

    #[test]
    fn test_convert_ec_public_key_invalid_length() {
        let original_proto =
            EcPublicKeyProto { key_value: [1u8; 31].to_vec(), ..Default::default() };
        let aidl = original_proto.try_to_aidl();

        assert!(aidl.is_err());
    }

    #[test]
    fn test_convert_typed_metadata_unexpected_variant() {
        let original_proto = TypedMetadataProto { ..Default::default() };
        let aidl = original_proto.try_to_aidl();

        assert!(aidl.is_err());
        assert!(matches!(aidl.unwrap_err(), ConversionError::UnexpectedMetadataVariant));
    }
}
