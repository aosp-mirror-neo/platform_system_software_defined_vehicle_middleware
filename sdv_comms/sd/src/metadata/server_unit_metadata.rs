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

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::ServerUnitMetadata::ServerUnitMetadata as ServerUnitMetadataAidl;
use sdv_status_rs::SdvStatus;

#[derive(Clone, Debug, Default, PartialEq)]
#[repr(transparent)]
pub struct ServerUnitMetadata {
    aidl: ServerUnitMetadataAidl,
}

impl ServerUnitMetadata {
    pub fn get_version(&self) -> u32 {
        self.aidl.version as u32
    }

    pub fn set_version(&mut self, version: u32) {
        self.aidl.version = version as i32;
    }
}

impl TryFrom<ServerUnitMetadataAidl> for ServerUnitMetadata {
    type Error = SdvStatus;

    fn try_from(aidl: ServerUnitMetadataAidl) -> Result<Self, Self::Error> {
        Ok(ServerUnitMetadata { aidl })
    }
}

impl From<ServerUnitMetadata> for ServerUnitMetadataAidl {
    fn from(metadata: ServerUnitMetadata) -> Self {
        metadata.aidl
    }
}
