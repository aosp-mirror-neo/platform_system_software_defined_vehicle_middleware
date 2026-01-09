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

//! Crate that implements the SDV SDK for Service Identity and Service Fqin.
#![allow(dead_code)]

pub mod ffi;
pub mod names;

mod service_fqin;
pub use service_fqin::{
    ServiceFqin, ServiceFqinBuilder, ServiceFqinValidationError, SERVICE_FQIN_MAX_LENGTH_BYTES,
};

mod service_identity;
pub use service_identity::{
    get_binder_caller_process_identity, register_new_identity, ServiceIdentity, ServicePublicKey,
    SERVICE_PUBLIC_KEY_SIZE_BYTES,
};

mod unit_identifier;
pub use unit_identifier::UnitIdentifier;
