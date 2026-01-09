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

//! Submodule that implements the SDV Service Discovery Metadata API.
#![allow(dead_code)]

mod publisher_unit_metadata;
pub use publisher_unit_metadata::PublisherUnitMetadata;

mod server_unit_metadata;
pub use server_unit_metadata::ServerUnitMetadata;

mod unit_metadata;
pub use unit_metadata::UnitMetadata;

mod inferred_metadata;
pub use inferred_metadata::InferredMetadata;
