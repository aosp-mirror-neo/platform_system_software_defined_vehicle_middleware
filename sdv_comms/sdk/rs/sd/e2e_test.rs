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

use bytemuck::TransparentWrapper;
use futures::StreamExt;
use googletest::prelude::*;
use lazy_static::lazy_static;
use sdv_comms_bindgen::ASDV_UnitIdentifier;
use sdv_comms_common::ffi_str;
use sdv_status_rs::SdvResult;
use std::collections::HashMap;
use std::default::Default;
use std::str::FromStr;
use std::time::Duration;
use tokio::join;
use tokio::time::timeout;

use crate::dt::Publisher;
use crate::id::{ServiceFqin, ServiceIdentity, UnitIdentifier};
use crate::sd::{
    PublisherUnitMetadata, ServiceDiscoveryManager, ServiceUnitChangeEvent, ServiceUnitDefinition,
    UnitMetadata, UnitNameDiscoveryArgs, UnitRegistrationArgs, UnitType,
};
use crate::ContextRef;

static UNIT_TYPE_STR: &str = "com.android.sdv.samples.SampleBundle1#HelloWorldTopic";
static SERVICE_FQIN_STR: &str = "vm-name:com.google.comms_stack_tests.SDE2Etests";

lazy_static! {
    static ref UNIT_TYPE: UnitType = UnitType::from_str(UNIT_TYPE_STR).expect("Invalid UnitType");
}

fn create_unit_identifier_with_unit_name(ctx: ContextRef, unit_name: &str) -> UnitIdentifier {
    let mut asdv_unit_identifier = ASDV_UnitIdentifier {
        service_identity: ServiceIdentity::peel(ctx.get_self_identity()),
        ..Default::default()
    };
    ffi_str::str_to_bytes(unit_name, &mut asdv_unit_identifier.unit_name)
        .expect("st_to_bytes failed");
    UnitIdentifier::wrap(asdv_unit_identifier)
}

fn create_unit_identifier(ctx: ContextRef) -> UnitIdentifier {
    create_unit_identifier_with_unit_name(ctx, "test-publisher")
}

fn create_unit_metadata() -> UnitMetadata {
    let mut publisher_metadata = PublisherUnitMetadata::new();
    publisher_metadata.set_message_size(10);
    publisher_metadata.set_message_count(100);

    UnitMetadata::Publisher(publisher_metadata)
}

async fn create_publisher(
    ctx: ContextRef,
    unit_type: &UnitType,
    unit_name: &str,
    metadata: &UnitMetadata,
) -> SdvResult<Publisher> {
    let mut unit_registration_args = UnitRegistrationArgs::new();

    unit_registration_args.set_unit_type(unit_type);
    unit_registration_args.set_unit_name(unit_name);
    unit_registration_args.set_unit_metadata(metadata);

    Publisher::create(ctx, unit_registration_args).await
}

async fn create_publisher_with_metadata(
    ctx: ContextRef,
    unit_type: &UnitType,
    unit_name: &str,
    unit_metadata: &UnitMetadata,
    application_metadata: &[u8],
) -> SdvResult<Publisher> {
    let mut unit_registration_args = UnitRegistrationArgs::new();

    unit_registration_args.set_unit_type(unit_type);
    unit_registration_args.set_unit_name(unit_name);
    unit_registration_args.set_unit_metadata(unit_metadata);
    unit_registration_args.set_application_metadata(application_metadata);

    Publisher::create(ctx, unit_registration_args).await
}

#[tokio::test]
async fn discover_publisher_by_type_success() {
    tokio::spawn(async {
        let fqin_str = SERVICE_FQIN_STR.to_string() + "DiscoverPublisherByType/instance";
        let fqin = ServiceFqin::from_str(&fqin_str).expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_identifier = create_unit_identifier(ctx);
        let metadata = create_unit_metadata();
        let unit_type = UnitType::from_str("discover.by.type.com.MyBundle#MyTypeName")
            .expect("received invalid unittype");
        let _publisher =
            create_publisher(ctx, &unit_type, unit_identifier.get_unit_name(), &metadata)
                .await
                .expect("Publisher creation failed");

        let suds = ServiceDiscoveryManager::new(ctx)
            .list_units_by_type(&unit_type)
            .collect::<Vec<_>>()
            .await;

        assert_that!(suds, len(eq(1)));
        assert_that!(suds[0].get_unit_identifier(), eq(&unit_identifier));
        assert_that!(suds[0].get_unit_metadata(), eq(&metadata));
    })
    .await
    .expect("Spawning a task should be successful");
}

