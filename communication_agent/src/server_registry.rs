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

#![allow(dead_code)] // Currently this code is used by tests only.

use sdv_comms::id::ServiceFqin;
use std::collections::HashMap;
use std::str::FromStr;

pub(crate) trait IdentityVerifier {
    fn verify(&self, new_server_request: &ServerRegistrationInfo) -> bool;
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) struct ServerRegistrationInfo {
    pid: i32,
    uid: u32,
    fqin: ServiceFqin,
    endpoint_name: String,
}

/// The token that is used for identification of the exact server instance.
/// It should be kept in secret. Only the server process and the agent may
/// know it.
/// The UID of the process that pass a token to the agent must be equal to
/// the UID of the process that has initially received the token from the agent.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub(crate) struct ServerToken(u128);

#[derive(Debug, PartialEq)]
pub(crate) struct AddServerResult {
    pub(crate) new_server_token: ServerToken,
    pub(crate) replaced_server_token: Option<ServerToken>, // The old token is necessary to cancel the pending requests.
}

/// Contains info about all the registered servers.
pub(crate) struct ServerRegistry<V: IdentityVerifier> {
    servers: HashMap<ServerToken, ServerRegistrationInfo>,
    verifier: V,
}

#[derive(Debug)]
pub(crate) enum ServerRegistryError {
    ServerIdentityIsNotConfirmed,
}

impl<V: IdentityVerifier> ServerRegistry<V> {
    pub(crate) fn new(verifier: V) -> Self {
        Self { servers: HashMap::new(), verifier }
    }

    /// Adds a new server to the registry. Before adding verifies that the
    /// OS process corresponds to the claimed FQIN.
    pub(crate) fn add_server(
        &mut self,
        new_server_request: ServerRegistrationInfo,
    ) -> Result<AddServerResult, ServerRegistryError> {
        if !self.verifier.verify(&new_server_request) {
            return Err(ServerRegistryError::ServerIdentityIsNotConfirmed);
        }

        let old_server_token = self.servers.iter().find_map(|(token, server_info)| {
            if server_info.fqin == new_server_request.fqin
                && server_info.endpoint_name == new_server_request.endpoint_name
            {
                Some(*token)
            } else {
                None
            }
        });

        old_server_token.map(|token| self.servers.remove(&token));

        let token = ServerToken(rand::random::<u128>());
        if self.servers.contains_key(&token) {
            // This is extremely unlikely to happen. The probability of at least one
            // collision between 1000 random numbers of the 128 bit length
            // is estimated as 1.5 * 10^-33.
            panic!("FATAL: ServerToken collision detected.");
        }
        self.servers.insert(token, new_server_request.clone());
        Ok(AddServerResult { new_server_token: token, replaced_server_token: old_server_token })
    }

    pub(crate) fn count(&self) -> usize {
        self.servers.len()
    }

