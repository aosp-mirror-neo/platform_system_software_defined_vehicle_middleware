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

//! Submodule that implements the C ABI layer for SDV Service Registration and Discovery API.
#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![cfg_attr(not(comms_stack_ffi), allow(unused_imports))]

mod unit_discovery;
mod unit_metadata;
mod unit_registration;
mod unit_type;

pub use unit_discovery::{
    ASDV_ServiceUnitChangeEvent, ASDV_ServiceUnitChangeEventType, ASDV_ServiceUnitDefinition,
    ASDV_UnitNameDiscoveryArgs,
};
pub use unit_metadata::{
    ASDV_InferredMetadata, ASDV_PublisherUnitMetadata, ASDV_RelativeLocation,
    ASDV_ServerUnitMetadata,
};
pub use unit_registration::{
    ASDV_UnitMetadataType::{
        self, ASDV_UNIT_METADATA_TYPE_PUBLISHER, ASDV_UNIT_METADATA_TYPE_SERVER,
    },
    ASDV_UnitRegistrationArgs,
};
pub use unit_type::ASDV_UnitType;
