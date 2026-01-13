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

#![allow(non_camel_case_types)]
#![cfg_attr(not(comms_stack_ffi), allow(unused))]

use sdv_comms_common::{ptr, try_status};
use sdv_comms_id::ffi::{ASDV_ServiceFqin, ASDV_ServiceIdentity};
use sdv_comms_stack_ffi_macro::{comms_stack_ffi_fn, comms_stack_ffi_struct};
use sdv_status_rs::{sdv_status_t, SdvStatus, SdvStatusCode};

#[cfg(comms_stack_ffi)]
use sdv_comms_id::ServiceFqin;

#[cfg(not(comms_stack_stub))]
use crate::{agents, ContextImpl};

/// Privileged API.
/// Waits (and blocks current thread) for SDV Comms Agents to be available, or start them if any
/// is configured as a dynamic service and isn't yet started.
///
/// # Safety
/// None. This function needs to be marked unsafe because it is called from a C context.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_Context_waitForAgents() -> sdv_status_t {
    try_status! {
        agents::wait_for_agents()
    }
}

/// Struct declaration for Comms Stack Context object.
#[comms_stack_ffi_struct(opaque)]
pub struct ASDV_Context(ContextImpl);

/// Temporary fake factory function.
/// Will be removed once migration to SDV LifecycleManager is completed.
///
/// # Safety
/// The caller must NOT dealloc or destroy the returned variable.
#[comms_stack_ffi_fn]
#[allow_reference]
#[no_mangle]
pub unsafe extern "C" fn ASDV_Context_temporaryCreate(
    #[allow_ptr] fqin: *const ASDV_ServiceFqin,
) -> &'static ASDV_Context {
    // SAFETY: fqin is expected to be a valid pointer to `ASDV_ServiceFqin`.
    let fqin = unsafe { fqin.as_ref().expect("nullptr provided") };
    let fqin = ServiceFqin::try_from(fqin).expect("Invalid ASDV_ServiceFqin provided");
    let ctx = crate::initialise(fqin).expect("Error initialising Context object");

    // SAFETY: We want the Context object to live for the remainder of the program, and hence we
    // can leak it from here to avoid de-allocating the object.
    Box::leak(Box::new(ASDV_Context(ctx)))
}

/// Fetches the Identity of the process which is associated with Context object.
///
/// # Safety
/// The caller is responsible for creating and destroying the lifetime of the ASDV_ServiceIdentity
/// object. The caller is additionally responsible for ensuring that ctx is a valid pointer to an
/// ASDV_Context instance.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_Context_getSelfIdentity(
    ctx: &'static ASDV_Context,
    asdv_identity: &mut ASDV_ServiceIdentity,
) -> sdv_status_t {
    try_status! {
        let identity = ctx.0.get_identity();
        identity.clone_into(asdv_identity).map_err(|_| SdvStatus::with_message(
            SdvStatusCode::Internal,
            format!("Invalid ASDV_ServiceIdentity {identity:?} provided for cloning"))
        )
    }
}

/// Fetches the Fully Qualified Name of the process which is associated with Context object.
///
/// # Safety
/// The caller is responsible for creating managing the lifetime of the ASDV_ServiceFqin
/// object. The caller is additionally responsible for ensuring that ctx is a valid pointer to an
/// ASDV_Context instance.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_Context_getSelfFqin(
    ctx: &'static ASDV_Context,
    asdv_fqin: &mut ASDV_ServiceFqin,
) -> sdv_status_t {
    try_status! {
        let fqin = ctx.0.get_identity().fqin();
        fqin.clone_into(asdv_fqin).map_err(|_| SdvStatus::with_message(
            SdvStatusCode::Internal,
            format!("Invalid ASDV_ServiceFqin {fqin:?} provided for cloning")))
    }
}