    pub(crate) fn get_registration_info(
        &self,
        token: &ServerToken,
    ) -> Option<&ServerRegistrationInfo> {
        self.servers.get(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeIdentityVerifier {
        verification_succeeds: bool,
    }

    impl FakeIdentityVerifier {
        fn new(verification_succeeds: bool) -> Self {
            Self { verification_succeeds }
        }
    }

    impl IdentityVerifier for FakeIdentityVerifier {
        fn verify(&self, _new_server_request: &ServerRegistrationInfo) -> bool {
            self.verification_succeeds
        }
    }

    const SERVICE_FQIN: &str = "sdv-vm:com.sdv.google.Bundle/instance";

    #[test]
    fn test_registry_initialization() {
        let verifier = FakeIdentityVerifier::new(true);
        let registry = ServerRegistry::new(verifier);
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_add_server_updates_count_when_verified() {
        let fqin = ServiceFqin::from_str(SERVICE_FQIN)
            .unwrap_or_else(|_| panic!("Failed to parse FQIN '{SERVICE_FQIN}'."));

        let verifier = FakeIdentityVerifier::new(true);
        let mut registry = ServerRegistry::new(verifier);
        let result = registry.add_server(ServerRegistrationInfo {
            pid: 123,
            uid: 456,
            fqin,
            endpoint_name: "default".to_string(),
        });
        assert!(result.is_ok());
        assert!(result.unwrap().replaced_server_token.is_none());
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn test_add_server_does_not_update_count_when_not_verified() {
        let fqin = ServiceFqin::from_str(SERVICE_FQIN)
            .unwrap_or_else(|_| panic!("Failed to parse FQIN '{SERVICE_FQIN}'."));
        let verifier = FakeIdentityVerifier::new(false);
        let mut registry = ServerRegistry::new(verifier);
        let result = registry.add_server(ServerRegistrationInfo {
            pid: 1,
            uid: 1,
            fqin,
            endpoint_name: "default".to_string(),
        });
        assert!(result.is_err());
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_add_server_with_existing_fqin_replaces_and_returns_old() {
        let fqin = ServiceFqin::from_str(SERVICE_FQIN)
            .unwrap_or_else(|_| panic!("Failed to parse FQIN '{SERVICE_FQIN}'."));

        let verifier = FakeIdentityVerifier::new(true);
        let mut registry = ServerRegistry::new(verifier);

        // Add the first server
        let old_server_info = ServerRegistrationInfo {
            pid: 1,
            uid: 1,
            fqin: fqin.clone(),
            endpoint_name: "default".to_string(),
        };
        let old_token = registry.add_server(old_server_info).unwrap().new_server_token;

        assert_eq!(registry.count(), 1);

        // Add a new server with the same FQIN
        let new_server_info =
            ServerRegistrationInfo { pid: 2, uid: 2, fqin, endpoint_name: "default".to_string() };
        let result = registry.add_server(new_server_info);

        assert!(result.is_ok());
        let add_result = result.unwrap();
        assert_ne!(add_result.new_server_token, old_token, "A new token should be generated.");
        assert_eq!(
            add_result.replaced_server_token,
            Some(old_token),
            "The token of the old server should be returned."
        );
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn test_add_server_with_same_fqin_and_different_endpoint_does_not_replace() {
        let fqin = ServiceFqin::from_str(SERVICE_FQIN)
            .unwrap_or_else(|_| panic!("Failed to parse FQIN '{SERVICE_FQIN}'."));

        let verifier = FakeIdentityVerifier::new(true);
        let mut registry = ServerRegistry::new(verifier);

        // Add the first server
        let old_server_info = ServerRegistrationInfo {
            pid: 1,
            uid: 1,
            fqin: fqin.clone(),
            endpoint_name: "endpoint1".to_string(),
        };
        registry.add_server(old_server_info).unwrap();
        assert_eq!(registry.count(), 1);

        // Add a new server with the same FQIN but different endpoint
        let new_server_info =
            ServerRegistrationInfo { pid: 2, uid: 2, fqin, endpoint_name: "endpoint2".to_string() };
        let result = registry.add_server(new_server_info);

        assert!(result.is_ok());
        assert!(result.unwrap().replaced_server_token.is_none());
        assert_eq!(registry.count(), 2);
    }

    #[test]
    fn test_get_registration_info_with_valid_token_returns_info() {
        let fqin = ServiceFqin::from_str(SERVICE_FQIN)
            .unwrap_or_else(|_| panic!("Failed to parse FQIN '{SERVICE_FQIN}'."));

        let verifier = FakeIdentityVerifier::new(true);
        let mut registry = ServerRegistry::new(verifier);

        let server_info = ServerRegistrationInfo {
            pid: 123,
            uid: 456,
            fqin,
            endpoint_name: "default".to_string(),
        };
        let result = registry.add_server(server_info.clone()).unwrap();

        let retrieved_info = registry.get_registration_info(&result.new_server_token);
        assert_eq!(retrieved_info, Some(&server_info));
    }

    #[test]
    fn test_get_registration_info_with_invalid_token_returns_none() {
        let verifier = FakeIdentityVerifier::new(true);
        let registry = ServerRegistry::new(verifier);
        let invalid_token = ServerToken(12345u128);
        assert!(registry.get_registration_info(&invalid_token).is_none());
    }
}
