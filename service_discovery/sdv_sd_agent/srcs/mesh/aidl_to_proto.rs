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

use protobuf::MessageField;
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

pub(crate) trait ToProto<T> {
    fn to_proto(&self) -> T;
}

impl ToProto<ServiceUnitDefinitionProto> for ServiceUnitDefinitionAidl {
    fn to_proto(&self) -> ServiceUnitDefinitionProto {
        ServiceUnitDefinitionProto {
            identity: MessageField::some(self.identity.to_proto()),
            service_unit_name: self.serviceUnitName.clone(),
            unit_type: MessageField::some(self.unitType.to_proto()),
            app_metadata: MessageField::some(self.appMetadata.to_proto()),
            ..Default::default()
        }
    }
}

impl ToProto<UnitTypeProto> for UnitTypeAidl {
    fn to_proto(&self) -> UnitTypeProto {
        UnitTypeProto {
            sdv_package_name: self.sdvPackageName.clone(),
            service_bundle_name: self.serviceBundleName.clone(),
            type_name: self.typeName.clone(),
            ..Default::default()
        }
    }
}

impl ToProto<ApplicationMetadataProto> for ApplicationMetadataAidl {
    fn to_proto(&self) -> ApplicationMetadataProto {
        ApplicationMetadataProto {
            typed_metadata: MessageField::some(self.typedMetadata.to_proto()),
            value_holder: self.valueHolder.clone(),
            ..Default::default()
        }
    }
}

impl ToProto<TypedMetadataProto> for TypedMetadataAidl {
    fn to_proto(&self) -> TypedMetadataProto {
        match self {
            TypedMetadataAidl::ServerUnit(data) => TypedMetadataProto {
                metadata: Some(MetadataProto::ServerUnit(data.to_proto())),
                ..Default::default()
            },
            TypedMetadataAidl::PublisherUnit(data) => TypedMetadataProto {
                metadata: Some(MetadataProto::PublisherUnit(data.to_proto())),
                ..Default::default()
            },
        }
    }
}

impl ToProto<ServerUnitMetadataProto> for ServerUnitMetadataAidl {
    fn to_proto(&self) -> ServerUnitMetadataProto {
        ServerUnitMetadataProto { version: self.version as u32, ..Default::default() }
    }
}

impl ToProto<TransportMetadataProto> for TransportMetadataAidl {
    fn to_proto(&self) -> TransportMetadataProto {
        TransportMetadataProto { value_holder: self.value_holder.clone(), ..Default::default() }
    }
}

impl ToProto<PublisherUnitMetadataProto> for PublisherUnitMetadataAidl {
    fn to_proto(&self) -> PublisherUnitMetadataProto {
        PublisherUnitMetadataProto {
            version: self.version as u32,
            message_size: self.message_size,
            message_count: self.message_count,
            ..Default::default()
        }
    }
}

impl ToProto<ServiceIdentityProto> for ServiceIdentityAidl {
    fn to_proto(&self) -> ServiceIdentityProto {
        ServiceIdentityProto {
            sid: self.sid,
            public_key: MessageField::some(self.publicKey.to_proto()),
            name: MessageField::some(self.name.to_proto()),
            ..Default::default()
        }
    }
}

impl ToProto<EcPublicKeyProto> for EcPublicKeyAidl {
    fn to_proto(&self) -> EcPublicKeyProto {
        EcPublicKeyProto { key_value: self.keyValue.to_vec(), ..Default::default() }
    }
}

impl ToProto<ServiceFqinProto> for ServiceFqinAidl {
    fn to_proto(&self) -> ServiceFqinProto {
        ServiceFqinProto {
            sdv_vm_name: self.sdvVmName.clone(),
            sdv_package_name: self.sdvPackageName.clone(),
            service_bundle_name: self.serviceBundleName.clone(),
            service_instance_name: self.serviceInstanceName.clone(),
            ..Default::default()
        }
    }
}
