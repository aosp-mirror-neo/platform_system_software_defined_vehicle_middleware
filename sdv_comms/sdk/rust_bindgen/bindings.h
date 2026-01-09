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

// This file is required for rust_bindgen to work properly.
// Ref: https://rust-lang.github.io/rust-bindgen/tutorial-2.html
// This file collects all C Headers exposed by libsdv_comms_id.

// SDV Context library headers
#include "sdv/ctx.h"

// Service Identity library headers
#include "sdv/service_fqin.h"
#include "sdv/service_identity.h"

// Service Discovery library headers
#include "sdv/unit_discovery.h"
#include "sdv/unit_registration.h"
#include "sdv/unit_type.h"

// Data Tunnel library header
#include "libsdv_comms_dt.h"
