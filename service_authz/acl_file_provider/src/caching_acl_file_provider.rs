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

//! SingleVmAclPolicyProvider implementation that caches ACLs read from
//! textproto files.

use std::sync::Mutex;
use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use tracing::instrument;

use sdv_authz::{
    AclPolicyModel, LocallyStoredAuthzPoliciesProvider, SingleVmAclPolicyProvider,
    VmAuthzPolicyModel, VmLevelPermissionsProvider,
};
use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
use sdv_authz_policy_proto_rust::vm_authz_policy::VmAuthzPolicy as VmAuthzPolicyProto;
use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName, VmName};

/// LocalVmAuthzPolicyProvider that caches ACLs read from textproto files.
///
/// CachingAclFileProvider should be used in the environment where ACL files are
/// not changing dynamically.
pub struct CachingAclFileProvider {
    inner_provider: Arc<dyn LocallyStoredAuthzPoliciesProvider + Send + Sync>,
    proto_cache: Mutex<HashMap<(PackageName, Option<ServiceBundleName>), AclPolicyProto>>,
    model_cache: Mutex<HashMap<(PackageName, Option<ServiceBundleName>), Arc<AclPolicyModel>>>,
    vm_proto_cache: Mutex<HashMap<VmName, VmAuthzPolicyProto>>,
    vm_model_cache: Mutex<HashMap<VmName, Arc<VmAuthzPolicyModel>>>,
}

impl CachingAclFileProvider {
    /// Create a CachingAclFileProvider with the given inner provider.
    pub fn new(inner_provider: Arc<dyn LocallyStoredAuthzPoliciesProvider + Send + Sync>) -> Self {
        Self {
            inner_provider,
            proto_cache: Mutex::new(HashMap::new()),
            model_cache: Mutex::new(HashMap::new()),
            vm_proto_cache: Mutex::new(HashMap::new()),
            vm_model_cache: Mutex::new(HashMap::new()),
        }
    }
}

impl LocallyStoredAuthzPoliciesProvider for CachingAclFileProvider {}

impl SingleVmAclPolicyProvider for CachingAclFileProvider {
    /// Gets the Service Bundle ACL policy proto.
    ///
    /// If the ACL policy proto is already cached, returns it from the cache.
    /// Otherwise, gets the ACL policy proto from the inner provider, caches it,
    /// and returns it.
    #[instrument(name = "authz_perf::cache_provider::get_acl_policy_proto", skip_all)]
    fn get_acl_policy_proto(
        &self,
        package_name: &PackageName,
        service_bundle_name: Option<&ServiceBundleName>,
    ) -> anyhow::Result<AclPolicyProto> {
        let key = (package_name.clone(), service_bundle_name.cloned());
        {
            let proto_cache = self.proto_cache.lock().unwrap();
            if let Some(acl_policy) = proto_cache.get(&key) {
                return Ok(acl_policy.clone());
            }
        }

        let acl_policy_proto = self
            .inner_provider
            .get_acl_policy_proto(package_name, service_bundle_name)
            .with_context(|| {
                format!(
                    "{package_name}{}: Failed to get ACL policy proto",
                    match service_bundle_name {
                        Some(name) => format!(".{}", name),
                        None => String::new(),
                    }
                )
            })?;

        {
            let mut proto_cache = self.proto_cache.lock().unwrap();
            proto_cache.insert(key, acl_policy_proto.clone());
        }

        Ok(acl_policy_proto)
    }

    /// Gets the Service Bundle ACL policy.
    ///
    /// If the ACL policy is already cached, returns it from the cache.
    /// Otherwise, gets the ACL policy from the inner provider, caches it,
    /// and returns it.
    fn get_acl_policy(
        &self,
        package_name: &PackageName,
        service_bundle_name: Option<&ServiceBundleName>,
    ) -> Result<Arc<AclPolicyModel>, anyhow::Error> {
        let key = (package_name.clone(), service_bundle_name.cloned());
        {
            let model_cache = self.model_cache.lock().unwrap();
            if let Some(acl_policy) = model_cache.get(&key) {
                return Ok(acl_policy.clone());
            }
        }

        let acl_policy_proto: AclPolicyProto =
            self.get_acl_policy_proto(package_name, service_bundle_name)?;

        let acl_policy_model = AclPolicyModel::try_from(acl_policy_proto).with_context(|| {
            format!(
                "{package_name}{}: Failed to parse ACL policy from proto",
                match service_bundle_name {
                    Some(name) => format!(".{}", name),
                    None => String::new(),
                }
            )
        })?;

        let acl_policy_model = Arc::new(acl_policy_model);

        {
            let mut model_cache = self.model_cache.lock().unwrap();
            model_cache.insert(key, acl_policy_model.clone());
        }

        Ok(acl_policy_model)
    }

