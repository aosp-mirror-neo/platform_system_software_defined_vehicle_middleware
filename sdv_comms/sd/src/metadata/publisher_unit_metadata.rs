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

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::PublisherUnitMetadata::PublisherUnitMetadata as PublisherUnitMetadataAidl;
use sdv_comms_id::names::ChannelName;
use sdv_status_rs::{derive, SdvStatus};
use std::str::FromStr;

#[derive(Default, Debug, Clone, PartialEq)]
#[repr(transparent)]
pub struct PublisherUnitMetadata {
    aidl: PublisherUnitMetadataAidl,
}

impl PublisherUnitMetadata {
    pub fn get_version(&self) -> u32 {
        self.aidl.version as u32
    }

    pub fn set_version(&mut self, version: u32) {
        self.aidl.version = version as i32;
    }

    pub fn get_message_size(&self) -> u32 {
        self.aidl.message_size as u32
    }

    pub fn set_message_size(&mut self, message_size: u32) {
        self.aidl.message_size = message_size as i32;
    }

    pub fn get_message_count(&self) -> u32 {
        self.aidl.message_count as u32
    }

    pub fn set_message_count(&mut self, message_count: u32) {
        self.aidl.message_count = message_count as i32;
    }

    pub fn get_channel_name(&self) -> &str {
        &self.aidl.channel_name
    }

    pub fn set_channel_name(&mut self, channel_name: ChannelName) {
        self.aidl.channel_name = channel_name.into();
    }
}

impl TryFrom<PublisherUnitMetadataAidl> for PublisherUnitMetadata {
    type Error = SdvStatus;

    fn try_from(aidl: PublisherUnitMetadataAidl) -> Result<Self, Self::Error> {
        // Validate channel name upon conversion.
        ChannelName::from_str(&aidl.channel_name).map_err(|e| {
            SdvStatus::with_message(
                sdv_status_rs::SdvStatusCode::InvalidArgument,
                format!("Invalid channel name: {e}"),
            )
        })?;

        Ok(PublisherUnitMetadata { aidl })
    }
}

impl From<PublisherUnitMetadata> for PublisherUnitMetadataAidl {
    fn from(metadata: PublisherUnitMetadata) -> Self {
        metadata.aidl
    }
}
