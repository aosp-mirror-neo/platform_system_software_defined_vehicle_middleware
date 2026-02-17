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

use std::str::FromStr;

use sdv_authz_local_vm_acl_policy_provider_factory::create_local_vm_acl_policy_provider;
use sdv_comms_id::names::{
    PackageName, ServiceName as ServiceBundleName, ServiceUnitName, UnitTypeName,
};

#[test]
fn test_authz_apex_has_acl_policy() {
    // Should create SDV ACL policy provider.
    let provider = create_local_vm_acl_policy_provider().unwrap();

    let package_name =
        PackageName::from_str("com.android.sdv.test.authz.local_vm_acl_policy_provider").unwrap();
    let service_bundle_name = ServiceBundleName::from_str("AuthzTest").unwrap();

    let acl_policy = provider.get_acl_policy(&package_name, Some(&service_bundle_name)).unwrap();

    assert!(acl_policy.has_unit_type(&UnitTypeName::from_str("TirePressureTopic1").unwrap()));
    assert!(acl_policy.has_unit_type(&UnitTypeName::from_str("TirePressureTopic2").unwrap()));

    assert!(acl_policy
        .has_service_unit(&ServiceUnitName::from_str("tire-pressure-publisher-1").unwrap()));
    assert!(acl_policy
        .has_service_unit(&ServiceUnitName::from_str("tire-pressure-publisher-2").unwrap()));
}

#[test]
fn test_authz_apex_without_acl_policy() {
    // Should create SDV ACL policy provider.
    let provider = create_local_vm_acl_policy_provider().unwrap();

    let package_name =
        PackageName::from_str("com.android.sdv.test.authz.local_vm_acl_policy_provider").unwrap();
    let service_bundle_name = ServiceBundleName::from_str("AuthzTestWithoutAclPolicy").unwrap();

    assert!(provider.get_acl_policy(&package_name, Some(&service_bundle_name)).is_err());
}

#[test]
fn test_authz_apex_service_bundles_with_acl_policy() {
    // Should create SDV ACL policy provider.
    let provider = create_local_vm_acl_policy_provider().unwrap();

    let service_bundles = provider.get_namespace_with_acl_policy().unwrap();

    let package_name =
        PackageName::from_str("com.android.sdv.test.authz.local_vm_acl_policy_provider").unwrap();

    let service_bundle_name_with_acl = ServiceBundleName::from_str("AuthzTest").unwrap();
    assert!(
        service_bundles.contains(&(package_name.clone(), Some(service_bundle_name_with_acl))),
        "Unexpected service bundles: {service_bundles:?}",
    );

    let service_bundle_name_without_acl =
        ServiceBundleName::from_str("AuthzTestWithoutAclPolicy").unwrap();
    assert!(
        !service_bundles.contains(&(package_name.clone(), Some(service_bundle_name_without_acl))),
        "Unexpected service bundles: {service_bundles:?}"
    );
}
