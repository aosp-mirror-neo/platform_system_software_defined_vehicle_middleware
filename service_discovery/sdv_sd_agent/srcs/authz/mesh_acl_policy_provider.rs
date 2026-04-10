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

//! Mesh ACL policy provider.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use sdv_authz::{
    AclPolicyModel, AclPolicyProvider, LocallyStoredAuthzPoliciesProvider,
    SingleVmAclPolicyProvider, VmAuthzPolicyModel,
};
use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName, VmName};

use anyhow::{anyhow, bail};
use tracing::instrument;

pub(crate) struct MeshAclPolicyProvider {
    local_vm_name: VmName,
    local_vm_acl_policy_provider: Arc<dyn LocallyStoredAuthzPoliciesProvider>,
    // This hashmap should not have a provider for a local VM.
    remote_vm_acl_policy_providers: Mutex<HashMap<VmName, Box<dyn SingleVmAclPolicyProvider>>>,
}

impl MeshAclPolicyProvider {
    /// Creates new MeshAclPolicyProvider.
    pub fn new(
        local_vm_name: VmName,
        local_vm_acl_policy_provider: Arc<dyn LocallyStoredAuthzPoliciesProvider>,
    ) -> Self {
        let remote_vm_acl_policy_providers = Mutex::new(HashMap::new());

        Self { local_vm_name, local_vm_acl_policy_provider, remote_vm_acl_policy_providers }
    }

    /// Sets a remote ACL policy provider for a `vm_name`.
    #[instrument(
        name = "authz_perf::mesh_provider::set_remote_acl_policy_provider",
        skip(self, provider)
    )]
    pub fn set_remote_acl_policy_provider(
        &self,
        vm_name: VmName,
        provider: Box<dyn SingleVmAclPolicyProvider>,
    ) {
        let mut remote_providers = self.remote_vm_acl_policy_providers.lock().unwrap();

        remote_providers.insert(vm_name, provider);
    }

    /// Removes a remote ACL policy provider for a `vm_name`.
    #[instrument(name = "authz_perf::mesh_provider::remove_remote_acl_policy_provider", skip(self))]
    pub fn remove_remote_acl_policy_provider(&self, vm_name: &VmName) {
        let mut remote_providers = self.remote_vm_acl_policy_providers.lock().unwrap();

        remote_providers.remove(vm_name);
    }

    pub fn get_local_vm_acl_policy_provider(&self) -> Arc<dyn SingleVmAclPolicyProvider> {
        self.local_vm_acl_policy_provider.clone()
    }
}

impl AclPolicyProvider for MeshAclPolicyProvider {
    #[instrument(name = "authz_perf::mesh_provider::get_acl_policy", skip_all)]
    fn get_acl_policy(
        &self,
        vm_name: &VmName,
        package_name: &PackageName,
        service_bundle_name: Option<&ServiceBundleName>,
    ) -> Result<Arc<AclPolicyModel>, anyhow::Error> {
        if *vm_name == self.local_vm_name {
            return self
                .local_vm_acl_policy_provider
                .get_acl_policy(package_name, service_bundle_name);
        }

        let remote_providers = self.remote_vm_acl_policy_providers.lock().unwrap();

        let remote_provider = remote_providers
            .get(vm_name)
            .ok_or_else(|| anyhow!("No remote ACL policy provider for {:?}", vm_name))?;

        remote_provider.get_acl_policy(package_name, service_bundle_name)
    }

    fn get_vm_authz_policy(
        &self,
        vm_name: &VmName,
    ) -> Result<Arc<VmAuthzPolicyModel>, anyhow::Error> {
        if *vm_name != self.local_vm_name {
            return self.local_vm_acl_policy_provider.get_vm_authz_policy(vm_name);
        }

        bail!("Trying to get VM Authz policy for local VM: {:?}", vm_name)
    }

