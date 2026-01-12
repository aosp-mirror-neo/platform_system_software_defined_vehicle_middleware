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

//! Module that defines the SDK API for Service Unit Registration and Discovery.

mod service_unit_definition;
pub use service_unit_definition::ServiceUnitDefinition;

mod unit_metadata;
pub use unit_metadata::{PublisherUnitMetadata, ServerUnitMetadata, UnitMetadata};

mod inferred_metadata;
pub use inferred_metadata::{InferredMetadata, RelativeLocation};

mod unit_name_discovery_args;
pub use unit_name_discovery_args::UnitNameDiscoveryArgs;

mod unit_registration_args;
pub use unit_registration_args::UnitRegistrationArgs;

mod unit_type;
pub use unit_type::UnitType;

mod service_discovery_manager;
pub use service_discovery_manager::ServiceDiscoveryManager;

mod service_unit_change_event;
pub use service_unit_change_event::{ServiceUnitChangeEvent, UnitChangeEventStream};

#[cfg(test)]
mod e2e_test;