    /// Returns a vector of (Package Name, Service Bundle Name) pairs
    /// representing the service bundles available on the local VM with
    /// available ACL policy.
    fn get_namespace_with_acl_policy(
        &self,
    ) -> Result<Vec<(PackageName, Option<ServiceBundleName>)>, anyhow::Error> {
        self.inner_provider.get_namespace_with_acl_policy()
    }
}

impl VmLevelPermissionsProvider for CachingAclFileProvider {
    /// Gets the VM Authz policy proto.
    ///
    /// If the VM Authz policy proto is already cached, returns it from the cache.
    /// Otherwise, gets the VM Authz policy proto from the inner provider, caches it,
    /// and returns it.
    fn get_vm_authz_policy_proto(
        &self,
        vm_name: &VmName,
    ) -> Result<VmAuthzPolicyProto, anyhow::Error> {
        let key = vm_name.clone();
        {
            let vm_proto_cache = self.vm_proto_cache.lock().unwrap();
            if let Some(vm_authz_policy) = vm_proto_cache.get(&key) {
                return Ok(vm_authz_policy.clone());
            }
        }

        let vm_authz_policy_proto = self
            .inner_provider
            .get_vm_authz_policy_proto(vm_name)
            .with_context(|| format!("{vm_name}: Failed to get VM Authz policy proto"))?;

        {
            let mut vm_proto_cache = self.vm_proto_cache.lock().unwrap();
            vm_proto_cache.insert(key, vm_authz_policy_proto.clone());
        }

        Ok(vm_authz_policy_proto)
    }

