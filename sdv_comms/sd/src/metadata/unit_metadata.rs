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

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::ApplicationMetadata::ApplicationMetadata as ApplicationMetadataAidl;
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::ApplicationMetadata::TypedMetadata::TypedMetadata as TypedMetadataAidl;
use sdv_status_rs::SdvStatus;

use super::{PublisherUnitMetadata, ServerUnitMetadata};
use crate::ffi::ASDV_UnitMetadataType;

#[derive(Clone, Debug, Default)]
pub struct UnitMetadata {
    aidl: ApplicationMetadataAidl,
}

impl UnitMetadata {
    pub fn get_type(&self) -> ASDV_UnitMetadataType {
        match self.aidl.typedMetadata {
            TypedMetadataAidl::ServerUnit(_) => {
                ASDV_UnitMetadataType::ASDV_UNIT_METADATA_TYPE_SERVER
            }
            TypedMetadataAidl::PublisherUnit(_) => {
                ASDV_UnitMetadataType::ASDV_UNIT_METADATA_TYPE_PUBLISHER
            }
        }
    }

    // Method to retrieve a reference to the ServerUnitMetadata if the variant is `Server`
    pub fn get_server_metadata(&self) -> Option<ServerUnitMetadata> {
        match &self.aidl.typedMetadata {
            TypedMetadataAidl::ServerUnit(server_metadata_aidl) => {
                Some(server_metadata_aidl.clone().try_into().ok()?)
            }
            _ => None,
        }
    }

    // Method to set ServerUnitMetadata
    pub fn set_server_metadata(&mut self, metadata: ServerUnitMetadata) {
        self.aidl.typedMetadata = TypedMetadataAidl::ServerUnit(metadata.into())
    }

    // Method to retrieve a reference to the PublisherUnitMetadata if the variant is `Publisher`
    pub fn get_publisher_metadata(&self) -> Option<PublisherUnitMetadata> {
        match &self.aidl.typedMetadata {
            TypedMetadataAidl::PublisherUnit(ref publisher_metadata_aidl) => {
                Some(publisher_metadata_aidl.clone().try_into().ok()?)
            }
            _ => None,
        }
    }

    // Method to set a reference PublisherUnitMetadata
    pub fn set_publisher_metadata(&mut self, metadata: PublisherUnitMetadata) {
        self.aidl.typedMetadata = TypedMetadataAidl::PublisherUnit(metadata.into())
    }

    // Method to retrieve application metadata
    pub fn get_application_metadata(&self) -> &[u8] {
        &self.aidl.valueHolder
    }

    // Method to retrieve size of application metadata
    pub fn get_application_metadata_size(&self) -> usize {
        self.aidl.valueHolder.len()
    }

    // Method to set application metadata
    pub fn set_application_metadata(&mut self, metadata: &[u8]) {
        self.aidl.valueHolder = metadata.to_vec()
    }
}

impl TryFrom<ApplicationMetadataAidl> for UnitMetadata {
    type Error = SdvStatus;

    fn try_from(aidl: ApplicationMetadataAidl) -> Result<Self, Self::Error> {
        Ok(UnitMetadata { aidl })
    }
}

impl From<UnitMetadata> for ApplicationMetadataAidl {
    fn from(metadata: UnitMetadata) -> Self {
        metadata.aidl
    }
}
