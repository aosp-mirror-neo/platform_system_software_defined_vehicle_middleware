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
use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinAidl;
use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::EcPublicKey::EcPublicKey;
use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::ServiceIdentity as ServiceIdentityAidl;

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::{
    metadata::{
        ApplicationMetadata::ApplicationMetadata,
        ApplicationMetadata::TypedMetadata::TypedMetadata::ServerUnit,
        ServerUnitMetadata::ServerUnitMetadata,
    },
    AccessControl::AccessControl,
    UnitType::UnitType as UnitTypeAidl,
};

#[allow(unused_imports)]
use binder::{binder_impl::Binder, ExceptionCode, Interface};

use test_agents::{get_identity_agent, get_registration_agent};

fn make_test_identity(instance_name: &str) -> ServiceIdentityAidl {
    let identity_registry_v2 = get_identity_agent();
    let publickey = EcPublicKey { keyValue: *b"01234567890123456789012345678912" };
    let fqin = ServiceFqinAidl {
        sdvVmName: "".to_string(),
        sdvPackageName: "com.android.sdv.service_discovery.test".to_string(),
        serviceBundleName: "ServiceUnitRegistrationTest".to_string(),
        serviceInstanceName: instance_name.to_string(),
    };

    // 1.) Create identity
    let link_to_death = Binder::new(()).as_binder();
    identity_registry_v2.createIdentity(&publickey, &fqin, &link_to_death).unwrap()
}

fn make_test_unit_type(fqin: &ServiceFqinAidl, unit_type_name: &str) -> UnitTypeAidl {
    UnitTypeAidl {
        sdvPackageName: fqin.sdvPackageName.clone(),
        serviceBundleName: fqin.serviceBundleName.clone(),
        typeName: unit_type_name.to_string(),
    }
}

#[test]
fn register_service_unit_success() {
    let identity = make_test_identity("register-service-unit");

    let registration_agent = get_registration_agent();
    let unit_type = make_test_unit_type(&identity.name, "UnitTypeName");
    let acl: AccessControl = AccessControl { ..Default::default() };
    let app_metadata = ApplicationMetadata {
        typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
        valueHolder: "test-value:1".into(),
    };
    let result = registration_agent.registerServiceUnit(
        "service-unit-name",
        &unit_type,
        &acl,
        &app_metadata,
    );
    result.expect("Service unit registration did not succeed!");
}

#[test]
fn register_service_unit_with_invalid_type() {
    let identity = make_test_identity("invalid-type-authz-enabled");

    let registration_agent = get_registration_agent();
    let unit_type = make_test_unit_type(&identity.name, "B@dUnitTypeName");
    let acl: AccessControl = AccessControl { ..Default::default() };
    let app_metadata = ApplicationMetadata {
        typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
        valueHolder: "test-value:1".into(),
    };
    let result = registration_agent.registerServiceUnit(
        "service-unit-name",
        &unit_type,
        &acl,
        &app_metadata,
    );
    let err = result.expect_err("Service unit registration succeeded when it should have failed!");
    assert_eq!(err.exception_code(), ExceptionCode::ILLEGAL_ARGUMENT);
}

#[test]
fn register_service_unit_with_invalid_name() {
    let identity = make_test_identity("invalid-name-authz-enabled");

    let registration_agent = get_registration_agent();
    let unit_type = make_test_unit_type(&identity.name, "UnitTypeName");
    let acl: AccessControl = AccessControl { ..Default::default() };
    let app_metadata = ApplicationMetadata {
        typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
        valueHolder: "test-value:1".into(),
    };
    let result = registration_agent.registerServiceUnit(
        "b@d-service-unit-name",
        &unit_type,
        &acl,
        &app_metadata,
    );
    let err = result.expect_err("Service unit registration succeeded when it should have failed!");
    assert_eq!(err.exception_code(), ExceptionCode::ILLEGAL_ARGUMENT);
}