// discover a large number of publishers. this is to verify we can transfer >1Mb over binder
#[tokio::test]
async fn discover_many_publishers_by_type_is_success() {
    const APPLICATION_METADATA_SIZE: usize = 20000;
    const NUMBER_OF_PUBLISHERS: usize = 50;
    tokio::spawn(async {
        let fqin_str = SERVICE_FQIN_STR.to_string() + "DiscoverManyPublishersByType/instance";
        let fqin = ServiceFqin::from_str(&fqin_str).expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        // create publishers with large metadata each
        let mut publishers = Vec::with_capacity(NUMBER_OF_PUBLISHERS);
        let mut unit_identifiers = HashMap::new();
        let application_metadata: [u8; APPLICATION_METADATA_SIZE] =
            [0xba; APPLICATION_METADATA_SIZE];
        let unit_type = UnitType::from_str("discover.by.type.concurrent.com.MyBundle#MyTypeName")
            .expect("received invalid unittype");
        for i in 1..=NUMBER_OF_PUBLISHERS {
            let unit_name = format!("{}{}", "test-publisher", i);
            let unit_identifier = create_unit_identifier_with_unit_name(ctx, &unit_name);
            let unit_metadata = create_unit_metadata();
            let publisher = create_publisher_with_metadata(
                ctx,
                &unit_type,
                unit_identifier.get_unit_name(),
                &unit_metadata,
                &application_metadata,
            )
            .await
            .expect("Publisher creation failed");
            publishers.push(publisher);
            unit_identifiers.insert(unit_name, unit_identifier);
        }

        let future1 =
            ServiceDiscoveryManager::new(ctx).list_units_by_type(&unit_type).collect::<Vec<_>>();

        let future2 =
            ServiceDiscoveryManager::new(ctx).list_units_by_type(&unit_type).collect::<Vec<_>>();

        let (suds1, suds2) = join!(future1, future2);

        assert_that!(suds1, len(eq(NUMBER_OF_PUBLISHERS)));
        for sud in suds1 {
            let received = sud.get_unit_identifier();
            let received_unit_name = received.get_unit_name();
            let actual = unit_identifiers.get(received_unit_name);
            assert!(actual.is_some(), "Expected a UnitIdentifier, but got None");
            let actual_ref = actual.unwrap();
            assert_that!(received, eq(actual_ref));
            assert_that!(sud.get_application_metadata(), eq(&application_metadata));
        }

        assert_that!(suds2, len(eq(NUMBER_OF_PUBLISHERS)));
        for sud in suds2 {
            let received = sud.get_unit_identifier();
            let received_unit_name = received.get_unit_name();
            let actual = unit_identifiers.get(received_unit_name);
            assert!(actual.is_some(), "Expected a UnitIdentifier, but got None");
            let actual_ref = actual.unwrap();
            assert_that!(received, eq(actual_ref));
            assert_that!(sud.get_application_metadata(), eq(&application_metadata));
        }
    })
    .await
    .expect("Spawning a task should be successful");
}

#[tokio::test]
async fn subscribe_unit_by_type_success() {
    binder::ProcessState::start_thread_pool();
    tokio::spawn(async {
        let fqin_str = SERVICE_FQIN_STR.to_string() + "SubscribeUnitByType/instance";
        let fqin = ServiceFqin::from_str(&fqin_str).expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_type: UnitType =
            UnitType::from_str("com.android.sdv.samples.SampleBundle1#SubscribeByType")
                .expect("Valid UnitType");

        // Subscribe before we register a specific unit.
        let mut event_stream = ServiceDiscoveryManager::new(ctx)
            .subscribe_service_unit_change_by_type(&unit_type)
            .await
            .expect("Failed to subscribe for unit registration");

        // Create and register publisher.
        let unit_identifier = create_unit_identifier(ctx);
        let metadata = create_unit_metadata();
        let _publisher =
            create_publisher(ctx, &unit_type, unit_identifier.get_unit_name(), &metadata)
                .await
                .expect("Publisher creation failed");

        let event = timeout(Duration::from_millis(5000), event_stream.next())
            .await
            .expect("Timed out waiting for registration event");
        assert_that!(event, some(matches_pattern!(ServiceUnitChangeEvent::Registered(anything()))));

        let Some(ServiceUnitChangeEvent::Registered(sud)) = event else { unreachable!() };
        assert_that!(sud.get_unit_identifier(), eq(&unit_identifier));
    })
    .await
    .expect("Spawning a task should be successful");
}

