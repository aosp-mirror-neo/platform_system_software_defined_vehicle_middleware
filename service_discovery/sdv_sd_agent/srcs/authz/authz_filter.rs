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

//! Authz filter function.

use sdv_authz::AuthzService;
use sdv_comms_id::ServiceFqin;

use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::ServiceUnitDefinition;

use std::sync::Arc;

/// AuthzFilter function for Service Unit Definition.
pub(crate) type AuthzFilterForSud = Arc<dyn Fn(&ServiceUnitDefinition) -> bool + Send + Sync>;

/// Creates Authz filter function for filtering ServiceUnitDefinition.
pub(crate) fn create_authz_filter_for_sud(
    caller_fqin: ServiceFqin,
    authz_service: Arc<AuthzService>,
) -> AuthzFilterForSud {
    let authz_service = authz_service.clone();

    Arc::new(move |sud: &ServiceUnitDefinition| {
        authz_service.is_authorized_for_sud(&caller_fqin, sud)
    })
}
