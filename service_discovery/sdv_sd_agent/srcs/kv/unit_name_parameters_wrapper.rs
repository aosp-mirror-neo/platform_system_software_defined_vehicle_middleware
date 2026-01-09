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
use crate::kv::ServiceUnitDefinition;
use crate::kv::UnitNameParameters;

#[repr(transparent)]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct InternalUnitNameParameters(pub(crate) UnitNameParameters);
impl InternalUnitNameParameters {
    pub(crate) fn is_query_match(&self, service_unit_definition: &ServiceUnitDefinition) -> bool {
        let same_vm: bool = match self.0.sdvVmName.as_ref() {
            Some(query_vm_name) => {
                service_unit_definition.identity.name.sdvVmName == *query_vm_name
            }
            None => true, // The caller doesn't care about VM name
        };
        let same_unit_name: bool = match self.0.unitName.as_ref() {
            Some(query_unit_name) => service_unit_definition.serviceUnitName == *query_unit_name,
            None => true, // The caller doesn't care about service unit name matching
        };

        same_vm
            && same_unit_name
            && (service_unit_definition.identity.name.sdvPackageName == self.0.sdvPackageName)
            && (service_unit_definition.identity.name.serviceBundleName == self.0.serviceBundleName)
    }
}
