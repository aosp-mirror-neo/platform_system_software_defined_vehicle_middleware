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

use async_trait::async_trait;
use binder_tokio::Tokio;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use google_sdv_authz::aidl::google::sdv::authz::IAuthzService::{
    BpAuthzService, IAuthzService, IAuthzServiceAsync,
};
use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinAidl;

use crate::id::ServiceFqin;
use crate::rpc::server::Authorizer;

#[derive(Clone)]
pub struct SdvAuthorizer {
    authz_service: binder::Strong<dyn IAuthzServiceAsync<Tokio>>,
    object_fqin_aidl: ServiceFqinAidl,
    object_service_unit_name: String,
}

impl SdvAuthorizer {
    pub async fn new(
        callee_fqin: &ServiceFqin,
        callee_service_unit_name: &String,
    ) -> SdvAuthorizer {
        let authz_service = get_authz_service().await;

        let object_fqin_aidl = ServiceFqinAidl {
            sdvVmName: callee_fqin.get_sdv_vm_name().to_string(),
            sdvPackageName: callee_fqin.get_sdv_package_name().to_string(),
            serviceBundleName: callee_fqin.get_service_bundle_name().to_string(),
            serviceInstanceName: callee_fqin.get_service_instance_name().to_string(),
        };

        SdvAuthorizer {
            authz_service,
            object_fqin_aidl,
            object_service_unit_name: callee_service_unit_name.to_string(),
        }
    }
}

#[async_trait]
impl Authorizer for SdvAuthorizer {
    async fn is_authorized(&self, subject_fqin: &ServiceFqin) -> SdvResult<bool> {
        let subject_fqin_aidl = ServiceFqinAidl {
            sdvVmName: subject_fqin.get_sdv_vm_name().to_string(),
            sdvPackageName: subject_fqin.get_sdv_package_name().to_string(),
            serviceBundleName: subject_fqin.get_service_bundle_name().to_string(),
            serviceInstanceName: subject_fqin.get_service_instance_name().to_string(),
        };

        self.authz_service
            .isAuthorized(
                &subject_fqin_aidl,
                &self.object_fqin_aidl,
                self.object_service_unit_name.as_str(),
            )
            .await
            .map_err(|e| {
                SdvStatus::with_cause_and_message(
                    SdvStatusCode::Internal,
                    e,
                    "authz_service request failed",
                )
            })
    }
}

async fn get_authz_service() -> binder::Strong<dyn IAuthzServiceAsync<Tokio>> {
    let descriptor = <BpAuthzService as IAuthzService>::get_descriptor().to_owned() + "/default";
    let binder_service =
        binder_tokio::wait_for_interface::<dyn IAuthzServiceAsync<Tokio>>(&descriptor);
    binder_service.await.expect("Something wrong with authz service")
}

/// A wrapper that adds caching to any type that implements the Authorizer trait.
#[derive(Clone)]
pub struct CachedAuthorizer<T: Authorizer> {
    wrapped_authorizer: T,
    // Caches ACL check results for gRPC clients. Entries in this
    // cache are assumed to be valid for the entire lifetime of the server
    // process. This is because, under the current APEX structure,
    // any changes to ACLs necessitate a server restart. An instance of
    // Authorizer always belongs to one server only.
    cache: Arc<RwLock<HashMap<ServiceFqin, bool>>>,
}

impl<T: Authorizer + std::marker::Sync> CachedAuthorizer<T> {
    pub fn new(authorizer: T) -> Self {
        Self { wrapped_authorizer: authorizer, cache: Arc::new(RwLock::new(HashMap::new())) }
    }
}

