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

//! Permissions based Authz.

use crate::{AclPolicyProvider, AuthzResult};
use sdv_comms_id::{
    names::{PackageName, ServiceUnitName, UnitTypeName},
    ServiceFqin,
};
use std::sync::Arc;

/// Permissions based Authz.
pub struct AuthzViaPermissions {
    _policy_provider: Arc<dyn AclPolicyProvider>,
}

impl AuthzViaPermissions {
    /// Creates new instance of Permissions Authz.
    pub fn new(policy_provider: Arc<dyn AclPolicyProvider>) -> Self {
        Self { _policy_provider: policy_provider }
    }

    /// Returns Subject FQIN authorization result for the Object Service Unit.
    pub fn check_authorization_for_service_unit(
        &self,
        _subject_fqin: &ServiceFqin,
        _object_fqin: &ServiceFqin,
        _object_service_unit_name: &ServiceUnitName,
    ) -> AuthzResult {
        AuthzResult::ImplicitlyDenied("permissions check not implemented".to_string())
    }

    /// Returns Subject FQIN authorization result for the Object Unit Type.
    pub fn check_authorization_for_unit_type(
        &self,
        _subject_fqin: &ServiceFqin,
        _object_package_name: &PackageName,
        _object_unit_type_name: &UnitTypeName,
    ) -> AuthzResult {
        AuthzResult::ImplicitlyDenied("permissions check not implemented".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AclPolicyModel;
    use anyhow::anyhow;
    use sdv_comms_id::names::{ServiceName as ServiceBundleName, VmName};
    use std::str::FromStr;

    struct TestAclPolicyProvider;

    impl AclPolicyProvider for TestAclPolicyProvider {
        fn get_acl_policy(
            &self,
            _vm_name: &VmName,
            _package_name: &PackageName,
            _service_bundle_name: Option<&ServiceBundleName>,
        ) -> Result<Arc<AclPolicyModel>, anyhow::Error> {
            Err(anyhow!("not implemented"))
        }

        fn get_available_vm_names(&self) -> Vec<VmName> {
            vec![]
        }
    }

    #[test]
    fn test_check_authorization_for_service_unit() {
        let policy_provider = Arc::new(TestAclPolicyProvider);
        let service = AuthzViaPermissions::new(policy_provider);
        let subject_fqin = ServiceFqin::from_str("vm:com.a.B/c").unwrap();
        let object_fqin = ServiceFqin::from_str("vm:com.d.E/f").unwrap();
        let service_unit_name = ServiceUnitName::from_str("unit").unwrap();

        let result = service.check_authorization_for_service_unit(
            &subject_fqin,
            &object_fqin,
            &service_unit_name,
        );

        assert!(matches!(result, AuthzResult::ImplicitlyDenied(_)));
    }

    #[test]
    fn test_check_authorization_for_unit_type() {
        let policy_provider = Arc::new(TestAclPolicyProvider);
        let service = AuthzViaPermissions::new(policy_provider);
        let subject_fqin = ServiceFqin::from_str("vm:com.a.B/c").unwrap();
        let package_name = PackageName::from_str("com.d.e").unwrap();
        let unit_type_name = UnitTypeName::from_str("Unit_type").unwrap();

        let result = service.check_authorization_for_unit_type(
            &subject_fqin,
            &package_name,
            &unit_type_name,
        );

        assert!(matches!(result, AuthzResult::ImplicitlyDenied(_)));
    }
}
