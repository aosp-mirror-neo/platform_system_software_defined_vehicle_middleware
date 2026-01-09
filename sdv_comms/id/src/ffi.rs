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

//! FFI declarations for the Comms Stack Identity library.

#![allow(unused_imports)]

mod service_fqin;
pub use service_fqin::ASDV_ServiceFqin;
pub use service_fqin::{
    ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
    ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
    ASDV_SERVICE_INSTANCE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
    ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL, ASDV_UNIT_TYPE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
    ASDV_VM_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
};

mod service_identity;
pub use service_identity::{ASDV_ServiceIdentity, ASDV_UnitIdentifier};