#[tokio::test]
async fn subscribe_unit_by_type_notifies_existing_units() {
    binder::ProcessState::start_thread_pool();
    tokio::spawn(async {
        let fqin_str = SERVICE_FQIN_STR.to_string() + "SubscribeUnitByTypeExisting/instance";
        let fqin = ServiceFqin::from_str(&fqin_str).expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_type: UnitType =
            UnitType::from_str("com.android.sdv.samples.SampleBundle1#ExistingByType")
                .expect("Valid UnitType");

        // Create and register publisher before we subscribe.
        let unit_identifier = create_unit_identifier(ctx);
        let metadata = create_unit_metadata();
        let _publisher =
            create_publisher(ctx, &unit_type, unit_identifier.get_unit_name(), &metadata)
                .await
                .expect("Publisher creation failed");

        // Subscribe and expect preexisting unit to be notified.
        let mut event_stream = ServiceDiscoveryManager::new(ctx)
            .subscribe_service_unit_change_by_type(&unit_type)
            .await
            .expect("Failed to subscribe for unit registration");

        let event = timeout(Duration::from_millis(5000), event_stream.next())
            .await
            .expect("Timed out waiting for registration event");
        assert_that!(event, some(matches_pattern!(ServiceUnitChangeEvent::Registered(anything()))));

        let Some(ServiceUnitChangeEvent::Registered(sud)) = event else { unreachable!() };
        assert_that!(sud.get_unit_identifier(), eq(&unit_identifier));
    })
    .await
    .expect("Spawning a task should be successful");
}

#[tokio::test]
async fn discover_publisher_by_name_success() {
    tokio::spawn(async {
        let fqin_str = SERVICE_FQIN_STR.to_string() + "DiscoverPublisherByName/instance";
        let fqin = ServiceFqin::from_str(&fqin_str).expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin.clone());

        let unit_identifier = create_unit_identifier(ctx);
        let metadata = create_unit_metadata();
        let unit_type = UnitType::from_str("discover.by.name.com.MyBundle#MyTypeName")
            .expect("received invalid unittype");
        let publisher =
            create_publisher(ctx, &unit_type, unit_identifier.get_unit_name(), &metadata)
                .await
                .expect("Publisher creation failed");

        let name_args = UnitNameDiscoveryArgs::new_builder()
            .set_sdv_package_name(fqin.get_sdv_package_name())
            .set_service_bundle_name(fqin.get_service_bundle_name())
            .build()
            .expect("UnitNameDiscoveryArgs build() failed");
        let suds = ServiceDiscoveryManager::new(ctx)
            .list_units_by_name(&name_args)
            .collect::<Vec<_>>()
            .await;

        assert_that!(suds, len(eq(1)));
        assert_that!(suds[0].get_unit_identifier(), eq(&unit_identifier));
        assert_that!(suds[0].get_unit_metadata(), eq(&metadata));

        drop(publisher);
    })
    .await
    .expect("Spawning a task should be successful");
}