#[async_trait]
impl<T: Authorizer + std::marker::Sync> Authorizer for CachedAuthorizer<T> {
    async fn is_authorized(&self, subject_fqin: &ServiceFqin) -> SdvResult<bool> {
        {
            let cache_read_guard = self.cache.read().await;
            if let Some(&is_authorized) = cache_read_guard.get(subject_fqin) {
                return Ok(is_authorized);
            }
        } // The read lock is dropped here, allowing a write lock to be acquired.

        // If not in cache, proceed with authorization.
        let result = self.wrapped_authorizer.is_authorized(subject_fqin).await?;

        {
            let mut cache_write = self.cache.write().await;
            cache_write.insert(subject_fqin.clone(), result);
        } // The write lock is dropped here.

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // An authorizer that counts how many times its is_authorized method is called.
    struct CountingAuthorizer {
        allowed_fqins: HashMap<ServiceFqin, SdvResult<bool>>,
        call_count: AtomicUsize,
    }

    impl CountingAuthorizer {
        fn new(fqins: HashMap<ServiceFqin, SdvResult<bool>>) -> Self {
            Self { allowed_fqins: fqins, call_count: AtomicUsize::new(0) }
        }

        pub fn get_call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl Authorizer for CountingAuthorizer {
        async fn is_authorized(&self, fqin: &ServiceFqin) -> SdvResult<bool> {
            self.call_count.fetch_add(1, Ordering::SeqCst);

            match self.allowed_fqins.get(fqin) {
                Some(Ok(is_authorized)) => Ok(*is_authorized),
                Some(Err(e)) => Err(SdvStatus::new(e.status_code())),
                None => Ok(false),
            }
        }
    }

    // Helper to create a ServiceFqin for tests using the FromStr implementation.
    fn fqin(name: &str) -> ServiceFqin {
        ServiceFqin::from_str(name).unwrap()
    }

    #[tokio::test]
    async fn cache_hit_for_authorized_fqin() {
        let mut allowed_fqins = HashMap::new();
        let fqin1 = fqin("vm:com.google.sdv.Service1/instance1");
        allowed_fqins.insert(fqin1.clone(), Ok(true));

        let counting_authorizer = CountingAuthorizer::new(allowed_fqins);
        let cached_authorizer = CachedAuthorizer::new(counting_authorizer);

        assert!(cached_authorizer.is_authorized(&fqin1).await.unwrap());
        assert_eq!(cached_authorizer.wrapped_authorizer.get_call_count(), 1);

        assert!(cached_authorizer.is_authorized(&fqin1).await.unwrap());
        assert_eq!(cached_authorizer.wrapped_authorizer.get_call_count(), 1);
    }

    #[tokio::test]
    async fn cache_hit_for_unauthorized_fqin() {
        let mut allowed_fqins = HashMap::new();
        let fqin_denied = fqin("vm:com.google.sdv.Service1/instance2");
        allowed_fqins.insert(fqin_denied.clone(), Ok(false));

        let counting_authorizer = CountingAuthorizer::new(allowed_fqins);
        let cached_authorizer = CachedAuthorizer::new(counting_authorizer);

        assert!(!cached_authorizer.is_authorized(&fqin_denied).await.unwrap());
        assert_eq!(cached_authorizer.wrapped_authorizer.get_call_count(), 1);

        assert!(!cached_authorizer.is_authorized(&fqin_denied).await.unwrap());
        assert_eq!(cached_authorizer.wrapped_authorizer.get_call_count(), 1);
    }

    #[tokio::test]
    async fn different_fqins_are_cached_independently() {
        let mut allowed_fqins = HashMap::new();
        let fqin1 = fqin("vm:com.google.sdv.Service1/instance1");
        let fqin2 = fqin("vm:com.google.sdv.Service2/instance2");
        allowed_fqins.insert(fqin1.clone(), Ok(true));
        allowed_fqins.insert(fqin2.clone(), Ok(false));

        let counting_authorizer = CountingAuthorizer::new(allowed_fqins);
        let cached_authorizer = CachedAuthorizer::new(counting_authorizer);

        assert!(cached_authorizer.is_authorized(&fqin1).await.unwrap());
        assert_eq!(cached_authorizer.wrapped_authorizer.get_call_count(), 1);

        assert!(!cached_authorizer.is_authorized(&fqin2).await.unwrap());
        assert_eq!(cached_authorizer.wrapped_authorizer.get_call_count(), 2);

        assert!(cached_authorizer.is_authorized(&fqin1).await.unwrap());
        assert_eq!(cached_authorizer.wrapped_authorizer.get_call_count(), 2);

        assert!(!cached_authorizer.is_authorized(&fqin2).await.unwrap());
        assert_eq!(cached_authorizer.wrapped_authorizer.get_call_count(), 2);
    }

    #[tokio::test]
    async fn cached_authorizer_does_not_cache_errors() {
        let mut allowed_fqins = HashMap::new();
        let fqin_error = fqin("vm:com.google.sdv.Service_err/instance1");
        let fqin_success = fqin("vm:com.google.sdv.Service_ok/instance1");
        allowed_fqins.insert(fqin_error.clone(), Err(SdvStatus::new(SdvStatusCode::Internal)));
        allowed_fqins.insert(fqin_success.clone(), Ok(true));

        let counting_authorizer = CountingAuthorizer::new(allowed_fqins);
        let cached_authorizer = CachedAuthorizer::new(counting_authorizer);

        assert!(cached_authorizer.is_authorized(&fqin_error).await.is_err());
        assert_eq!(cached_authorizer.wrapped_authorizer.get_call_count(), 1);

        assert!(cached_authorizer.is_authorized(&fqin_error).await.is_err());
        assert_eq!(cached_authorizer.wrapped_authorizer.get_call_count(), 2);

        assert!(cached_authorizer.is_authorized(&fqin_success).await.is_ok());
        assert_eq!(cached_authorizer.wrapped_authorizer.get_call_count(), 3);

        assert!(cached_authorizer.is_authorized(&fqin_success).await.is_ok());
        assert_eq!(cached_authorizer.wrapped_authorizer.get_call_count(), 3);
    }
}
