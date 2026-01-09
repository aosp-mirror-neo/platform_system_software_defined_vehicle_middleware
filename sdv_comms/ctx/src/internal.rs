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

use crate::agents::SdvAgents;
use crate::executor::{self, AsyncExecutor};

use google_sdv_comms::aidl::google::sdv::comms::ContextInitializationToken::ContextInitializationToken;
use sdv_comms_id::{register_new_identity, ServiceFqin, ServiceIdentity, ServicePublicKey};
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};

/// An implementation of the internals of Context object.
/// This object contains fields owned by the Context Object that are not directly exposed in
/// the Context Public APIs.
pub struct ContextImpl {
    /// Identity of the Service Bundle for which this Context was created.
    identity: ServiceIdentity,

    /// The private key that idenitfies the Service Bundle for which this Context was created.
    /// TODO: Replace with well formed types.
    priv_key: [u8; 32],

    /// Container for SDV IPC connections to respective Comms Stack Agent instances.
    agents: SdvAgents,

    // Container for a reference to an Executor that can spawn async tasks.
    executor: Box<dyn AsyncExecutor>,
}

/// Context Object API for retrieving the SDK internal state of the Service Bundle to which this
/// Context Object attached. This API is internal and build restricted to the SDV Comms Stack and
/// MUST NOT be used as the Public API surface.
///
/// # Concurrency
/// NON_BLOCKING: All functions defined by this trait do not block and return immediately.
impl ContextImpl {
    /// Creates and returns a new Context using the provided identity.
    pub fn new(init: ContextInitializationToken) -> SdvResult<ContextImpl> {
        let priv_key = [0; 32];
        let agents = SdvAgents::new(&init)?;
        let identity = init.identity.try_into().map_err(|_| {
            SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                "Failed to parse ServiceIdentity AIDL value",
            )
        })?;
        let executor = executor::start()?;

        Ok(ContextImpl { identity, priv_key, agents, executor })
    }

    /// Returns the Identity of the Service Bundle for which this Context was created.
    pub fn get_identity(&self) -> &ServiceIdentity {
        &self.identity
    }

    /// Returns private key to be used for cryptographic identification of the Service Bundle for
    /// which this Context was created.
    pub fn get_priv_key(&self) -> &[u8] {
        &self.priv_key
    }

    /// Returns a Binder proxy for Service Discovery Agent.
    pub fn get_agents(&self) -> &SdvAgents {
        &self.agents
    }

    /// Returns the executor for the async runtime.
    pub fn get_executor(&self) -> &dyn AsyncExecutor {
        self.executor.as_ref()
    }
}

/// Creates and initialises a new Context object.
pub fn initialise(fqin: ServiceFqin) -> SdvResult<ContextImpl> {
    let (priv_key, pub_key) = ServicePublicKey::forge();

    let identity = register_new_identity(fqin, pub_key)?;
    let agents = SdvAgents::new_static()?;
    let executor = executor::start()?;
    Ok(ContextImpl { identity, priv_key, agents, executor })
}
