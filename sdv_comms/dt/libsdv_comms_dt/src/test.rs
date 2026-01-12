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

use super::*;

use crate::core::publisher_internal::PublisherInternal;
use crate::ffi::ffi_types;
use sdv_comms_common::callback::sdk::CallbackWrapper;
use sdv_comms_ctx::ContextImpl;
use sdv_comms_id::names::ServiceUnitName;
use sdv_comms_id::{ServiceFqin, UnitIdentifier};
use sdv_comms_sd::discovery::UnitType;
use sdv_comms_sd::metadata::PublisherUnitMetadata;
use sdv_comms_sd::UnitRegistrationArgs;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::option::Option;
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time;

const MESSAGE_SIZE: u32 = 32;
const MESSAGE_COUNT: u32 = 16;

fn make_test_unit_type(unit_type_name: &str) -> UnitType {
    UnitType::builder()
        .package_name("com.dt_test.pkg")
        .service_bundle_name("SamplePublisher")
        .unit_type_name(unit_type_name)
        .build()
        .unwrap()
}

async fn register_publisher(
    ctx: &'static ContextImpl,
    unit_name: &str,
    unit_type: UnitType,
    message_size: u32,
    message_count: u32,
) -> SdvResult<PublisherInternal> {
    let mut args = UnitRegistrationArgs::default();
    args.set_unit_name(ServiceUnitName::from_str(unit_name).unwrap());
    args.set_unit_type(unit_type);

    let mut publisher_metadata = PublisherUnitMetadata::default();
    publisher_metadata.set_version(1);
    publisher_metadata.set_message_size(message_size);
    publisher_metadata.set_message_count(message_count);
    let unit_metadata = args.get_mut_unit_metadata();
    unit_metadata.set_publisher_metadata(publisher_metadata);

    core::publisher_internal::PublisherInternal::create_publication(ctx, args).await
}

async fn register_test_publisher(
    ctx: &'static ContextImpl,
    unit_name: &str,
    unit_type: UnitType,
) -> SdvResult<PublisherInternal> {
    register_publisher(ctx, unit_name, unit_type, MESSAGE_SIZE, MESSAGE_COUNT).await
}