    fn get_vm_authz_policy(
        &self,
        vm_name: &VmName,
    ) -> Result<Arc<VmAuthzPolicyModel>, anyhow::Error> {
        let key = vm_name.clone();
        {
            let vm_model_cache = self.vm_model_cache.lock().unwrap();
            if let Some(vm_authz_policy) = vm_model_cache.get(&key) {
                return Ok(vm_authz_policy.clone());
            }
        }

        let vm_authz_policy_proto = self.get_vm_authz_policy_proto(vm_name)?;

        let vm_authz_policy_model = VmAuthzPolicyModel::try_from(vm_authz_policy_proto)
            .with_context(|| format!("{vm_name}: Failed to parse VM Authz policy from proto"))?;

        let vm_authz_policy_model = Arc::new(vm_authz_policy_model);

        {
            let mut vm_model_cache = self.vm_model_cache.lock().unwrap();
            vm_model_cache.insert(key, vm_authz_policy_model.clone());
        }

        Ok(vm_authz_policy_model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;
    use std::str::FromStr;
    use std::sync::Arc;

    use protobuf::text_format;
    use sdv_authz::{
        LocallyStoredAuthzPoliciesProvider, SingleVmAclPolicyProvider, VmLevelPermissionsProvider,
    };
    use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
    use sdv_authz_policy_proto_rust::vm_authz_policy::VmAuthzPolicy as VmAuthzPolicyProto;
    use sdv_comms_id::names::{
        PackageName, ServiceName as ServiceBundleName, ServiceUnitName, VmName,
    };

    struct TestAclPolicyProvider {
        mapping: HashMap<(PackageName, Option<ServiceBundleName>), AclPolicyProto>,
        vm_mapping: HashMap<VmName, VmAuthzPolicyProto>,
        // Here we use Mutex because `TestAclPolicyProvider` will be wrapper as Arc<>.
        calls_counter: Mutex<u32>,
        vm_calls_counter: Mutex<u32>,
    }

    impl TestAclPolicyProvider {
        fn new() -> Self {
            Self {
                mapping: HashMap::new(),
                vm_mapping: HashMap::new(),
                calls_counter: Mutex::new(0),
                vm_calls_counter: Mutex::new(0),
            }
        }

        fn set_acl_policy(
            &mut self,
            package_name: PackageName,
            service_name: Option<ServiceBundleName>,
            acl_policy: AclPolicyProto,
        ) {
            self.mapping.insert((package_name.clone(), service_name.clone()), acl_policy.clone());
            if service_name.is_some() {
                self.mapping.insert((package_name, None), acl_policy);
            }
        }

        fn set_vm_authz_policy(&mut self, vm_name: VmName, vm_authz_policy: VmAuthzPolicyProto) {
            self.vm_mapping.insert(vm_name, vm_authz_policy);
        }

        fn get_calls_counter(&self) -> u32 {
            *self.calls_counter.lock().unwrap()
        }

        fn get_vm_calls_counter(&self) -> u32 {
            *self.vm_calls_counter.lock().unwrap()
        }
    }

    impl LocallyStoredAuthzPoliciesProvider for TestAclPolicyProvider {}

    impl SingleVmAclPolicyProvider for TestAclPolicyProvider {
        fn get_acl_policy_proto(
            &self,
            package_name: &PackageName,
            service_bundle_name: Option<&ServiceBundleName>,
        ) -> anyhow::Result<AclPolicyProto> {
            *self.calls_counter.lock().unwrap() += 1;

            self.mapping
                .get(&(package_name.clone(), service_bundle_name.cloned()))
                .ok_or(anyhow::anyhow!(
                    "ACLs for {}{} are not specified",
                    package_name,
                    match service_bundle_name {
                        Some(name) => format!(".{}", name),
                        None => String::new(),
                    }
                ))
                .cloned()
        }

        fn get_namespace_with_acl_policy(
            &self,
        ) -> Result<Vec<(PackageName, Option<ServiceBundleName>)>, anyhow::Error> {
            Ok(self.mapping.keys().cloned().collect())
        }
    }

    impl VmLevelPermissionsProvider for TestAclPolicyProvider {
        fn get_vm_authz_policy_proto(
            &self,
            vm_name: &VmName,
        ) -> Result<VmAuthzPolicyProto, anyhow::Error> {
            *self.vm_calls_counter.lock().unwrap() += 1;

            self.vm_mapping
                .get(vm_name)
                .ok_or(anyhow::anyhow!("Authz policy for {vm_name} is not specified"))
                .cloned()
        }
    }

    #[test]
    fn test_get_acl_policy_proto_success() {
        let package_name = PackageName::from_str("com.google.sdv").unwrap();
        let service_bundle_name = ServiceBundleName::from_str("WindowManager").unwrap();
        let acl_policy_proto = text_format::parse_from_str::<AclPolicyProto>(
            r#"
            acls {
                name: "TirePressureSubs"
                include: "vm-cdc:*"
                include: "vm-telemetry:*"
                never_include: "vm-ivi:*"
            }
            service_units {
                name: "tire-pressure-publisher"
                acl_names: "TirePressureSubs"
            }
        "#,
        )
        .unwrap();

        let test_provider = {
            let mut test_provider = TestAclPolicyProvider::new();
            test_provider.set_acl_policy(
                package_name.clone(),
                Some(service_bundle_name.clone()),
                acl_policy_proto.clone(),
            );
            Arc::new(test_provider)
        };

        let caching_provider = CachingAclFileProvider::new(test_provider.clone());

        assert_eq!(test_provider.get_calls_counter(), 0);

        // First call should populate the cache.
        assert_eq!(
            caching_provider
                .get_acl_policy_proto(&package_name, Some(&service_bundle_name))
                .unwrap(),
            acl_policy_proto.clone()
        );
        assert_eq!(test_provider.get_calls_counter(), 1);

        // Second call should read from the cache.
        assert_eq!(
            caching_provider
                .get_acl_policy_proto(&package_name, Some(&service_bundle_name))
                .unwrap(),
            acl_policy_proto
        );
        assert_eq!(test_provider.get_calls_counter(), 1);
    }

    #[test]
    fn test_get_acl_policy_proto_not_found() {
        let test_provider = TestAclPolicyProvider::new();
        let caching_provider = CachingAclFileProvider::new(Arc::new(test_provider));

        let package_name = PackageName::from_str("com.google.sdv").unwrap();
        let service_bundle_name = ServiceBundleName::from_str("WindowManager").unwrap();

        assert_eq!(
            caching_provider
                .get_acl_policy_proto(&package_name, Some(&service_bundle_name))
                .unwrap_err()
                .to_string(),
            "com.google.sdv.WindowManager: Failed to get ACL policy proto"
        );
    }

    #[test]
    fn test_get_acl_policy_success() {
        let package_name = PackageName::from_str("com.google.sdv").unwrap();
        let service_bundle_name = ServiceBundleName::from_str("WindowManager").unwrap();
        let acl_policy_proto = text_format::parse_from_str::<AclPolicyProto>(
            r#"
            acls {
                name: "TirePressureSubs"
                include: "vm-cdc:*"
                include: "vm-telemetry:*"
                never_include: "vm-ivi:*"
            }
            service_units {
                name: "tire-pressure-publisher"
                acl_names: "TirePressureSubs"
            }
        "#,
        )
        .unwrap();

        let test_provider = {
            let mut test_provider = TestAclPolicyProvider::new();
            test_provider.set_acl_policy(
                package_name.clone(),
                Some(service_bundle_name.clone()),
                acl_policy_proto,
            );
            Arc::new(test_provider)
        };

        let caching_provider = CachingAclFileProvider::new(test_provider.clone());

        assert_eq!(test_provider.get_calls_counter(), 0);

        // First call should populate the cache.
        assert!(caching_provider
            .get_acl_policy(&package_name, Some(&service_bundle_name))
            .unwrap()
            .has_service_unit(&ServiceUnitName::from_str("tire-pressure-publisher").unwrap()));
        assert_eq!(test_provider.get_calls_counter(), 1);

        // Second call should read from the cache.
        assert!(caching_provider
            .get_acl_policy(&package_name, Some(&service_bundle_name))
            .unwrap()
            .has_service_unit(&ServiceUnitName::from_str("tire-pressure-publisher").unwrap()));
        assert_eq!(test_provider.get_calls_counter(), 1);
    }

    #[test]
    fn test_get_acl_policy_not_found() {
        let test_provider = TestAclPolicyProvider::new();
        let caching_provider = CachingAclFileProvider::new(Arc::new(test_provider));

        let package_name = PackageName::from_str("com.google.sdv").unwrap();
        let service_bundle_name = ServiceBundleName::from_str("WindowManager").unwrap();

        assert_eq!(
            caching_provider
                .get_acl_policy(&package_name, Some(&service_bundle_name))
                .unwrap_err()
                .to_string(),
            "com.google.sdv.WindowManager: Failed to get ACL policy proto"
        );
    }

    #[test]
    fn test_get_namespace_with_acl_policy() {
        let package_name = PackageName::from_str("com.google.sdv").unwrap();
        let service_bundle_name = ServiceBundleName::from_str("WindowManager").unwrap();

        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_acl_policy(
            package_name.clone(),
            Some(service_bundle_name.clone()),
            text_format::parse_from_str::<AclPolicyProto>(
                r#"
                    acls {
                        name: "TirePressureSubs"
                        include: "vm-cdc:*"
                        include: "vm-telemetry:*"
                        never_include: "vm-ivi:*"
                    }
                    service_units {
                        name: "tire-pressure-publisher"
                        acl_names: "TirePressureSubs"
                    }
                "#,
            )
            .unwrap(),
        );

        let caching_provider = CachingAclFileProvider::new(Arc::new(test_provider));

        let namespaces = caching_provider.get_namespace_with_acl_policy().unwrap();
        assert_eq!(namespaces.len(), 2);

        let known_namespaces_set =
            HashSet::<(PackageName, Option<ServiceBundleName>)>::from_iter(namespaces);
        assert_eq!(known_namespaces_set.len(), 2);
        assert!(known_namespaces_set
            .contains(&(package_name.clone(), Some(service_bundle_name.clone()))));
        assert!(known_namespaces_set.contains(&(package_name.clone(), None)));
    }

    #[test]
    fn test_get_vm_authz_policy_proto_success() {
        let vm_name = VmName::from_str("vm-ivi").unwrap();
        let vm_authz_policy_proto = text_format::parse_from_str::<VmAuthzPolicyProto>(
            r#"
            allow_server {
                service: "com.pkg.Svc"
                channel: "*"
            }
        "#,
        )
        .unwrap();

        let test_provider = {
            let mut test_provider = TestAclPolicyProvider::new();
            test_provider.set_vm_authz_policy(vm_name.clone(), vm_authz_policy_proto.clone());
            Arc::new(test_provider)
        };

        let caching_provider = CachingAclFileProvider::new(test_provider.clone());

        assert_eq!(test_provider.get_vm_calls_counter(), 0);

        // First call should populate the cache.
        assert_eq!(
            caching_provider.get_vm_authz_policy_proto(&vm_name).unwrap(),
            vm_authz_policy_proto.clone()
        );
        assert_eq!(test_provider.get_vm_calls_counter(), 1);

        // Second call should read from the cache.
        assert_eq!(
            caching_provider.get_vm_authz_policy_proto(&vm_name).unwrap(),
            vm_authz_policy_proto
        );
        assert_eq!(test_provider.get_vm_calls_counter(), 1);
    }

    #[test]
    fn test_get_vm_authz_policy_success() {
        let vm_name = VmName::from_str("vm-ivi").unwrap();
        let vm_authz_policy_proto = text_format::parse_from_str::<VmAuthzPolicyProto>(
            r#"
            allow_server {
                service: "com.pkg.Svc"
                channel: "*"
            }
        "#,
        )
        .unwrap();

        let test_provider = {
            let mut test_provider = TestAclPolicyProvider::new();
            test_provider.set_vm_authz_policy(vm_name.clone(), vm_authz_policy_proto);
            Arc::new(test_provider)
        };

        let caching_provider = CachingAclFileProvider::new(test_provider.clone());

        assert_eq!(test_provider.get_vm_calls_counter(), 0);

        // First call should populate the cache.
        let policy = caching_provider.get_vm_authz_policy(&vm_name).unwrap();
        let debug_str = format!("{:?}", policy);
        assert!(debug_str.contains("com.pkg") && debug_str.contains("Svc"));
        assert_eq!(test_provider.get_vm_calls_counter(), 1);

        // Second call should read from the cache.
        let policy = caching_provider.get_vm_authz_policy(&vm_name).unwrap();
        let debug_str = format!("{:?}", policy);
        assert!(debug_str.contains("com.pkg") && debug_str.contains("Svc"));
        assert_eq!(test_provider.get_vm_calls_counter(), 1);
    }

    #[test]
    fn test_get_vm_authz_policy_proto_error() {
        let test_provider = TestAclPolicyProvider::new();
        let caching_provider = CachingAclFileProvider::new(Arc::new(test_provider));

        let vm_name = VmName::from_str("vm-ivi").unwrap();

        assert!(caching_provider
            .get_vm_authz_policy_proto(&vm_name)
            .unwrap_err()
            .to_string()
            .contains("Failed to get VM Authz policy proto"));
    }

    #[test]
    fn test_get_vm_authz_policy_error() {
        let test_provider = TestAclPolicyProvider::new();
        let caching_provider = CachingAclFileProvider::new(Arc::new(test_provider));

        let vm_name = VmName::from_str("vm-ivi").unwrap();

        assert!(caching_provider
            .get_vm_authz_policy(&vm_name)
            .unwrap_err()
            .to_string()
            .contains("Failed to get VM Authz policy proto"));
    }

    #[test]
    fn test_get_vm_authz_policy_invalid_proto() {
        let vm_name = VmName::from_str("vm-ivi").unwrap();
        // Missing both 'package' and 'service' in server permission is invalid.
        let vm_authz_policy_proto = text_format::parse_from_str::<VmAuthzPolicyProto>(
            r#"
            allow_server {
                channel: "*"
            }
        "#,
        )
        .unwrap();

        let mut test_provider = TestAclPolicyProvider::new();
        test_provider.set_vm_authz_policy(vm_name.clone(), vm_authz_policy_proto);
        let caching_provider = CachingAclFileProvider::new(Arc::new(test_provider));

        assert!(caching_provider
            .get_vm_authz_policy(&vm_name)
            .unwrap_err()
            .to_string()
            .contains("Failed to parse VM Authz policy from proto"));
    }
}
