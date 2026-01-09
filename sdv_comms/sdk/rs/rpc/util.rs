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

use crate::sd::UnitType;

// Derives the gRPC service name from the service's unit type.
// This relies on the SDV package name and typename to form the fully qualified name (FQN)
// of the corresponding proto service.
pub(crate) fn get_service_name_from_unit_type(unit_type: &UnitType) -> String {
    let package_name = unit_type.get_sdv_package_name();
    let type_name = unit_type.get_unit_type_name();
    format!("{package_name}.{type_name}")
}
