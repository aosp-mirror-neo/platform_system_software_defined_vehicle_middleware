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

use binder::{binder_impl::Binder, Interface, Result as BinderResult};
use google_sdv_identity::aidl::google::sdv::identity::{
    ServiceFqin::ServiceFqin, ServiceIdentity::EcPublicKey::EcPublicKey,
    ServiceIdentity::ServiceIdentity,
};
use sdv_comms_id::get_binder_caller_process_identity;

use named_lock::NamedLock;
use std::time::Duration;
use test_agents::get_identity_agent;

fn retry<T, E, F>(mut operation: F, num_retries: u32, delay: Duration) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
{
    for _ in 0..num_retries {
        match operation() {
            Ok(result) => return Ok(result),
            Err(_) => std::thread::sleep(delay),
        }
    }
    operation() // Last attempt without retrying
}

fn create_test_fqin(instance_name: String) -> ServiceFqin {
    ServiceFqin {
        sdvVmName: "".to_string(),
        sdvPackageName: "com.android.sdv.service_discovery.test".to_string(),
        serviceBundleName: "CreateIdentityTestV2".to_string(),
        serviceInstanceName: instance_name,
    }
}

fn create_and_register_identity(instance_name: String) -> BinderResult<ServiceIdentity> {
    let identity_agent = get_identity_agent();

    let publickey = EcPublicKey { keyValue: *b"01234567890123456789012345678912" };
    let fqin = create_test_fqin(instance_name);

    let link_to_death = Binder::new(()).as_binder();

    identity_agent.createIdentity(&publickey, &fqin, &link_to_death)
}

#[test]
fn create_identity_succeeds() {
    // Don't run integration tests in parallel as we rely on link_to_death logic
    let lock = NamedLock::create("test_lock").unwrap();
    let _guard = lock.lock().unwrap();
    // We have a fundamental problem with link_to_death. Our logic can't wait until identity is destroyed, so it can happen too late.
    // We need to retry to avoid flakiness.
    retry(|| create_and_register_identity("default".to_string()), 3, Duration::from_millis(100))
        .expect("Failed to create identity");
}
#[test]
/// Second test with the same identity registration to verify that identity is cleared by link_to_death logic
fn create_same_identity_succeeds() {
    // Don't run integration tests in parallel as we rely on link_to_death logic
    let lock = NamedLock::create("test_lock").unwrap();
    let _guard = lock.lock().unwrap();
    // We have a fundamental problem with link_to_death. Our logic can't wait until identity is destroyed, so it can happen too late.
    // We need to retry to avoid flakiness.
    retry(|| create_and_register_identity("default".to_string()), 3, Duration::from_millis(100))
        .expect("Failed to create identity");
}

#[test]
/// Third test with unique (in test target) identity registration.
/// We know that each test is running in the separate process in Rust, so it should be allowed anyway.
fn create_different_identity_succeeds() {
    // Don't run integration tests in parallel as we rely on link_to_death logic
    let lock = NamedLock::create("test_lock").unwrap();
    let _guard = lock.lock().unwrap();
    create_and_register_identity("different".to_string()).expect("Failed to create identity");
}

#[test]
fn create_multiple_identity_fails() {
    // Don't run integration tests in parallel as we rely on link_to_death logic
    let lock = NamedLock::create("test_lock").unwrap();
    let _guard = lock.lock().unwrap();
    create_and_register_identity("instance-a".to_string()) // first registration
        .expect("Failed to create first identity");
    create_and_register_identity("instance-b".to_string())
        .expect_err("Second registration from the same process should fail");
}

#[test]
fn create_with_invalid_identity_fails() {
    // Don't run integration tests in parallel as we rely on link_to_death logic
    let lock = NamedLock::create("test_lock").unwrap();
    let _guard = lock.lock().unwrap();
    let create_error = create_and_register_identity("invalid_instance".to_string())
        .expect_err("Registration of identity with underscore in instance name should fail when authz enforcement enabled.");
    assert_eq!(create_error.to_string(), "Status(-3, EX_ILLEGAL_ARGUMENT): 'invalid Instance Name: should consist of [a-z, 0-9, -] characters, found '_''");
}

#[test]
fn get_binder_caller_process_identity_succeeds() {
    let lock = NamedLock::create("test_lock").unwrap();
    let _guard = lock.lock().unwrap();
    let identity =
        create_and_register_identity("identity123".to_string()).expect("Failed to create identity");

    assert_eq!(
        identity,
        get_binder_caller_process_identity()
            .expect("Failed to get binder caller process identity")
            .into()
    );
}

#[test]
fn get_binder_caller_process_identity_not_found() {
    let lock = NamedLock::create("test_lock").unwrap();
    let _guard = lock.lock().unwrap();
    // Don't create identity, so it should not be found
    assert!(get_binder_caller_process_identity().is_err());
}