#[tokio::test]
async fn discover_many_publishers_by_name_success() {
    const APPLICATION_METADATA_SIZE: usize = 20000;
    const NUMBER_OF_PUBLISHERS: usize = 50;
    tokio::spawn(async {
        let fqin_str = SERVICE_FQIN_STR.to_string() + "DiscoverManyPublishersByName/instance";
        let fqin = ServiceFqin::from_str(&fqin_str).expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin.clone());

        // create publishers with large metadata each
        let mut publishers = Vec::with_capacity(NUMBER_OF_PUBLISHERS);
        let mut unit_identifiers = HashMap::new();
        let application_metadata: [u8; APPLICATION_METADATA_SIZE] =
            [0xba; APPLICATION_METADATA_SIZE];
        let unit_type = UnitType::from_str("discover.by.name.concurrent.com.MyBundle#MyTypeName")
            .expect("received invalid unittype");
        for i in 1..=NUMBER_OF_PUBLISHERS {
            let unit_name = format!("{}{}", "test-publisher", i);
            let unit_identifier = create_unit_identifier_with_unit_name(ctx, &unit_name);
            let unit_metadata = create_unit_metadata();
            let publisher = create_publisher_with_metadata(
                ctx,
                &unit_type,
                unit_identifier.get_unit_name(),
                &unit_metadata,
                &application_metadata,
            )
            .await
            .expect("Publisher creation failed");
            publishers.push(publisher);
            unit_identifiers.insert(unit_name, unit_identifier);
        }

        let name_args = UnitNameDiscoveryArgs::new_builder()
            .set_sdv_package_name(fqin.get_sdv_package_name())
            .set_service_bundle_name(fqin.get_service_bundle_name())
            .build()
            .expect("UnitNameDiscoveryArgs build() failed");

        let future1 =
            ServiceDiscoveryManager::new(ctx).list_units_by_name(&name_args).collect::<Vec<_>>();

        let future2 =
            ServiceDiscoveryManager::new(ctx).list_units_by_name(&name_args).collect::<Vec<_>>();

        let (suds1, suds2) = join!(future1, future2);

        assert_that!(suds1, len(eq(NUMBER_OF_PUBLISHERS)));
        for sud in suds1 {
            let received = sud.get_unit_identifier();
            let received_unit_name = received.get_unit_name();
            let actual = unit_identifiers.get(received_unit_name);
            assert!(actual.is_some(), "Expected a UnitIdentifier, but got None");
            let actual_ref = actual.unwrap();
            assert_that!(received, eq(actual_ref));
            assert_that!(sud.get_application_metadata(), eq(&application_metadata));
        }
        assert_that!(suds2, len(eq(NUMBER_OF_PUBLISHERS)));
        for sud in suds2 {
            let received = sud.get_unit_identifier();
            let received_unit_name = received.get_unit_name();
            let actual = unit_identifiers.get(received_unit_name);
            assert!(actual.is_some(), "Expected a UnitIdentifier, but got None");
            let actual_ref = actual.unwrap();
            assert_that!(received, eq(actual_ref));
            assert_that!(sud.get_application_metadata(), eq(&application_metadata));
        }
    })
    .await
    .expect("Spawning a task should be successful");
}

#[tokio::test]
async fn subscribe_unit_by_name_success() {
    binder::ProcessState::start_thread_pool();
    tokio::spawn(async {
        let fqin_str = SERVICE_FQIN_STR.to_string() + "SubscribeUnitByName/instance";
        let fqin = ServiceFqin::from_str(&fqin_str).expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin.clone());
        let unit_identifier = create_unit_identifier_with_unit_name(ctx, "sub-unit-by-name");

        // Subscribe before we register a specific unit.
        let name_args = UnitNameDiscoveryArgs::new_builder()
            .set_sdv_package_name(ctx.get_self_fqin().get_sdv_package_name())
            .set_service_bundle_name(fqin.get_service_bundle_name())
            .set_service_unit_name(unit_identifier.get_unit_name())
            .build()
            .expect("UnitNameDiscoveryArgs build() failed.");
        let mut event_stream = ServiceDiscoveryManager::new(ctx)
            .subscribe_service_unit_change_by_name(&name_args)
            .await
            .expect("Failed to subscribe for unit registration");

        // Create and register publisher.
        let metadata = create_unit_metadata();
        let publisher =
            create_publisher(ctx, &UNIT_TYPE, unit_identifier.get_unit_name(), &metadata)
                .await
                .expect("Publisher creation failed");

        let event = timeout(Duration::from_millis(5000), event_stream.next())
            .await
            .expect("Timed out waiting for registration event");

        assert_that!(
            event,
            some(matches_pattern!(ServiceUnitChangeEvent::Registered(property!(
                ServiceUnitDefinition.get_unit_identifier(),
                eq(&unit_identifier)
            ))))
        );

        drop(publisher);

        let event = timeout(Duration::from_millis(5000), event_stream.next())
            .await
            .expect("Timed out waiting for unregistration event");

        assert_that!(
            event,
            some(matches_pattern!(ServiceUnitChangeEvent::Unregistered(property!(
                ServiceUnitDefinition.get_unit_identifier(),
                eq(&unit_identifier)
            ))))
        );
    })
    .await
    .expect("Spawning a task should be successful");
}

