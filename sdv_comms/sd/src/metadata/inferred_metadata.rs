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

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::InferredMetadata::{InferredMetadata as InferredMetadataAidl, RelativeLocation::RelativeLocation as RelativeLocationAidl};
use sdv_status_rs::SdvStatus;
use crate::ffi::ASDV_RelativeLocation;

#[derive(Default, Debug, Clone, PartialEq)]
#[repr(transparent)]
pub struct InferredMetadata {
    aidl: InferredMetadataAidl,
}

impl InferredMetadata {
    pub fn get_location(&self) -> ASDV_RelativeLocation {
        match self.aidl.location {
            RelativeLocationAidl::LOCATION_UNKNOWN => ASDV_RelativeLocation::ASDV_LOCATION_UNKNOWN,
            RelativeLocationAidl::LOCATION_LOCAL_SDV_INSTANCE => {
                ASDV_RelativeLocation::ASDV_LOCATION_LOCAL_SDV_INSTANCE
            }
            RelativeLocationAidl::LOCATION_LOCAL_HARDWARE => {
                ASDV_RelativeLocation::ASDV_LOCATION_LOCAL_HARDWARE
            }
            RelativeLocationAidl::LOCATION_REMOTE => ASDV_RelativeLocation::ASDV_LOCATION_REMOTE,
            _ => ASDV_RelativeLocation::ASDV_LOCATION_UNKNOWN,
        }
    }
}

impl TryFrom<InferredMetadataAidl> for InferredMetadata {
    type Error = SdvStatus;

    fn try_from(aidl: InferredMetadataAidl) -> Result<Self, Self::Error> {
        Ok(InferredMetadata { aidl })
    }
}

impl From<InferredMetadata> for InferredMetadataAidl {
    fn from(metadata: InferredMetadata) -> Self {
        metadata.aidl
    }
}
