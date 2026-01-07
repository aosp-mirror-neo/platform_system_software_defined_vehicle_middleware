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

use log::debug;
use sdv_comms::ContextRef;
use sdv_lifecycle_client::service_bundle::ServiceBundle;

/// Registers Authz service bundle to receive lifecycle events.
#[derive(Default)]
pub struct AuthzTestServiceBundle {}

impl ServiceBundle for AuthzTestServiceBundle {
    /// Creates new instance of the service bundle.
    fn new(_context: ContextRef) -> AuthzTestServiceBundle {
        debug!("AuthzTestServiceBundle::new() called");
        AuthzTestServiceBundle {}
    }

    fn on_start(&mut self) {
        debug!("AuthzTestServiceBundle::on_start() called");
    }

    fn on_stop(&mut self) {
        debug!("AuthzTestServiceBundle::on_stop() called");
    }
}

impl Drop for AuthzTestServiceBundle {
    fn drop(&mut self) {
        debug!("AuthzTestServiceBundle::drop() called");
    }
}