#[tokio::test]
async fn subscribe_unit_by_name_notifies_existing_units() {
    binder::ProcessState::start_thread_pool();
    tokio::spawn(async {
        let fqin_str = SERVICE_FQIN_STR.to_string() + "SubscribeUnitByNameExisting/instance";
        let fqin = ServiceFqin::from_str(&fqin_str).expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin.clone());
        let unit_identifier = create_unit_identifier_with_unit_name(ctx, "sub-unit-by-name-notify");

        // Create and register publisher before we subscribe.
        let metadata = create_unit_metadata();
        let _publisher =
            create_publisher(ctx, &UNIT_TYPE, unit_identifier.get_unit_name(), &metadata)
                .await
                .expect("Publisher creation failed");

        // Subscribe and expect preexisting unit to be notified.
        let name_args = UnitNameDiscoveryArgs::new_builder()
            .set_sdv_package_name(ctx.get_self_fqin().get_sdv_package_name())
            .set_service_bundle_name(fqin.get_service_bundle_name())
            .set_service_unit_name(unit_identifier.get_unit_name())
            .build()
            .expect("UnitNameDiscoveryArgs build() failed.");
        let mut event_stream = ServiceDiscoveryManager::new(ctx)
            .subscribe_service_unit_change_by_name(&name_args)
            .await
            .expect("Failed to subscribe for unit registration");

        let event = timeout(Duration::from_millis(5000), event_stream.next())
            .await
            .expect("Timed out waiting for registration event");
        assert_that!(event, some(matches_pattern!(ServiceUnitChangeEvent::Registered(anything()))));

        let Some(ServiceUnitChangeEvent::Registered(sud)) = event else { unreachable!() };
        assert_that!(sud.get_unit_identifier(), eq(&unit_identifier));
    })
    .await
    .expect("Spawning a task should be successful");
}

#[tokio::test]
async fn discover_publisher_get_unit_success() {
    // Spawn a new task to ensure all futures are Send.
    tokio::spawn(async {
        let fqin_str = SERVICE_FQIN_STR.to_string() + "DiscPublisherGetUnit/instance";
        let fqin = ServiceFqin::from_str(&fqin_str).expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin.clone());

        let unit_identifier = create_unit_identifier(ctx);
        let metadata = create_unit_metadata();
        let _publisher =
            create_publisher(ctx, &UNIT_TYPE, unit_identifier.get_unit_name(), &metadata)
                .await
                .expect("Publisher creation failed");

        let sud = ServiceDiscoveryManager::new(ctx)
            .get_unit(&ctx.get_self_fqin(), unit_identifier.get_unit_name().to_string())
            .await
            .unwrap();

        assert_that!(sud.get_unit_identifier(), eq(&unit_identifier));
        assert_that!(sud.get_unit_metadata(), eq(&metadata));
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn register_publisher_with_metadata_success() {
    let fqin_str = SERVICE_FQIN_STR.to_string() + "ApplicationMetadata/instance";
    let fqin = ServiceFqin::from_str(&fqin_str).expect("ServiceFqin should not be malformed");
    let ctx = ContextRef::create(fqin.clone());

    let application_metadata = [0x11_u8; 100];

    let unit_identifier = create_unit_identifier(ctx);
    let unit_metadata = create_unit_metadata();
    let _publisher = create_publisher_with_metadata(
        ctx,
        &UNIT_TYPE,
        unit_identifier.get_unit_name(),
        &unit_metadata,
        &application_metadata,
    )
    .await
    .expect("Publisher creation failed");

    let sud = ServiceDiscoveryManager::new(ctx)
        .get_unit(&ctx.get_self_fqin(), unit_identifier.get_unit_name().to_string())
        .await
        .unwrap();

    assert_that!(sud.get_unit_identifier(), eq(&unit_identifier));
    assert_that!(sud.get_application_metadata(), eq(&application_metadata));
}
