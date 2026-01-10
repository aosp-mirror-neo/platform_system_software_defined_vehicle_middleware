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

//! Wrapper for IServiceUnitChangeListener.

use crate::authz::AuthzFilterForSud;

use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceUnitChangeListener::IServiceUnitChangeListener;
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::ServiceUnitDefinition;

/// ServiceUnitChangeListener is a listener which checks the ACLs of
/// Service Unit Definitions before sending them to the listener.
///
/// Service Unit Definition will not be send to the caller if
/// the caller is not authorized.
pub(crate) struct ServiceUnitChangeListener {
    authz_filter: AuthzFilterForSud,
    listener: binder::Strong<dyn IServiceUnitChangeListener>,
}

impl ServiceUnitChangeListener {
    /// Creates new listener.
    pub(crate) fn new(
        authz_filter: AuthzFilterForSud,
        listener: binder::Strong<dyn IServiceUnitChangeListener>,
    ) -> Self {
        ServiceUnitChangeListener { authz_filter, listener }
    }

    /// Calls listener's onServiceUnitRegistered method if the caller is authorized.
    pub(crate) fn on_service_unit_registered(
        &self,
        service_unit_definition: &ServiceUnitDefinition,
    ) -> binder::Result<()> {
        match (self.authz_filter)(service_unit_definition) {
            // Sending an event without waiting for a response from listener.
            true => self.listener.onServiceUnitRegistered(service_unit_definition),
            // Do not return error if not authorized as this is intended behavior.
            false => Ok(()),
        }
    }

    /// Calls listener's onServiceUnitUnregistered method if the caller is authorized.
    pub(crate) fn on_service_unit_unregistered(
        &self,
        service_unit_definition: &ServiceUnitDefinition,
    ) -> binder::Result<()> {
        match (self.authz_filter)(service_unit_definition) {
            // Sending an event without waiting for a response from listener.
            true => self.listener.onServiceUnitUnregistered(service_unit_definition),
            // Do not return error if not authorized as this is intended behavior.
            false => Ok(()),
        }
    }
}

// This is needed to avoid comparing authz_filter field.
impl PartialEq for ServiceUnitChangeListener {
    fn eq(&self, other: &Self) -> bool {
        self.listener == other.listener
    }
}

impl Clone for ServiceUnitChangeListener {
    fn clone(&self) -> Self {
        ServiceUnitChangeListener {
            authz_filter: self.authz_filter.clone(),
            listener: self.listener.clone(),
        }
    }
}