    fn get_available_vm_names(&self) -> Vec<VmName> {
        let mut vm_names = vec![self.local_vm_name.clone()];

        for (vm_name, _) in self.remote_vm_acl_policy_providers.lock().unwrap().iter() {
            vm_names.push(vm_name.clone());
        }

        vm_names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;
    use protobuf::text_format;
    use sdv_authz::{
        LocallyStoredAuthzPoliciesProvider, SingleVmAclPolicyProvider, VmLevelPermissionsProvider,
    };
    use sdv_authz_policy_proto_rust::authz_policy::AuthzPolicy as AclPolicyProto;
    use sdv_authz_policy_proto_rust::vm_authz_policy::VmAuthzPolicy as VmAuthzPolicyProto;
    use std::str::FromStr;
    use std::sync::Arc;

    const ACL_POLICY: &str = r#"
        acls {
            name: "TestAcl"
            include: "*"
        }
        service_units {
            name: "test-unit"
            acl_names: "TestAcl"
        }
    "#;

    const VM_AUTHZ_POLICY: &str = r#"
        allow_subscriber {
            message: "*"
            topic: "*"
        }
    "#;

    struct MockSingleVmAclPolicyProvider {
        acl_policy: Option<AclPolicyModel>,
        vm_authz_policy: Option<VmAuthzPolicyModel>,
    }

    impl MockSingleVmAclPolicyProvider {
        fn new() -> Self {
            Self { acl_policy: None, vm_authz_policy: None }
        }

        fn with_acl_policy(mut self, acl_policy_textproto: &str) -> Self {
            if !acl_policy_textproto.is_empty() {
                let proto: AclPolicyProto =
                    text_format::parse_from_str(acl_policy_textproto).unwrap();
                self.acl_policy = Some(AclPolicyModel::try_from(proto).unwrap());
            }
            self
        }

        fn with_vm_authz_policy(mut self, vm_authz_policy_textproto: &str) -> Self {
            if !vm_authz_policy_textproto.is_empty() {
                let proto: VmAuthzPolicyProto =
                    text_format::parse_from_str(vm_authz_policy_textproto).unwrap();
                self.vm_authz_policy = Some(VmAuthzPolicyModel::try_from(proto).unwrap());
            }
            self
        }
    }

    impl SingleVmAclPolicyProvider for MockSingleVmAclPolicyProvider {
        fn get_acl_policy_proto(
            &self,
            _package_name: &PackageName,
            _service_bundle_name: Option<&ServiceBundleName>,
        ) -> Result<AclPolicyProto, anyhow::Error> {
            bail!("Mesh ACL policy provider should call get_acl_policy instead to enable caching")
        }

        fn get_acl_policy(
            &self,
            _package_name: &PackageName,
            _service_bundle_name: Option<&ServiceBundleName>,
        ) -> Result<Arc<AclPolicyModel>, anyhow::Error> {
            // Return defaults if none provided, to support "empty" tests if needed, or error
            match &self.acl_policy {
                Some(policy) => Ok(Arc::new(policy.clone())),
                None => unimplemented!("Unexpected call to get_acl_policy"),
            }
        }

        fn get_namespace_with_acl_policy(
            &self,
        ) -> Result<Vec<(PackageName, Option<ServiceBundleName>)>, anyhow::Error> {
            Ok(vec![])
        }
    }

    impl LocallyStoredAuthzPoliciesProvider for MockSingleVmAclPolicyProvider {}

    impl VmLevelPermissionsProvider for MockSingleVmAclPolicyProvider {
        fn get_vm_authz_policy_proto(
            &self,
            _vm_name: &VmName,
        ) -> Result<VmAuthzPolicyProto, anyhow::Error> {
            bail!("Mesh ACL policy provider should call get_vm_authz_policy instead to enable caching")
        }

        fn get_vm_authz_policy(
            &self,
            _vm_name: &VmName,
        ) -> Result<Arc<VmAuthzPolicyModel>, anyhow::Error> {
            match &self.vm_authz_policy {
                Some(policy) => Ok(Arc::new(policy.clone())),
                None => unimplemented!("Unexpected call to get_vm_authz_policy"),
            }
        }
    }

    #[test]
    fn test_get_acl_policy_local() {
        let local_vm_name = VmName::from_str("local-vm").unwrap();
        let local_provider = Arc::new(
            MockSingleVmAclPolicyProvider::new()
                .with_acl_policy(ACL_POLICY)
                .with_vm_authz_policy(VM_AUTHZ_POLICY),
        );
        let provider = MeshAclPolicyProvider::new(local_vm_name.clone(), local_provider);

        let package_name = PackageName::from_str("com.test").unwrap();
        // Should delegate to local provider
        let result = provider.get_acl_policy(&local_vm_name, &package_name, None);
        let expected_policy = AclPolicyModel::try_from(
            text_format::parse_from_str::<AclPolicyProto>(ACL_POLICY).unwrap(),
        )
        .unwrap();
        assert_eq!(result.unwrap(), Arc::new(expected_policy));
    }

    #[test]
    fn test_get_acl_policy_remote() {
        let local_vm_name = VmName::from_str("local-vm").unwrap();
        let remote_vm_name = VmName::from_str("remote-vm").unwrap();
        let local_provider = Arc::new(
            MockSingleVmAclPolicyProvider::new()
                .with_acl_policy(ACL_POLICY)
                .with_vm_authz_policy(VM_AUTHZ_POLICY),
        );
        let provider = MeshAclPolicyProvider::new(local_vm_name, local_provider);

        let remote_provider = Box::new(
            MockSingleVmAclPolicyProvider::new()
                .with_acl_policy(ACL_POLICY)
                .with_vm_authz_policy(VM_AUTHZ_POLICY),
        );
        provider.set_remote_acl_policy_provider(remote_vm_name.clone(), remote_provider);

        let package_name = PackageName::from_str("com.test").unwrap();
        let result = provider.get_acl_policy(&remote_vm_name, &package_name, None);
        let expected_policy = AclPolicyModel::try_from(
            text_format::parse_from_str::<AclPolicyProto>(ACL_POLICY).unwrap(),
        )
        .unwrap();
        assert_eq!(result.unwrap(), Arc::new(expected_policy));
    }

    #[test]
    fn test_get_acl_policy_remote_not_found() {
        let local_vm_name = VmName::from_str("local-vm").unwrap();
        let remote_vm_name = VmName::from_str("remote-vm").unwrap();
        let local_provider = Arc::new(MockSingleVmAclPolicyProvider::new());
        let provider = MeshAclPolicyProvider::new(local_vm_name, local_provider);

        let package_name = PackageName::from_str("com.test").unwrap();
        let result = provider.get_acl_policy(&remote_vm_name, &package_name, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("No remote ACL policy provider for {:?}", remote_vm_name)
        );
    }

    #[test]
    fn test_get_vm_authz_policy_delegation_remote_vm() {
        let local_vm_name = VmName::from_str("local-vm").unwrap();
        let local_provider = Arc::new(
            MockSingleVmAclPolicyProvider::new()
                .with_acl_policy(ACL_POLICY)
                .with_vm_authz_policy(VM_AUTHZ_POLICY),
        );

        let remote_vm_name = VmName::from_str("remote-vm").unwrap();
        let remote_provider = Box::new(MockSingleVmAclPolicyProvider::new());

        let provider = MeshAclPolicyProvider::new(local_vm_name.clone(), local_provider);
        provider.set_remote_acl_policy_provider(remote_vm_name.clone(), remote_provider);

        // Remote VM should be available, as it is fetched from the local provider.
        let result = provider.get_vm_authz_policy(&remote_vm_name);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_vm_authz_policy_delegation_local_vm() {
        let local_vm_name = VmName::from_str("local-vm").unwrap();
        let local_provider = Arc::new(
            MockSingleVmAclPolicyProvider::new()
                .with_acl_policy(ACL_POLICY)
                .with_vm_authz_policy(VM_AUTHZ_POLICY),
        );

        let provider = MeshAclPolicyProvider::new(local_vm_name.clone(), local_provider);

        // Call to get VM Authz policy for local VM should fail, as it should never be called.
        let result = provider.get_vm_authz_policy(&local_vm_name);
        assert!(result.is_err());
    }

    #[test]
    fn test_available_vm_names() {
        let local_vm_name = VmName::from_str("local-vm").unwrap();
        let remote_vm_name = VmName::from_str("remote-vm").unwrap();
        let local_provider = Arc::new(MockSingleVmAclPolicyProvider::new());
        let provider = MeshAclPolicyProvider::new(local_vm_name.clone(), local_provider);

        let remote_provider = Box::new(MockSingleVmAclPolicyProvider::new());
        provider.set_remote_acl_policy_provider(remote_vm_name.clone(), remote_provider);

        let vms = provider.get_available_vm_names();
        assert!(vms.contains(&local_vm_name));
        assert!(vms.contains(&remote_vm_name));
        assert_eq!(vms.len(), 2);

        provider.remove_remote_acl_policy_provider(&remote_vm_name);
        let vms = provider.get_available_vm_names();
        assert_eq!(vms.len(), 1);
        assert!(vms.contains(&local_vm_name));
    }
}
