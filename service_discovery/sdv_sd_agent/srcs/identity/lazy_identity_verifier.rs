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

use anyhow::anyhow;
use binder::{DeathRecipient, IBinder};
use std::sync::{Arc, Mutex};

use google_sdv_service_bundles_identity::aidl::google::sdv::service_bundles::identity::IIdentityVerifier::{
    BpIdentityVerifier, IIdentityVerifier,
};

#[cfg_attr(test, mockall::automock)]
pub(crate) trait IdentityVerifierProvider: Send + Sync {
    fn get(&self) -> anyhow::Result<binder::Strong<dyn IIdentityVerifier>>;
}

/// Caches a binder to the `IIdentityVerifier` service.
///
/// The binder is retrieved on the first call to `get()` and cached for subsequent calls.
/// If the `IIdentityVerifier` service dies, the cached binder is cleared and a new one is
/// retrieved on the next call to `get()`.
// TODO: b/442559325 - use SDV's persistent_binder instead.
#[derive(Default)]
pub(crate) struct LazyIdentityVerifier {
    state: Arc<Mutex<Option<LazyIdentityVerifierState>>>,
}

struct LazyIdentityVerifierState {
    identity_verifier: binder::Strong<dyn IIdentityVerifier>,
    // Keep the death recipient alive to receive notifications.
    #[allow(dead_code)]
    death_recipient: DeathRecipient,
}

impl IdentityVerifierProvider for LazyIdentityVerifier {
    fn get(&self) -> anyhow::Result<binder::Strong<dyn IIdentityVerifier>> {
        let mut state_guard = self.state.lock().unwrap();
        if let Some(state) = &*state_guard {
            return Ok(state.identity_verifier.clone());
        }

        let verifier = Self::create_identity_verifier()?;
        let death_recipient = self.create_death_recipient(&verifier)?;

        *state_guard = Some(LazyIdentityVerifierState {
            identity_verifier: verifier.clone(),
            death_recipient,
        });

        Ok(verifier)
    }
}

impl LazyIdentityVerifier {
    fn create_identity_verifier() -> anyhow::Result<binder::Strong<dyn IIdentityVerifier>> {
        let descriptor =
            <BpIdentityVerifier as IIdentityVerifier>::get_descriptor().to_owned() + "/default";
        // check_interface() doesn't block if the IIdentityVerifier is not available.
        Ok(binder::check_interface::<dyn IIdentityVerifier>(&descriptor)?)
    }

    fn create_death_recipient(
        &self,
        verifier: &binder::Strong<dyn IIdentityVerifier>,
    ) -> anyhow::Result<DeathRecipient> {
        // Maintaining only a weak pointer in the closure to allow the destruction of the Arc in
        // the state when the LazyIdentityVerifier object is destroyed.
        let weak_state = Arc::downgrade(&self.state);
        let mut recipient = DeathRecipient::new(Box::new(move || {
            log::info!("IIdentityVerifier has died, clearing cached binder.");
            if let Some(state) = weak_state.upgrade() {
                *state.lock().unwrap() = None;
                log::info!("Cleared cached binder.");
            }
        }));

        verifier
            .as_binder()
            .link_to_death(&mut recipient)
            .map_err(|e| anyhow!("Failed to link to death for IIdentityVerifier: {e}"))?;

        Ok(recipient)
    }
}
