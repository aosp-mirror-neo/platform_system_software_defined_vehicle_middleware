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

use bytemuck::TransparentWrapper;
use sdv_comms_bindgen::{ASDV_Context, ASDV_ServiceFqin, ASDV_ServiceIdentity};
use sdv_status_rs::{SdvResult, SdvStatus};
use std::default::Default;

use crate::id::{ServiceFqin, ServiceIdentity};

/// The single entry point for all Comms Stack APIs.
///
/// ContextRef is a reference to a pointer provided by the Context library implementation.
///
/// # Concurrency
/// IMMUTABLE: All functions defined on this object MUST NOT mutate internal state.
/// NON_BLOCKING: All functions defined on this object MUST NOT block and return immediately.
#[allow(dead_code)]
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct ContextRef(&'static ASDV_Context);

// SAFETY: Since [ContextRef] is an effective singleton from the Service Bundle's perspective, it
// is safe to transfer the pointer from one thread to another. This pointer is promised to be alive
// for at least as long as the Service Bundle is alive; as guaranteed by the SDV LifecycleManager
// which executes the Service Bundle.
unsafe impl Send for ContextRef {}

// SAFETY: Since the Context object is effectively immutable, [ContextRef] is safe to be accessed
// from multiple threads at the same time. Also note that this pointer is promised to be alive for
// at least as long as the Service Bundle is alive; as guaranteed by the SDV LifecycleManager
// which executes the Service Bundle.
unsafe impl Sync for ContextRef {}

impl ContextRef {
    /// Creates a new ref from the raw pointer.
    pub fn from_raw(ptr: &'static ASDV_Context) -> ContextRef {
        ContextRef(ptr)
    }

    /// Creates a new ContextRef.
    /// This API is only provided as a stop-gap for until Service Lifecycle APIs are ready.
    /// TODO(b/329859325): Remove this function when the Service Lifecycle functionality is available.
    pub fn create(fqin: ServiceFqin) -> ContextRef {
        let fqin_ref = ServiceFqin::peel_ref(&fqin);
        // Temporary fake factory function that returns a pointer.
        // Will be substituted once migration to SDV LifecycleManager is completed.
        // SAFETY: The returned pointer is valid for the lifetime of the program, and hence can be
        // assigned `static lifetime.
        let ctx_ref = unsafe {
            let ctx_ptr = sdv_comms_bindgen::ASDV_Context_temporaryCreate(fqin_ref);
            &(*ctx_ptr)
        };
        ContextRef(ctx_ref)
    }

    /// Public API. Returns the Identity of the Service Bundle for which this Context was created.
    pub fn get_self_identity(&self) -> ServiceIdentity {
        let mut asdv_identity = ASDV_ServiceIdentity::default();
        // SAFETY: The lifecycle for the returned Identity value is managed by [ServiceIdentity]
        // struct and `self` contains a valid pointer to ASDV_Context.
        unsafe { sdv_comms_bindgen::ASDV_Context_getSelfIdentity(self.0, &mut asdv_identity) };
        ServiceIdentity::wrap(asdv_identity)
    }

    /// Returns ASDV_Context
    #[inline]
    pub fn as_raw(&self) -> *const ASDV_Context {
        self.0 as _
    }

    /// Public API. Returns the FQIN of the Service Bundle for which this Context was created.
    #[inline]
    pub fn get_self_fqin(&self) -> ServiceFqin {
        let mut asdv_fqin = ASDV_ServiceFqin::default();
        // SAFETY: The lifecycle for the returned Identity value is managed by [ServiceFqin]
        // struct and `self`` contains a valid pointer to ASDV_Context.
        unsafe { sdv_comms_bindgen::ASDV_Context_getSelfFqin(self.0, &mut asdv_fqin) };
        ServiceFqin::wrap(asdv_fqin)
    }

    // TODO: b/400635525 - Remove this once API breaks are possible.
    /// Unused
    #[inline]
    pub fn get_pub_key(&self) -> &[u8] {
        unimplemented!()
    }
}

/// Privileged API.
/// Waits (and blocks current thread) for SDV Comms Agents to be available, or start them if any
/// is configured as a dynamic service and isn't yet started.
pub fn wait_for_agents() -> SdvResult<()> {
    // SAFETY: No memory sharing occurs and hence this call is safe to make.
    let status = unsafe { sdv_comms_bindgen::ASDV_Context_waitForAgents() };
    SdvStatus::from_raw_code(status).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr;

    #[test]
    fn test_context_temporary_create() {
        let fqin = ServiceFqin::from_str("vm-name:com.google.testpkg.ServiceName/instance")
            .expect("ServiceFqin should not be malformed");
        let fqin_clone = fqin.clone();

        wait_for_agents().expect("SDV Comms Agents not ready");

        let ctx = ContextRef::create(fqin);

        // assert_ne!(ctx, ptr::null());
        let identity = ctx.get_self_identity();
        assert_eq!(identity.get_fqin().get_sdv_package_name(), fqin_clone.get_sdv_package_name());
        assert_eq!(
            identity.get_fqin().get_service_bundle_name(),
            fqin_clone.get_service_bundle_name()
        );
        assert_eq!(
            identity.get_fqin().get_service_instance_name(),
            fqin_clone.get_service_instance_name()
        );
    }
}