#[tokio::test]
async fn end_to_end_test() {
    // Prepare Buffers
    let pub_buff = [0x11_i8; MESSAGE_SIZE as usize];
    let mut sub_buff = [0x00_i8; MESSAGE_SIZE as usize];

    let fqin = ServiceFqin::from_str("vm-name:com.google.testpkg.InternalCommsDtTest/end-to-end")
        .expect("ServiceFqin should not be malformed");

    let ctx = sdv_comms_ctx::initialise(fqin).expect("Context initialisation failed");
    // SAFETY: Context object is alive for as long as it is required below so it is safe to
    // transmute into 'static lifetime.
    // TODO(b/361746197): Remove this transmute call once the Test framework is ready
    let ctx_ref = unsafe { std::mem::transmute::<&ContextImpl, &'static ContextImpl>(&ctx) };

    // Create and register sample publisher
    let unit_type = make_test_unit_type("DataTunnelEndToEndTestUnitType");
    let unit_name = "data-tunnel-end-to-end-test-publication";
    let mut publisher = register_test_publisher(ctx_ref, unit_name, unit_type)
        .await
        .expect("Failed to create publication");

    // Write 1 message using publisher
    assert!(publisher.write_messages(&pub_buff, 1).unwrap());

    // Subscribe to registered publisher without fetching the last message
    let unit_identifier = UnitIdentifier::new(ctx.get_identity().clone(), unit_name.to_string());

    let subscriber = core::subscriber_internal::SubscriberInternal::subscribe_to_publication(
        ctx_ref,
        &unit_identifier,
        ffi_types::ASDV_SubscriptionFlags_None,
    )
    .await
    .expect("Unable to subscribe");

    // Expect to have no messages available to read
    let mut count = subscriber.available_to_read().unwrap();
    assert_eq!(count as u32, 0);

    // Subscribe to registered publisher with fetching the last message
    let mut subscriber_flm =
        core::subscriber_internal::SubscriberInternal::subscribe_to_publication(
            ctx_ref,
            &unit_identifier,
            ffi_types::ASDV_SubscriptionFlags_FetchLastMessage,
        )
        .await
        .expect("Unable to subscribe");

    // Expect to have 1 message available to read
    count = subscriber_flm.available_to_read().unwrap();
    assert_eq!(count as u32, 1);

    // read 0-length and verify it "succeeds" but is a no-op.
    assert_eq!(subscriber_flm.read(&mut [], 0).unwrap(), 0);
    assert_ne!(pub_buff, sub_buff);

    // Expect to still have 1 message available to read
    count = subscriber_flm.available_to_read().unwrap();
    assert_eq!(count as u32, 1);

    // read the message and compare it with the written one
    assert_eq!(subscriber_flm.read(&mut sub_buff, count).unwrap(), count);
    assert_eq!(pub_buff, sub_buff);

    // Expect to have 0 messages available to read
    count = subscriber_flm.available_to_read().unwrap();
    assert_eq!(count as u32, 0);
}

#[tokio::test]
async fn regular_subscriber_test() {
    // Prepare Buffers
    let pub_buff = [0x11_i8; MESSAGE_SIZE as usize];
    let mut sub_buff = [0x00_i8; MESSAGE_SIZE as usize];

    let fqin = ServiceFqin::from_str("vm-name:com.google.testpkg.InternalCommsDtTest/end-to-end")
        .expect("ServiceFqin should not be malformed");

    let ctx = sdv_comms_ctx::initialise(fqin).expect("Context initialisation failed");
    // SAFETY: Context object is alive for as long as it is required below so it is safe to
    // transmute into 'static lifetime.
    // TODO(b/361746197): Remove this transmute call once the Test framework is ready
    let ctx_ref = unsafe { std::mem::transmute::<&ContextImpl, &'static ContextImpl>(&ctx) };

    // Create and register sample publisher
    let unit_type = make_test_unit_type("DataTunnelEndToEndTestUnitType");
    let unit_name = "data-tunnel-end-to-end-test-publication";
    let mut publisher = register_test_publisher(ctx_ref, unit_name, unit_type)
        .await
        .expect("Failed to create publication");

    // Write 1 message using publisher
    assert!(publisher.write_messages(&pub_buff, 1).unwrap());

    // Subscribe to registered publisher without fetching the last message
    let unit_identifier = UnitIdentifier::new(ctx.get_identity().clone(), unit_name.to_string());

    let subscriber = core::subscriber_internal::RegularSubscriber::subscribe_to_publication(
        ctx_ref,
        &unit_identifier,
        ffi_types::ASDV_SubscriptionFlags_None,
    )
    .await
    .expect("Unable to subscribe");

    // Expect to have no messages available to read
    let mut count = subscriber.available_to_read().unwrap();
    assert_eq!(count, 0_usize);

    // Subscribe to registered publisher with fetching the last message
    let mut subscriber_flm =
        core::subscriber_internal::RegularSubscriber::subscribe_to_publication(
            ctx_ref,
            &unit_identifier,
            ffi_types::ASDV_SubscriptionFlags_FetchLastMessage,
        )
        .await
        .expect("Unable to subscribe");

    // Expect to have 1 message available to read
    count = subscriber_flm.available_to_read().unwrap();
    assert_eq!(count, 1_usize);

    // read the message and compare it with the written one
    assert_eq!(subscriber_flm.read(&mut sub_buff, count).unwrap(), count);
    assert_eq!(pub_buff, sub_buff);

    // Expect to have 0 messages available to read
    count = subscriber_flm.available_to_read().unwrap();
    assert_eq!(count, 0_usize);

    // Set up a notification handler on one subscriber.
    let (tx, rx): (
        mpsc::Sender<ffi_types::ASDV_NotificationType>,
        mpsc::Receiver<ffi_types::ASDV_NotificationType>,
    ) = mpsc::channel();
    subscriber_flm
        .set_notification_closure(
            move |ev| tx.send(ev).map_err(|_| SdvStatus::new(SdvStatusCode::Internal)),
            0,
        )
        .expect("Failed to set notification closure on RegularSubscriber!");

    // Get initial message from DT agent since from its perspective FMQ is non-empty.
    assert_eq!(
        rx.recv_timeout(time::Duration::from_millis(200)),
        Ok(ffi_types::ASDV_NotificationType::DataAvailable)
    );

    // Write to the publisher again
    assert!(publisher.write_messages(&pub_buff, 1).unwrap());

    // Get message triggered by new write.
    assert_eq!(
        rx.recv_timeout(time::Duration::from_millis(200)),
        Ok(ffi_types::ASDV_NotificationType::DataAvailable)
    );

    // Check that 1 message is available and we can read it.
    assert_eq!(subscriber_flm.available_to_read().unwrap(), 1_usize);
    assert_eq!(subscriber_flm.read(&mut sub_buff, 1_usize).unwrap(), 1);

    // Remove notification handler
    subscriber_flm
        .clear_notification_handler()
        .expect("Failed to clear notification handler on RegularSubscriber!");

    // Write to the publisher again
    assert!(publisher.write_messages(&pub_buff, 1).unwrap());

    // No notification -- we should get error indicating disconnection since
    // tx will be dropped along with the closure.
    assert!(rx.recv().is_err());

    // Drop the publisher.
    drop(publisher);

    // Subscriber sees the last message written even though publisher is gone.
    assert_eq!(subscriber_flm.available_to_read().unwrap(), 1_usize);
    assert_eq!(subscriber_flm.read(&mut sub_buff, 1_usize).unwrap(), 1);
}

#[tokio::test]
async fn invalid_publication_metadata_test() {
    let fqin =
        ServiceFqin::from_str("vm-name:com.google.testpkg.InternalCommsDtTest/publication-drop")
            .expect("ServiceFqin should not be malformed");

    let ctx = sdv_comms_ctx::initialise(fqin).expect("Context initialisation failed");
    // SAFETY: Context object is alive for as long as it is required below so it is safe to
    // transmute into 'static lifetime.
    // TODO(b/361746197): Remove this transmute call once the Test framework is ready
    let ctx_ref = unsafe { std::mem::transmute::<&ContextImpl, &'static ContextImpl>(&ctx) };

    // Create and register sample publisher
    let unit_type = make_test_unit_type("DataTunnelPublicationDropUnitType");
    let unit_name = "data-tunnel-publication-drop-test-publication";

    // Try to create a publication with invalid metadata values
    assert!(
        register_publisher(ctx_ref, unit_name, unit_type.clone(), 0u32, 2u32).await.is_err(),
        "It must be impossible to register a publication with message_size=0"
    );
    assert!(
        register_publisher(ctx_ref, unit_name, unit_type.clone(), 2u32, 1u32).await.is_err(),
        "It must be impossible to register a publication with message_count<2"
    );
    assert!(
        register_publisher(ctx_ref, unit_name, unit_type.clone(), 2u32, 5u32).await.is_err(),
        "It must be impossible to register a publication with odd value of message_count"
    );

    // Try to create a publication with valid metadata values
    register_publisher(ctx_ref, unit_name, unit_type.clone(), 2u32, 2u32)
        .await
        .expect("Unable to register a publication with message_size=2, message_count=2");
}

#[tokio::test]
async fn publication_drop_test() {
    let fqin =
        ServiceFqin::from_str("vm-name:com.google.testpkg.InternalCommsDtTest/publication-drop")
            .expect("ServiceFqin should not be malformed");

    let ctx = sdv_comms_ctx::initialise(fqin).expect("Context initialisation failed");
    // SAFETY: Context object is alive for as long as it is required below so it is safe to
    // transmute into 'static lifetime.
    // TODO(b/361746197): Remove this transmute call once the Test framework is ready
    let ctx_ref = unsafe { std::mem::transmute::<&ContextImpl, &'static ContextImpl>(&ctx) };

    // Create and register sample publisher
    let unit_type = make_test_unit_type("DataTunnelPublicationDropUnitType");
    let unit_name = "data-tunnel-publication-drop-test-publication";
    let publisher = register_test_publisher(ctx_ref, unit_name, unit_type.clone())
        .await
        .expect("Failed to register a publication");

    // Try to create another publication with the same unit_type and unit_name
    let publisher2_res = register_test_publisher(ctx_ref, unit_name, unit_type.clone()).await;

    // It should fail
    assert!(
        publisher2_res.is_err(),
        "Re-registering a publication with the same name should return an error"
    );

    // Dropping the publisher should lead to the destruction of the publication in the DT agent
    drop(publisher);

    // Now we should be able to register another publication with the same unit_type and unit_name
    let _publisher3 = register_test_publisher(ctx_ref, unit_name, unit_type.clone())
        .await
        .expect("Failed to re-register the publication after drop");
}

#[tokio::test]
async fn overflow_test() {
    // Prepare Buffers
    let mut pub_buff = [0x11_i8; MESSAGE_SIZE as usize];
    let mut sub_buff = [0x00_i8; MESSAGE_SIZE as usize];

    let fqin = ServiceFqin::from_str("vm-name:com.google.testpkg.InternalCommsDtTest/end-to-end")
        .expect("ServiceFqin should not be malformed");

    let ctx = sdv_comms_ctx::initialise(fqin).expect("Context initialisation failed");
    // SAFETY: Context object is alive for as long as it is required below so it is safe to
    // transmute into 'static lifetime.
    // TODO(b/361746197): Remove this transmute call once the Test framework is ready
    let ctx_ref = unsafe { std::mem::transmute::<&ContextImpl, &'static ContextImpl>(&ctx) };

    // Create and register sample publisher
    let unit_type = make_test_unit_type("DataTunnelOverflowTestUnitType");
    let unit_name = "data-tunnel-overflow-test-publication";
    let mut publisher = register_test_publisher(ctx_ref, unit_name, unit_type)
        .await
        .expect("Failed to create publication");

    //
    // Test read pointer initialization by  overflowing the publication's buffer before subscribe
    //

    // Write MESSAGE_COUNT+1 messages to the publication
    for x in 0..=(MESSAGE_COUNT as usize) {
        pub_buff.fill((x % 128) as i8);
        assert!(publisher.write_messages(&pub_buff, 1).unwrap());
    }

    // Subscribe to registered publisher last message fetch
    let unit_identifier = UnitIdentifier::new(ctx.get_identity().clone(), unit_name.to_string());

    let mut subscriber = core::subscriber_internal::SubscriberInternal::subscribe_to_publication(
        ctx_ref,
        &unit_identifier,
        ffi_types::ASDV_SubscriptionFlags_FetchLastMessage,
    )
    .await
    .expect("Unable to subscribe");

    // Expect to have 1 message available to read
    assert_eq!(subscriber.available_to_read().unwrap(), 1);

    // read the message and compare it with the written one
    assert_eq!(subscriber.read(&mut sub_buff, 1).unwrap(), 1);
    assert_eq!(pub_buff, sub_buff);

    // Expect to have 0 messages available to read
    assert_eq!(subscriber.available_to_read().unwrap(), 0_usize);

    //
    // Test overflow by overflowing the publication's buffer after subscribing
    //

    // Write MESSAGE_COUNT+1 messages to the publication
    for x in 0..=(MESSAGE_COUNT as usize) {
        pub_buff.fill((x % 128) as i8);
        assert!(publisher.write_messages(&pub_buff, 1).unwrap());
    }

    // Expect to have MESSAGE_COUNT+1 messages available to read
    assert_eq!(subscriber.available_to_read().unwrap() as u32, MESSAGE_COUNT + 1);

    // Read must return error
    assert!(subscriber.read(&mut sub_buff, 1).is_err());

    // Expect to have MESSAGE_COUNT/2 messages available to read
    assert_eq!(subscriber.available_to_read().unwrap() as u32, MESSAGE_COUNT / 2);

    //
    // Test overflow possible on 0-length read
    //

    // Write MESSAGE_COUNT/2+1 messages to the publication to re-overflow
    for x in 0..=((MESSAGE_COUNT / 2) as usize) {
        pub_buff.fill((x % 128) as i8);
        assert!(publisher.write_messages(&pub_buff, 1).unwrap());
    }

    // Expect to have MESSAGE_COUNT+1 messages available to read
    assert_eq!(subscriber.available_to_read().unwrap() as u32, MESSAGE_COUNT + 1);

    // Read must return error
    assert!(subscriber.read(&mut [], 0).is_err());

    // Expect to have MESSAGE_COUNT/2 messages available to read
    assert_eq!(subscriber.available_to_read().unwrap() as u32, MESSAGE_COUNT / 2);
}

#[tokio::test]
async fn notifications_test() {
    // Prepare Buffers
    let pub_buff = [0x11_i8; MESSAGE_SIZE as usize];

    // Prepare sync channels
    let (tx, rx): (
        mpsc::Sender<ffi_types::ASDV_NotificationType>,
        mpsc::Receiver<ffi_types::ASDV_NotificationType>,
    ) = mpsc::channel();

    let fqin =
        ServiceFqin::from_str("vm-name:com.google.testpkg.InternalCommsDtTest/notifications")
            .expect("ServiceFqin should not be malformed");

    let ctx = sdv_comms_ctx::initialise(fqin).expect("Context initialisation failed");
    // SAFETY: Context object is alive for as long as it is required below so it is safe to
    // transmute into 'static lifetime.
    // TODO(b/361746197): Remove this transmute call once the Test framework is ready
    let ctx_ref = unsafe { std::mem::transmute::<&ContextImpl, &'static ContextImpl>(&ctx) };

    // Create and register sample publisher
    let unit_type = make_test_unit_type("DataTunnelNotificationsTestUnitType");
    let unit_name = "data-tunnel-notifications-publication";
    let mut publisher = register_test_publisher(ctx_ref, unit_name, unit_type)
        .await
        .expect("Failed to create publication");

    // Subscribe to registered publisher
    let unit_identifier = UnitIdentifier::new(ctx.get_identity().clone(), unit_name.to_string());

    let mut subscriber = core::subscriber_internal::SubscriberInternal::subscribe_to_publication(
        ctx_ref,
        &unit_identifier,
        0,
    )
    .await
    .expect("Unable to subscribe");

    // Set notification handler
    type CallbackFunc = CallbackWrapper<ffi_types::ASDV_NotificationType>;
    let cb = CallbackFunc::new({
        let tx = tx.clone();
        move |t| {
            tx.send(t).unwrap();
        }
    });

    let vtable = crate::ffi::ffi_types::ASDV_CallbackVTable_SetNotificationHandler::new(cb);
    subscriber.set_notification_handler(vtable.into(), 0).unwrap();

    // Check no message yet
    assert_eq!(
        rx.recv_timeout(time::Duration::from_millis(200)),
        Err(mpsc::RecvTimeoutError::Timeout),
        "The notification callback should not have been triggered with empty FMQ."
    );

    // Write one message
    assert!(publisher.write_messages(&pub_buff, 1).unwrap());

    // Ensure notification was triggered
    assert_eq!(
        rx.recv_timeout(time::Duration::from_millis(200)),
        Ok(ffi_types::ASDV_NotificationType::DataAvailable)
    );
    assert_eq!(
        rx.recv_timeout(time::Duration::from_millis(200)),
        Err(mpsc::RecvTimeoutError::Timeout),
        "The notification callback should have been triggered only once."
    );

    // Write one more message
    assert!(publisher.write_messages(&pub_buff, 1).unwrap());
    assert_eq!(
        rx.recv_timeout(time::Duration::from_millis(200)),
        Ok(ffi_types::ASDV_NotificationType::DataAvailable)
    );
    assert_eq!(
        rx.recv_timeout(time::Duration::from_millis(200)),
        Err(mpsc::RecvTimeoutError::Timeout),
        "The notification callback should have been triggered only once."
    );

    // Clear notification handler
    subscriber.clear_notification_handler().unwrap();

    // Write one more message
    assert!(publisher.write_messages(&pub_buff, 1).unwrap());
    assert_eq!(
        rx.recv_timeout(time::Duration::from_millis(200)),
        Err(mpsc::RecvTimeoutError::Timeout),
        "The notification callback should not have been triggered with a cleared callback."
    );

    // Set another notification handler
    let cb2 = CallbackFunc::new({
        let tx = tx.clone();
        move |t| {
            tx.send(t).unwrap();
        }
    });
    let vtable2 = crate::ffi::ffi_types::ASDV_CallbackVTable_SetNotificationHandler::new(cb2);
    subscriber.set_notification_handler(vtable2.into(), 0).unwrap();

    // The notification callback must be triggered right after we set it up since there are
    // messages to read
    assert_eq!(
        rx.recv_timeout(time::Duration::from_millis(200)),
        Ok(ffi_types::ASDV_NotificationType::DataAvailable)
    );

    // Write one more message
    assert!(publisher.write_messages(&pub_buff, 1).unwrap());

    // Ensure notification is triggered again
    assert_eq!(
        rx.recv_timeout(time::Duration::from_millis(200)),
        Ok(ffi_types::ASDV_NotificationType::DataAvailable)
    );

    assert_eq!(
        rx.recv_timeout(time::Duration::from_millis(200)),
        Err(mpsc::RecvTimeoutError::Timeout),
        "The notification callback should have been triggered only once."
    );
}

#[tokio::test]
async fn data_on_subscribe_test() {
    // Prepare Buffers
    let pub_buff = [0x11_i8; MESSAGE_SIZE as usize];

    // Prepare sync channels
    let (tx, rx): (
        mpsc::Sender<ffi_types::ASDV_NotificationType>,
        mpsc::Receiver<ffi_types::ASDV_NotificationType>,
    ) = mpsc::channel();

    let fqin =
        ServiceFqin::from_str("vm-name:com.google.testpkg.InternalCommsDtTest/data-on-subscribe")
            .expect("ServiceFqin should not be malformed");

    let ctx = sdv_comms_ctx::initialise(fqin).expect("Context initialisation failed");
    // SAFETY: Context object is alive for as long as it is required below so it is safe to
    // transmute into 'static lifetime.
    // TODO(b/361746197): Remove this transmute call once the Test framework is ready
    let ctx_ref = unsafe { std::mem::transmute::<&ContextImpl, &'static ContextImpl>(&ctx) };

    // Create and register sample publisher
    let unit_type = make_test_unit_type("DataTunnelNotificationsTestUnitType");
    let unit_name = "data-tunnel-data-on-subscribe-publication";
    let mut publisher = register_test_publisher(ctx_ref, unit_name, unit_type)
        .await
        .expect("Failed to create publication");

    // Write one message
    assert!(publisher.write_messages(&pub_buff, 1).unwrap());

    // Create a subscription with fetch_last_message flag and ensure callback is triggered
    // for the last previously published message
    let unit_identifier = UnitIdentifier::new(ctx.get_identity().clone(), unit_name.to_string());
    let mut subscriber = core::subscriber_internal::SubscriberInternal::subscribe_to_publication(
        ctx_ref,
        &unit_identifier,
        ffi_types::ASDV_SubscriptionFlags_FetchLastMessage,
    )
    .await
    .expect("Unable to subscribe");

    let available = subscriber.available_to_read().expect("Subscriber::available_to_read() failed");
    assert_eq!(available, 1, "Expected one message available to read");

    type CallbackFunc = CallbackWrapper<ffi_types::ASDV_NotificationType>;
    let cb = CallbackFunc::new({
        let tx = tx.clone();
        move |t| {
            tx.send(t).unwrap();
        }
    });
    let vtable = crate::ffi::ffi_types::ASDV_CallbackVTable_SetNotificationHandler::new(cb);
    subscriber.set_notification_handler(vtable.into(), 0).unwrap();

    // The notification callback must be triggered right after we set it up since we use
    // ASDV_SubscriptionFlags_FetchLastMessage and the publication already have multiple messages
    // published earlier.
    assert_eq!(
        rx.recv_timeout(time::Duration::from_millis(100)),
        Ok(ffi_types::ASDV_NotificationType::DataAvailable)
    );
    assert_eq!(
        rx.recv_timeout(time::Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout),
        "The notification callback should have been triggered only once."
    );
}

#[tokio::test]
async fn min_time_interval_test() {
    // Prepare Buffers
    let pub_buff = [0x11_i8; MESSAGE_SIZE as usize];

    let fqin =
        ServiceFqin::from_str("vm-name:com.google.testpkg.InternalCommsDtTest/min-time-interval")
            .expect("ServiceFqin should not be malformed");

    let ctx = sdv_comms_ctx::initialise(fqin).expect("Context initialisation failed");
    // SAFETY: Context object is alive for as long as it is required below so it is safe to
    // transmute into 'static lifetime.
    // TODO(b/361746197): Remove this transmute call once the Test framework is ready
    let ctx_ref = unsafe { std::mem::transmute::<&ContextImpl, &'static ContextImpl>(&ctx) };

    // Create and register sample publisher
    let unit_type = make_test_unit_type("DataTunnelMinTimeIntervalTestUnitType");
    let unit_name = "data-tunnel-min-time-interval-publication";
    let mut publisher = register_test_publisher(ctx_ref, unit_name, unit_type)
        .await
        .expect("Failed to create publication");

    // Subscribe to the registered publisher
    let unit_identifier = UnitIdentifier::new(ctx.get_identity().clone(), unit_name.to_string());
    let mut subscriber = core::subscriber_internal::SubscriberInternal::subscribe_to_publication(
        ctx_ref,
        &unit_identifier,
        0,
    )
    .await
    .expect("Unable to subscribe");

    // Set notification callback for the subscription with min time interval 100 msec
    let (tx, rx): (mpsc::Sender<time::SystemTime>, mpsc::Receiver<time::SystemTime>) =
        mpsc::channel();

    type CallbackFunc = CallbackWrapper<ffi_types::ASDV_NotificationType>;
    let cb = CallbackFunc::new({
        move |_| {
            tx.send(time::SystemTime::now()).unwrap();
        }
    });

    let vtable = crate::ffi::ffi_types::ASDV_CallbackVTable_SetNotificationHandler::new(cb);
    subscriber.set_notification_handler(vtable.into(), 100).unwrap();

    // Write 20 messages with 8 msec interval between each one
    for _ in 0..20 {
        assert!(publisher.write_messages(&pub_buff, 1).unwrap());
        thread::sleep(time::Duration::from_millis(8));
    }
    // Write 20 messages with 38 msec interval between each one
    for _ in 0..20 {
        assert!(publisher.write_messages(&pub_buff, 1).unwrap());
        thread::sleep(time::Duration::from_millis(38));
    }
    // Write 20 messages with 68 msec interval between each one
    for _ in 0..20 {
        assert!(publisher.write_messages(&pub_buff, 1).unwrap());
        thread::sleep(time::Duration::from_millis(68));
    }

    // Check all the triggered callbacks
    let mut last_invoke: Option<time::SystemTime> = None;
    let mut last_available: usize = 0;
    let it = rx.try_iter();
    for v in it {
        if let Some(last) = last_invoke {
            let millis = v.duration_since(last).unwrap().as_millis();
            // expect +/- 5% tollerance
            assert!(millis > 95u128 && millis < 105u128);
            last_available = subscriber.available_to_read().unwrap();
        }
        last_invoke = Some(v);
    }
    // Ensure we have all 60 messages available to read at the time of the list notifications has triggered
    assert_eq!(last_available, 60);
}

#[tokio::test]
async fn history_reader_test() {
    // Prepare Buffers
    let mut pub_buff = [0x11_i8; MESSAGE_SIZE as usize];
    let mut sub_buff = [0x00_i8; MESSAGE_SIZE as usize];

    let fqin =
        ServiceFqin::from_str("vm-name:com.google.testpkg.InternalCommsDtTest/history-reader")
            .expect("ServiceFqin should not be malformed");

    let ctx = sdv_comms_ctx::initialise(fqin).expect("Context initialisation failed");
    // SAFETY: Context object is alive for as long as it is required below so it is safe to
    // transmute into 'static lifetime.
    // TODO(b/361746197): Remove this transmute call once the Test framework is ready
    let ctx_ref = unsafe { std::mem::transmute::<&ContextImpl, &'static ContextImpl>(&ctx) };

    // Create and register sample publisher
    let unit_type = make_test_unit_type("DataTunnelHistoryReaderTestUnitType");
    let unit_name = "data-tunnel-history-reader-test-publication";
    let mut publisher = register_test_publisher(ctx_ref, unit_name, unit_type)
        .await
        .expect("Failed to create publication");

    let unit_identifier = UnitIdentifier::new(ctx.get_identity().clone(), unit_name.to_string());
    // Try to subscribe to history with zero sized buffer
    assert!(
        core::history_internal::HistoryInternal::subscribe_to_history(ctx_ref, &unit_identifier, 0)
            .await
            .is_err(),
        "It should be impossible to create a zero-sized History"
    );
    // Subscribe to the publication using a History reader instance
    let history_size = MESSAGE_COUNT + 1;
    let mut history_reader = core::history_internal::HistoryInternal::subscribe_to_history(
        ctx_ref,
        &unit_identifier,
        history_size,
    )
    .await
    .expect("Unable to create a history reader");

    thread::sleep(time::Duration::from_millis(200));
    let mut initial_count = history_reader.available_in_history().unwrap();
    pub_buff.fill(0i8);
    assert!(publisher.write_messages(&pub_buff, 1).unwrap());
    initial_count += 1;
    thread::sleep(time::Duration::from_millis(200));
    assert_eq!(history_reader.available_in_history().unwrap(), initial_count);

    for x in initial_count..(history_size as usize) {
        pub_buff.fill((x % 128) as i8);
        assert!(publisher.write_messages(&pub_buff, 1).unwrap());
        thread::sleep(time::Duration::from_millis(50));
    }

    // check if message is available in history
    thread::sleep(time::Duration::from_millis(200));
    assert_eq!(history_reader.available_in_history().unwrap(), history_size as usize);

    // check last message buffer content
    assert_eq!(history_reader.read(&mut sub_buff, 1u32).unwrap(), 1u32);
    assert_eq!(pub_buff, sub_buff);
}

#[tokio::test]
async fn history_reader_overflow_test() {
    let fqin = ServiceFqin::from_str(
        "vm-name:com.google.testpkg.InternalCommsDtTest/history-reader-overflow",
    )
    .expect("ServiceFqin should not be malformed");

    let ctx = sdv_comms_ctx::initialise(fqin).expect("Context initialisation failed");
    // SAFETY: Context object is alive for as long as it is required below so it is safe to
    // transmute into 'static lifetime.
    // TODO(b/361746197): Remove this transmute call once the Test framework is ready
    let ctx_ref = unsafe { std::mem::transmute::<&ContextImpl, &'static ContextImpl>(&ctx) };

    // Create and register sample publisher
    let unit_type = make_test_unit_type("DataTunnelHistoryReaderOverflowTestUnitType");
    let unit_name = "data-tunnel-history-reader-overflow-test-publication";
    let mut publisher = register_test_publisher(ctx_ref, unit_name, unit_type)
        .await
        .expect("Failed to create publication");

    let unit_identifier = UnitIdentifier::new(ctx.get_identity().clone(), unit_name.to_string());
    // Try to subscribe to history with zero sized buffer
    assert!(
        core::history_internal::HistoryInternal::subscribe_to_history(ctx_ref, &unit_identifier, 0)
            .await
            .is_err(),
        "It should be impossible to create a zero-sized History"
    );
    // Subscribe to the publication using a History reader instance
    let history_message_count = MESSAGE_COUNT / 2 + 1;
    let mut history_reader = core::history_internal::HistoryInternal::subscribe_to_history(
        ctx_ref,
        &unit_identifier,
        history_message_count,
    )
    .await
    .expect("Unable to create a history reader");

    // Prepare publisher buffer
    let mut pub_buff = [0x11_i8; (MESSAGE_SIZE * (MESSAGE_COUNT + 1)) as usize];
    pub_buff.fill(-127i8);

    // Write one message and wait until it falls into the history buffer
    assert!(publisher.write_messages(&pub_buff[0..(MESSAGE_SIZE) as usize], 1).unwrap());
    for it in 0..=10 {
        if history_reader.available_in_history().unwrap() == 1 {
            break;
        }
        thread::sleep(time::Duration::from_millis(20));
        assert!(it < 10);
    }

    // Fill publisher buffer with sequential messages
    let message_size = MESSAGE_SIZE as usize;
    for x in 0..(MESSAGE_COUNT + 1) as usize {
        pub_buff[(x * message_size)..((x + 1) * message_size)].fill((x % 128) as i8);
    }

    // Write COUNT+1 messages to the publication to trigger overflow
    assert!(publisher
        .write_messages_quiet(
            &pub_buff[0..(MESSAGE_COUNT / 2 * MESSAGE_SIZE) as usize],
            MESSAGE_COUNT / 2
        )
        .unwrap());
    assert!(publisher
        .write_messages(
            &pub_buff[(MESSAGE_COUNT / 2 * MESSAGE_SIZE) as usize..],
            MESSAGE_COUNT / 2 + 1
        )
        .unwrap());

    // wait until the new messages fall into the history buffer
    for it in 0..=10 {
        if history_reader.available_in_history().unwrap() > 1 {
            break;
        }
        thread::sleep(time::Duration::from_millis(20));
        assert!(it < 10);
    }

    assert_eq!(history_reader.available_in_history().unwrap(), (MESSAGE_COUNT / 2) as usize);

    // check last message buffer content
    let mut sub_buff = [0x00_i8; (MESSAGE_SIZE * MESSAGE_COUNT / 2) as usize];
    assert_eq!(history_reader.read(&mut sub_buff, MESSAGE_COUNT / 2).unwrap(), MESSAGE_COUNT / 2);
    assert_eq!(pub_buff[((MESSAGE_COUNT / 2 + 1) * MESSAGE_SIZE) as usize..], sub_buff);
}

#[tokio::test]
async fn instant_reader_test() {
    // Prepare Buffers
    let mut pub_buff = [0x11_i8; MESSAGE_SIZE as usize];
    let mut sub_buff = [0x00_i8; MESSAGE_SIZE as usize];

    let fqin =
        ServiceFqin::from_str("vm-name:com.google.testpkg.InternalCommsDtTest/instant-reader")
            .expect("ServiceFqin should not be malformed");

    let ctx = sdv_comms_ctx::initialise(fqin).expect("Context initialisation failed");
    // SAFETY: Context object is alive for as long as it is required below so it is safe to
    // transmute into 'static lifetime.
    // TODO(b/361746197): Remove this transmute call once the Test framework is ready
    let ctx_ref = unsafe { std::mem::transmute::<&ContextImpl, &'static ContextImpl>(&ctx) };

    // Create and register sample publisher
    let unit_type = make_test_unit_type("DataTunnelInstantReaderTestUnitType");
    let unit_name = "data-tunnel-instant-reader-test-publication";
    let mut publisher = register_test_publisher(ctx_ref, unit_name, unit_type)
        .await
        .expect("Failed to create publication");

    // Try to fetch the last message from the empty publication
    let unit_identifier = UnitIdentifier::new(ctx.get_identity().clone(), unit_name.to_string());
    let message_size = core::instant_reader_internal::get_last_publication_message(
        &ctx,
        &unit_identifier,
        &mut sub_buff,
    )
    .await
    .expect("Unable to fetch the last message from an empty publication. Expected Ok() with empty buffer");
    assert_eq!(message_size, 0, "Expected empty buffer");

    // Write two messages
    pub_buff.fill(0x10_i8);
    assert!(publisher.write_messages(&pub_buff, 1).unwrap());
    pub_buff.fill(0x20_i8);
    assert!(publisher.write_messages(&pub_buff, 1).unwrap());

    // Fetch the last message and check it
    core::instant_reader_internal::get_last_publication_message(
        &ctx,
        &unit_identifier,
        &mut sub_buff,
    )
    .await
    .expect("Unable to fetch the last message");
    // Ensure the fetched message is the last written one
    assert_eq!(pub_buff, sub_buff);
}

#[tokio::test]
async fn multipub_test() {
    // Multipub depends on a SD listener that needs a running binder thread.
    binder::ProcessState::start_thread_pool();

    // Prepare Buffers
    let mut pub_buff = [0x0A_i8; MESSAGE_SIZE as usize];
    let mut sub_buff = [0x0F_i8; 2 * MESSAGE_SIZE as usize];

    let fqin = ServiceFqin::from_str("vm-name:com.google.testpkg.InternalCommsDtTest/multipub")
        .expect("ServiceFqin should not be malformed");

    let ctx = sdv_comms_ctx::initialise(fqin).expect("Context initialisation failed");
    // SAFETY: Context object is alive for as long as it is required below so it is safe to
    // transmute into 'static lifetime.
    // TODO(b/361746197): Remove this transmute call once the Test framework is ready
    let ctx_ref = unsafe { std::mem::transmute::<&ContextImpl, &'static ContextImpl>(&ctx) };
    let comm_channel = &crate::core::state::get_state(&ctx).unwrap().comm_channel;

    // Create and register 3 different publications with the same unit type
    let unit_type = make_test_unit_type("DataTunnelMultipubTestUnitType");
    let unit_name_a = "data-tunnel-multipub-test-publication-a";
    let mut publisher_a = register_test_publisher(ctx_ref, unit_name_a, unit_type.clone())
        .await
        .expect("Failed to create publication");

    // Subscribe to all the publications of the specified unit type using a Multipub
    // Subscriber instance
    let mut multipub_subscriber =
        core::subscriber_internal::SubscriberInternal::subscribe_to_unit_type(
            ctx_ref, &unit_type, 0,
        )
        .await
        .expect("Unable to create a multipub subscription");

    // Ensure the multipub subscriber had enough time to init
    thread::sleep(time::Duration::from_millis(300));

    assert_eq!(multipub_subscriber.get_multiplexer().get_publication_count(), 1);
    assert_eq!(comm_channel.read().unwrap().get_callback_count(), 0);

    let unit_name_b = "data-tunnel-multipub-test-publication-b";
    let mut publisher_b = register_test_publisher(ctx_ref, unit_name_b, unit_type.clone())
        .await
        .expect("Failed to create publication");

    let unit_name_c = "data-tunnel-multipub-test-publication-c";
    let mut publisher_c = register_test_publisher(ctx_ref, unit_name_c, unit_type.clone())
        .await
        .expect("Failed to create publication");

    // Ensure the multipub subscriber had enough time to update
    thread::sleep(time::Duration::from_millis(300));

    // Check counts
    assert_eq!(multipub_subscriber.get_multiplexer().get_publication_count(), 3);
    assert_eq!(comm_channel.read().unwrap().get_callback_count(), 0);

    // Set notification handler
    let notification_received = Arc::new(AtomicBool::new(false));

    type CallbackFunc = CallbackWrapper<crate::ffi::ffi_types::ASDV_NotificationType>;
    let notification_received_clone = notification_received.clone();
    let cb = CallbackFunc::new(move |_t| {
        notification_received_clone.store(true, Ordering::SeqCst);
    });

    let vtable = crate::ffi::ffi_types::ASDV_CallbackVTable_SetNotificationHandler::new(cb);
    multipub_subscriber.set_notification_handler(vtable.into(), 0).unwrap();
    // Now we have callbacks since the app requested a handler
    assert_eq!(comm_channel.read().unwrap().get_callback_count(), 3);

    // Write one message to each of 3 publications
    pub_buff.fill(0x00_i8);
    assert!(publisher_a.write_messages(&pub_buff, 1).unwrap());
    pub_buff.fill(0x01_i8);
    assert!(publisher_b.write_messages(&pub_buff, 1).unwrap());
    pub_buff.fill(0x02_i8);
    assert!(publisher_c.write_messages(&pub_buff, 1).unwrap());

    // Check the multipub subscriber messages
    thread::sleep(time::Duration::from_millis(100));
    assert_eq!(multipub_subscriber.available_to_read().unwrap(), 3);

    // Ensure notification was triggered
    assert!(notification_received.load(Ordering::SeqCst));

    // Read and validate the received messages
    let mut values_received = [false; 3];
    for _ in 0..3 {
        assert_eq!(multipub_subscriber.read(&mut sub_buff, 1).unwrap(), 1);
        let val = sub_buff[0];
        assert!((0..3).contains(&val), "The received message content is unexpected");
        assert!(!values_received[val as usize], "Dublicated message detected");
        values_received[val as usize] = true;
    }
    assert!(
        values_received[0] && values_received[1] && values_received[2],
        "Not all messages were received"
    );

    // Write one more message from a publisher, then drop it
    pub_buff.fill(0x04_i8);
    assert!(publisher_c.write_messages(&pub_buff, 1).unwrap());

    // Drop publisher that just wrote.
    drop(publisher_c);

    thread::sleep(time::Duration::from_millis(100));
    // Publication count stays the same because there's outstanding data
    assert_eq!(multipub_subscriber.get_multiplexer().get_publication_count(), 3);
    // Callback will have been dropped due to exception message from publisher drop.
    assert_eq!(comm_channel.read().unwrap().get_callback_count(), 2);

    // Last written message readable even though publisher is gone
    assert_eq!(multipub_subscriber.available_to_read().unwrap(), 1);
    assert_eq!(multipub_subscriber.read(&mut sub_buff, 1).unwrap(), 1);
    assert_eq!(sub_buff[0], 0x4_i8);

    // *Now* the publication is removed, since we finished reading its data
    assert_eq!(multipub_subscriber.get_multiplexer().get_publication_count(), 2);
    assert_eq!(comm_channel.read().unwrap().get_callback_count(), 2);

    // Remove app callback, and the comm channel should have none
    multipub_subscriber.clear_notification_handler().unwrap();
    assert_eq!(comm_channel.read().unwrap().get_callback_count(), 0);

    // Round-robin check:  First write a distinct message to each publisher
    pub_buff.fill(0x05_i8);
    assert!(publisher_a.write_messages(&pub_buff, 1).unwrap());
    pub_buff.fill(0x06_i8);
    assert!(publisher_b.write_messages(&pub_buff, 1).unwrap());

    assert_eq!(multipub_subscriber.read(&mut sub_buff, 1).unwrap(), 1);

    // Get ready to write another message; this will be written to the same
    // publisher that provided the message we just read, and we'll expect
    // this message to be produced first since we should start the next read
    // with the same publication we left off from before.
    pub_buff.fill(0x07_i8);
    let expected_second = if sub_buff[0] == 0x05_i8 {
        // Just read from pub A, write to it again.
        assert!(publisher_a.write_messages(&pub_buff, 1).unwrap());
        0x06_i8
    } else {
        // Just read from pub B, write to it again.
        assert!(publisher_b.write_messages(&pub_buff, 1).unwrap());
        0x05_i8
    };

    // Read two more messages and confirm that the message written to the
    // previously-read publication is produced first, followed by the
    // message written to the previously-unread publication.
    assert_eq!(multipub_subscriber.read(&mut sub_buff, 2).unwrap(), 2);
    assert_eq!(sub_buff[0], 0x07_i8);
    assert_eq!(sub_buff[MESSAGE_SIZE as usize], expected_second);

    thread::sleep(time::Duration::from_millis(1000));

    // Check that exception messages (such as the TopicDestroyed generated when
    // a publisher is deleted) don't get propagated to multipub notification handler.

    // Clear the notification_received flag
    notification_received.store(false, Ordering::SeqCst);

    // Drop remaining publishers -- causes DT Agent to send Exception.
    drop(publisher_a);
    drop(publisher_b);

    // Wait for propagation, if any.
    thread::sleep(time::Duration::from_millis(1000));

    assert!(!notification_received.load(Ordering::SeqCst));
}
