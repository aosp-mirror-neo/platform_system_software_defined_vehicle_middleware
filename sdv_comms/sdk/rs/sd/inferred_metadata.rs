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

use sdv_comms_bindgen::{ASDV_InferredMetadata, ASDV_RelativeLocation};
use sdv_status_rs::SdvStatusCode;

/// Representation of InferredMetadata.
#[derive(Debug)]
pub struct InferredMetadata(*mut ASDV_InferredMetadata);

impl InferredMetadata {
    /// Create a new [ServerUnitMetadata].
    pub fn new() -> Self {
        // SAFETY: result of `ASDV_InferredMetadata_create` is guaranteed to be a valid pointer.
        Self(unsafe { sdv_comms_bindgen::ASDV_InferredMetadata_create() })
    }

    /// Raw pointer to the underlying implementation.
    pub fn as_raw_mut(&mut self) -> *mut ASDV_InferredMetadata {
        self.0
    }

    /// Returns the location field of this InferredMetadata.
    pub fn get_location(&self) -> RelativeLocation {
        let mut raw_location = ASDV_RelativeLocation::default();

        // SAFETY: `self.0` and `raw_location` are guaranteed to be valid.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_InferredMetadata_getLocation(self.0, &mut raw_location)
        };

        SdvStatusCode::from(status).expect("InferredMetadata::get_relative_location() has failed");

        match raw_location {
            sdv_comms_bindgen::ASDV_RelativeLocation_ASDV_LOCATION_UNKNOWN => {
                RelativeLocation::Unknown
            }
            sdv_comms_bindgen::ASDV_RelativeLocation_ASDV_LOCATION_LOCAL_SDV_INSTANCE => {
                RelativeLocation::LocalSdvInstance
            }
            sdv_comms_bindgen::ASDV_RelativeLocation_ASDV_LOCATION_LOCAL_HARDWARE => {
                RelativeLocation::LocalHardware
            }
            sdv_comms_bindgen::ASDV_RelativeLocation_ASDV_LOCATION_REMOTE => {
                RelativeLocation::Remote
            }
            _ => RelativeLocation::Unknown,
        }
    }
}

impl Default for InferredMetadata {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for InferredMetadata {
    fn drop(&mut self) {
        // SAFETY: `self.0` is guaranteed to be a valid pointer, before
        // `ASDV_InferredMetadata_destroy` is called.
        unsafe { sdv_comms_bindgen::ASDV_InferredMetadata_destroy(self.0) }
    }
}

/// Relative location of the service unit relative to the caller of the API.
#[derive(Debug, PartialEq)]
pub enum RelativeLocation {
    /// Unknown
    Unknown,
    /// Local SDV instance
    LocalSdvInstance,
    /// Local hardware
    LocalHardware,
    /// Remote
    Remote,
}
