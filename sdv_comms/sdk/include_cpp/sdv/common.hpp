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

#pragma once

/**
 * Forward declaration of external types.
 * Must be removed after integration with the rest part of Comm Stack API.
 *
 * The placeholder conversion functions should be replaced by the actual
 * API, when the external types are implemented.
 */

namespace sdv_comms {

struct Context {};
const ASDV_Context* ContextIntoRaw(const Context&);

namespace id {

struct ServiceUnitId {};
const ASDV_ServiceUnitId* ServiceUnitIdIntoRaw(const ServiceUnitId&);

}  // namespace id

namespace sd {

struct UnitRegistrationArgs {};
const ASDV_UnitRegistrationArgs* UnitRegistrationArgsIntoRaw(const UnitRegistrationArgs&);

}  // namespace sd

}  // namespace sdv_comms
