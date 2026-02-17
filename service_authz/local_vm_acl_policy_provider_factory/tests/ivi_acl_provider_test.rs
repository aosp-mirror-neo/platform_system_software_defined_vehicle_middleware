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

use std::path::PathBuf;
use std::str::FromStr;

use sdv_authz_local_vm_acl_policy_provider_factory::create_local_vm_acl_policy_provider;
use sdv_comms_id::names::{
    PackageName, ServiceName as ServiceBundleName, ServiceUnitName, UnitTypeName,
};

const PACKAGE_NAME: &str = "com.android.sdv.test.authz.local_vm_acl_policy_provider";

#[test]
fn test_vendor_partition_has_acl_policy() {
    // Check that the file is in the correct location.
    assert!(PathBuf::from(format!(
        "/vendor/etc/sdv_authz_acls/{PACKAGE_NAME}/VendorBundle.textproto"
    ))
    .exists());

    // Should create IVI ACL policy provider.
    let provider = create_local_vm_acl_policy_provider().unwrap();

    let package_name = PackageName::from_str(PACKAGE_NAME).unwrap();
    let service_bundle_name = ServiceBundleName::from_str("VendorBundle").unwrap();

    let acl_policy = provider.get_acl_policy(&package_name, Some(&service_bundle_name)).unwrap();

    assert!(acl_policy.has_unit_type(&UnitTypeName::from_str("VendorTopic").unwrap()));

    assert!(acl_policy.has_service_unit(&ServiceUnitName::from_str("vendor-service-unit").unwrap()));
}

#[test]
fn test_product_partition_has_acl_policy() {
    // Check that the file is in the correct location.
    assert!(PathBuf::from(format!(
        "/product/etc/sdv_authz_acls/{PACKAGE_NAME}/ProductBundle.textproto"
    ))
    .exists());

    // Should create IVI ACL policy provider.
    let provider = create_local_vm_acl_policy_provider().unwrap();

    let package_name = PackageName::from_str(PACKAGE_NAME).unwrap();
    let service_bundle_name = ServiceBundleName::from_str("ProductBundle").unwrap();

    let acl_policy = provider.get_acl_policy(&package_name, Some(&service_bundle_name)).unwrap();

    assert!(acl_policy.has_unit_type(&UnitTypeName::from_str("ProductTopic").unwrap()));

    assert!(
        acl_policy.has_service_unit(&ServiceUnitName::from_str("product-service-unit").unwrap())
    );
}

#[test]
fn test_system_partition_has_acl_policy() {
    // Check that the file is in the correct location.
    assert!(PathBuf::from(format!(
        "/system/etc/sdv_authz_acls/{PACKAGE_NAME}/SystemBundle.textproto"
    ))
    .exists());

    // Should create IVI ACL policy provider.
    let provider = create_local_vm_acl_policy_provider().unwrap();

    let package_name = PackageName::from_str(PACKAGE_NAME).unwrap();
    let service_bundle_name = ServiceBundleName::from_str("SystemBundle").unwrap();

    let acl_policy = provider.get_acl_policy(&package_name, Some(&service_bundle_name)).unwrap();

    assert!(acl_policy.has_unit_type(&UnitTypeName::from_str("SystemTopic").unwrap()));

    assert!(acl_policy.has_service_unit(&ServiceUnitName::from_str("system-service-unit").unwrap()));
}

#[test]
fn test_system_ext_partition_has_acl_policy() {
    // Check that the file is in the correct location.
    assert!(PathBuf::from(format!(
        "/system_ext/etc/sdv_authz_acls/{PACKAGE_NAME}/SystemExtBundle.textproto"
    ))
    .exists());

    // Should create IVI ACL policy provider.
    let provider = create_local_vm_acl_policy_provider().unwrap();

    let package_name = PackageName::from_str(PACKAGE_NAME).unwrap();
    let service_bundle_name = ServiceBundleName::from_str("SystemExtBundle").unwrap();

    let acl_policy = provider.get_acl_policy(&package_name, Some(&service_bundle_name)).unwrap();

    assert!(acl_policy.has_unit_type(&UnitTypeName::from_str("SystemExtTopic").unwrap()));

    assert!(
        acl_policy.has_service_unit(&ServiceUnitName::from_str("system-ext-service-unit").unwrap())
    );
}
