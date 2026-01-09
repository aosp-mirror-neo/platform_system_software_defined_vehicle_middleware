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

use crate::discovery::UnitType;
use crate::metadata::UnitMetadata;
use sdv_comms_id::names::ServiceUnitName;

/// Arguments needed for registering a Service Unit with Service Discovery.
#[derive(Clone, Default)]
pub struct UnitRegistrationArgs {
    unit_name: Option<ServiceUnitName>,
    unit_type: UnitType,
    unit_metadata: Option<UnitMetadata>,
}

impl UnitRegistrationArgs {
    /// Returns the Unit Name set for this [UnitRegistrationArgs].
    pub fn get_unit_name(&self) -> Option<&ServiceUnitName> {
        self.unit_name.as_ref()
    }

    /// Sets the Unit Name for this [UnitRegistrationArgs].
    pub fn set_unit_name(&mut self, unit_name: ServiceUnitName) {
        self.unit_name = Some(unit_name);
    }

    /// Returns the Unit Type set for this [UnitRegistrationArgs].
    pub fn get_unit_type(&self) -> &UnitType {
        &self.unit_type
    }

    /// Sets the Unit Type for this [UnitRegistrationArgs].
    pub fn set_unit_type(&mut self, unit_type: UnitType) {
        self.unit_type = unit_type;
    }

    /// Returns the Unit Metadata set for this [UnitRegistrationArgs].
    pub fn get_unit_metadata(&self) -> Option<&UnitMetadata> {
        self.unit_metadata.as_ref()
    }

    /// Returns a mutable reference to UnitMetadata if it's set
    /// Else initializes UnitMetadata with a default value and returns a mutable reference
    #[must_use]
    pub fn get_mut_unit_metadata(&mut self) -> &mut UnitMetadata {
        self.unit_metadata.get_or_insert_with(Default::default)
    }
}
