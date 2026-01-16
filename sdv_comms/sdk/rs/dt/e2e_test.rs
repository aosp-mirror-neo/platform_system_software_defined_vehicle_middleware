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
use futures::future;
use futures::stream::StreamExt;
use sdv_comms_bindgen::ASDV_UnitIdentifier;
use sdv_comms_common::ffi_str;
use sdv_status_rs::{SdvResult, SdvStatusCode};
use std::default::Default;
use std::str::FromStr;
use std::thread;
use std::time::Duration;

use crate::dt::{
    HistoryReader, InstantReader, Publisher, Subscriber, SubscriberOptions, SubscriberStream,
};
use crate::id::{ServiceFqin, ServiceIdentity, UnitIdentifier};
use crate::sd::{PublisherUnitMetadata, UnitMetadata, UnitRegistrationArgs, UnitType};
use crate::ContextRef;

const MESSAGE_SIZE: u32 = 16;
const MESSAGE_COUNT: u32 = 64;
const BUFFER_SIZE: usize = MESSAGE_SIZE as usize;

fn make_unit_type(unit_type_suffix: &str) -> SdvResult<UnitType> {
    UnitType::from_str(
        format!("com.google.comms_stack_tests.DTE2Etests#{unit_type_suffix}").as_str(),
    )
}

async fn create_publisher_with_channel(
    ctx: ContextRef,
    unit_type: &UnitType,
    unit_name: &str,
    channel_name: &str,
) -> SdvResult<Publisher> {
    let mut unit_registration_args = UnitRegistrationArgs::new();
    let mut publisher_metadata = PublisherUnitMetadata::new();
    publisher_metadata.set_message_size(MESSAGE_SIZE);
    publisher_metadata.set_message_count(MESSAGE_COUNT);
    publisher_metadata.set_channel(channel_name);
    unit_registration_args.set_unit_type(unit_type);
    unit_registration_args.set_unit_name(unit_name);
    unit_registration_args.set_unit_metadata(&UnitMetadata::Publisher(publisher_metadata));

    Publisher::create(ctx, unit_registration_args).await
}

async fn create_publisher_with_unittype_and_metadata(
    ctx: ContextRef,
    unit_type: &UnitType,
    unit_name: &str,
    message_size: u32,
    message_count: u32,
) -> SdvResult<Publisher> {
    let mut unit_registration_args = UnitRegistrationArgs::new();
    let mut publisher_metadata = PublisherUnitMetadata::new();
    publisher_metadata.set_message_size(message_size);
    publisher_metadata.set_message_count(message_count);
    unit_registration_args.set_unit_type(unit_type);
    unit_registration_args.set_unit_name(unit_name);
    unit_registration_args.set_unit_metadata(&UnitMetadata::Publisher(publisher_metadata));

    Publisher::create(ctx, unit_registration_args).await
}

async fn create_publisher_with_unittype(
    ctx: ContextRef,
    unit_type: &UnitType,
    unit_name: &str,
) -> SdvResult<Publisher> {
    create_publisher_with_unittype_and_metadata(
        ctx,
        unit_type,
        unit_name,
        MESSAGE_SIZE,
        MESSAGE_COUNT,
    )
    .await
}

async fn create_publisher(ctx: ContextRef, unit_name: &str) -> SdvResult<Publisher> {
    let unit_type = make_unit_type("TestTopic").expect("Valid UnitType");
    create_publisher_with_unittype(ctx, &unit_type, unit_name).await
}

fn make_unit_identifier(ctx: ContextRef, unit_name: &str) -> UnitIdentifier {
    let mut asdv_unit_identifier = ASDV_UnitIdentifier {
        service_identity: ServiceIdentity::peel(ctx.get_self_identity()),
        ..Default::default()
    };
    ffi_str::str_to_bytes(unit_name, &mut asdv_unit_identifier.unit_name).unwrap();
    UnitIdentifier::wrap(asdv_unit_identifier)
}

async fn create_subscriber(ctx: ContextRef, unit_id: &UnitIdentifier) -> SdvResult<Subscriber> {
    let options = SubscriberOptions { fetch_last_message: false };
    Subscriber::create(ctx, unit_id, options).await
}

async fn create_history_reader(
    ctx: ContextRef,
    unit_id: &UnitIdentifier,
    history_size: u32,
) -> SdvResult<HistoryReader> {
    HistoryReader::create(ctx, unit_id, history_size).await
}

async fn create_subscriber_stream(
    ctx: ContextRef,
    unit_id: &UnitIdentifier,
    fetch_last_message: bool,
) -> SdvResult<SubscriberStream> {
    let options = SubscriberOptions { fetch_last_message };
    SubscriberStream::create(ctx, unit_id, Duration::ZERO, options).await
}

#[tokio::test]
async fn dt_e2e_success() {
    // Spawn a new task to ensure all futures are Send.
    tokio::spawn(async {
        let pub_buff = [0x11_u8; BUFFER_SIZE];
        let mut sub_buff = [0x00_u8; BUFFER_SIZE];

        let fqin =
            ServiceFqin::from_str("vm-name:com.google.comms_stack_tests.DTE2Etests/instance")
                .expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_name = "test-publisher";
        let unit_id = make_unit_identifier(ctx, unit_name);

        let publisher = create_publisher(ctx, unit_name).await.expect("Publisher creation failed");
        let subscriber =
            create_subscriber(ctx, &unit_id).await.expect("Subscriber creation failed");

        let write_messages = publisher.write_messages(&pub_buff).expect("Failed to write messages");
        assert_eq!(write_messages, 1);

        let available_messages =
            subscriber.available_to_read().expect("Failed to get messages available to read");
        assert_eq!(available_messages, 1);

        // Verify expected behavior from zero-length read.
        let read_zero_messages =
            subscriber.read_messages(&mut []).expect("Failed to read zero messages");
        assert_eq!(read_zero_messages, 0);
        assert_ne!(pub_buff, sub_buff);
        let available_messages =
            subscriber.available_to_read().expect("Failed to get messages available to read");
        assert_eq!(available_messages, 1);

        // Actually read a message.
        let read_messages =
            subscriber.read_messages(&mut sub_buff).expect("Failed to read messages");
        assert_eq!(read_messages, 1);
        assert_eq!(pub_buff, sub_buff);
        let available_messages =
            subscriber.available_to_read().expect("Failed to get messages available to read");
        assert_eq!(available_messages, 0);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dt_e2e_overflow_handling() {
    // Spawn a new task to ensure all futures are Send.
    tokio::spawn(async {
        let pub_buff = [0x11_u8; BUFFER_SIZE];
        let mut sub_buff = [0x00_u8; BUFFER_SIZE];

        let fqin =
            ServiceFqin::from_str("vm-name:com.google.comms_stack_tests.DTE2Etests/instance")
                .expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_name = "test-publisher";
        let unit_id = make_unit_identifier(ctx, unit_name);

        let publisher = create_publisher(ctx, unit_name).await.expect("Publisher creation failed");
        let subscriber =
            create_subscriber(ctx, &unit_id).await.expect("Subscriber creation failed");

        // Overflow the publisher's buffer
        for _ in 0..=MESSAGE_COUNT {
            assert_eq!(
                (publisher.write_messages(&pub_buff)).expect("publisher.write_messages() failed"),
                1u32
            );
        }

        // Attempt read, expect overflow error.
        let read_messages = subscriber.read_messages(&mut sub_buff);
        assert!(read_messages.is_err(), "Expecting an error after overflow read.");
        assert_eq!(
            read_messages.unwrap_err().status_code(),
            SdvStatusCode::DataLoss,
            "Expecting SdvStatusCode::DataLoss status after overflow"
        );

        // Expect half of the messages to remain.
        assert_eq!(subscriber.available_to_read().unwrap(), MESSAGE_COUNT / 2);

        // Overflow the publisher's buffer
        for _ in 0..=MESSAGE_COUNT {
            assert_eq!(
                (publisher.write_messages(&pub_buff)).expect("publisher.write_messages() failed"),
                1u32
            );
        }

        // Attempt 0-length read, expect overflow error.
        let read_messages = subscriber.read_messages(&mut []);
        assert!(read_messages.is_err(), "Expecting an error after overflow read.");
        assert_eq!(
            read_messages.unwrap_err().status_code(),
            SdvStatusCode::DataLoss,
            "Expecting SdvStatusCode::DataLoss status after overflow"
        );

        // Expect half of the messages to remain.
        assert_eq!(subscriber.available_to_read().unwrap(), MESSAGE_COUNT / 2);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dt_e2e_read_more_than_available() {
    // Spawn a new task to ensure all futures are Send.
    tokio::spawn(async {
        let mut sub_buff = [0x00_u8; BUFFER_SIZE * 2];

        let fqin = ServiceFqin::from_str(
            "vm-name:com.google.comms_stack_tests.DTE2EReadMoreThanAvailable/instance",
        )
        .expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_name = "test-publisher";
        let unit_id = make_unit_identifier(ctx, unit_name);

        let publisher = create_publisher(ctx, unit_name).await.expect("Publisher creation failed");
        let subscriber =
            create_subscriber(ctx, &unit_id).await.expect("Subscriber creation failed");

        // Read when nothing yet available.
        assert_eq!(subscriber.read_messages(&mut sub_buff).unwrap(), 0);

        // Write a single message.
        let pub_buff = [0x11_u8; BUFFER_SIZE];
        let write_messages = publisher.write_messages(&pub_buff).unwrap();
        assert_eq!(write_messages, 1);

        // Try to read two messages
        let mut sub_buff = [0x00_u8; BUFFER_SIZE * 2];
        let read_messages = subscriber.read_messages(&mut sub_buff).unwrap();

        assert_eq!(read_messages, 1);
        assert_eq!(pub_buff, &sub_buff[..BUFFER_SIZE]);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dt_e2e_read_after_publisher_shutdown() {
    // Spawn a new task to ensure all futures are Send.
    tokio::spawn(async {
        let pub_buff = [0x11_u8; BUFFER_SIZE];
        let mut sub_buff = [0x00_u8; BUFFER_SIZE];

        let fqin = ServiceFqin::from_str(
            "vm-name:com.google.comms_stack_tests.DTE2EReadAfterPublisherShutdown/instance",
        )
        .expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_name = "test-publisher";
        let unit_id = make_unit_identifier(ctx, unit_name);

        let publisher = create_publisher(ctx, unit_name).await.expect("Publisher creation failed");
        let subscriber =
            create_subscriber(ctx, &unit_id).await.expect("Subscriber creation failed");

        let write_messages = publisher.write_messages(&pub_buff).expect("Failed to write messages");
        drop(publisher);

        // Even though the publisher is gone we can still read the message.
        let available_messages =
            subscriber.available_to_read().expect("Failed to get messages available to read");
        let read_messages =
            subscriber.read_messages(&mut sub_buff).expect("Failed to read messages");

        assert_eq!(write_messages, 1);
        assert_eq!(available_messages, 1);
        assert_eq!(read_messages, 1);
        assert_eq!(pub_buff, sub_buff);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dt_publisher_drop_success() {
    // Spawn a new task to ensure all futures are Send.
    tokio::spawn(async {
        let fqin = ServiceFqin::from_str(
            "vm-name:com.google.comms_stack_tests.DTPublisherDropTests/instance",
        )
        .expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_name = "test-publisher";
        let publisher = create_publisher(ctx, unit_name).await.expect("Publisher creation failed");
        assert!(
            create_publisher(ctx, unit_name).await.is_err(),
            "Re-registering a publication with the same name should return an error"
        );
        drop(publisher);
        let _publisher = create_publisher(ctx, unit_name)
            .await
            .expect("Failed to re-register the publication after drop");
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dt_multipub_success() {
    // Multipub depends on a SD listener that needs a running binder thread.
    binder::ProcessState::start_thread_pool();

    // Spawn a new task to ensure all futures are Send.
    tokio::spawn(async {
        let mut pub_buff = [0x11_u8; BUFFER_SIZE];
        let mut sub_buff = [0x00_u8; BUFFER_SIZE];

        let fqin = ServiceFqin::from_str(
            "vm-name:com.google.comms_stack_tests.DTMultipubSuccess/instance",
        )
        .expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        // Register 3 different publications with the same UnitType
        let unit_type = make_unit_type("MultipubTestTopic").expect("Valid UnitType");
        let publisher_a = create_publisher_with_unittype(ctx, &unit_type, "test-publisher-a")
            .await
            .expect("Publisher creation failed");
        let publisher_b = create_publisher_with_unittype(ctx, &unit_type, "test-publisher-b")
            .await
            .expect("Publisher creation failed");

        // Subscribe to all the publications of the specified unit type using a Multipub
        // Subscriber instance
        let options = SubscriberOptions { fetch_last_message: false };
        let multipub_subscriber = Subscriber::create_for_multipub(ctx, &unit_type, options)
            .await
            .expect("Subscriber creation failed");

        // Ensure the multipub subscriber had enough time to init
        thread::sleep(Duration::from_millis(200));

        // Write one message to each of 2 publications
        // The messages are filled with different values so that they can be distinguished
        // while reading.
        pub_buff.fill(0x00_u8);
        assert_eq!(publisher_a.write_messages(&pub_buff).expect("Failed to write messages"), 1);
        pub_buff.fill(0x01_u8);
        assert_eq!(publisher_b.write_messages(&pub_buff).expect("Failed to write messages"), 1);

        // Check the multipub subscriber messages
        thread::sleep(Duration::from_millis(200));
        assert_eq!(
            multipub_subscriber
                .available_to_read()
                .expect("Failed to get messages available to read"),
            2
        );

        // Add another publisher
        let publisher_c = create_publisher_with_unittype(ctx, &unit_type, "test-publisher-c")
            .await
            .expect("Publisher creation failed");

        // Give multipub subscriber time to learn of new topic.
        thread::sleep(Duration::from_millis(200));

        pub_buff.fill(0x02_u8);
        assert_eq!(publisher_c.write_messages(&pub_buff).expect("Failed to write messages"), 1);

        // Check the multipub subscriber messages
        thread::sleep(Duration::from_millis(200));
        assert_eq!(
            multipub_subscriber
                .available_to_read()
                .expect("Failed to get messages available to read"),
            3
        );

        // Drop one of the publishers; we should still get its message.
        drop(publisher_b);

        // Read and validate the received messages
        let mut message_received = [false; 3];
        for _ in 0..3 {
            assert_eq!(
                multipub_subscriber.read_messages(&mut sub_buff).expect("Failed to read messages"),
                1
            );
            let val = sub_buff[0];
            // We're expecting to receive messages fills with 0x00, 0x01 or 0x02
            assert!((0..3).contains(&val), "The received message content is unexpected");
            assert!(!message_received[val as usize], "Duplicated message detected");
            message_received[val as usize] = true;
        }
        // Make sure we received all 3 messages
        assert!(
            message_received[0] && message_received[1] && message_received[2],
            "Not all messages were received"
        );
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dt_multipub_read_more_than_available() {
    // Multipub depends on a SD listener that needs a running binder thread.
    binder::ProcessState::start_thread_pool();

    // Spawn a new task to ensure all futures are Send.
    tokio::spawn(async {
        let fqin = ServiceFqin::from_str(
            "vm-name:com.google.comms_stack_tests.DTMultipubReadMoreThanAvailable/instance",
        )
        .expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        // Register 3 different publications with the same UnitType
        let unit_type = make_unit_type("MultipubReadTooMuchTestTopic").expect("Valid UnitType");
        let publisher_a = create_publisher_with_unittype(ctx, &unit_type, "test-publisher-a")
            .await
            .expect("Publisher creation failed");
        let publisher_b = create_publisher_with_unittype(ctx, &unit_type, "test-publisher-b")
            .await
            .expect("Publisher creation failed");

        // Subscribe to all the publications of the specified unit type using a Multipub
        // Subscriber instance
        let options = SubscriberOptions { fetch_last_message: false };
        let multipub_subscriber = Subscriber::create_for_multipub(ctx, &unit_type, options)
            .await
            .expect("Subscriber creation failed");

        // Ensure the multipub subscriber had enough time to init
        thread::sleep(Duration::from_millis(200));

        // Try to read before anything is written, get success but 0 read count.
        let mut sub_buff = [0x00_u8; BUFFER_SIZE];
        assert_eq!(multipub_subscriber.read_messages(&mut sub_buff).unwrap(), 0);

        // Write one message to each of 2 publications
        // The messages are filled with different values so that they can be distinguished
        // while reading.
        let pub_buff_a = [0x00_u8; BUFFER_SIZE];
        assert_eq!(publisher_a.write_messages(&pub_buff_a).unwrap(), 1);
        let pub_buff_b = [0x01_u8; BUFFER_SIZE];
        assert_eq!(publisher_b.write_messages(&pub_buff_b).unwrap(), 1);

        // Read the messages; provide enough space for 3, but should only get 2.
        let mut sub_buff = [0x00_u8; BUFFER_SIZE * 3];
        assert_eq!(multipub_subscriber.read_messages(&mut sub_buff).unwrap(), 2);
        assert!(
            (sub_buff[0] == 0x00_u8 && sub_buff[MESSAGE_SIZE as usize] == 0x01_u8)
                || (sub_buff[0] == 0x01_u8 && sub_buff[MESSAGE_SIZE as usize] == 0x00_u8)
        );
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dt_history_success() {
    let history_size = MESSAGE_COUNT + 1;
    let mut pub_buff = [0x11_u8; BUFFER_SIZE];
    let mut sub_buff = [0x00_u8; BUFFER_SIZE];

    let fqin =
        ServiceFqin::from_str("vm-name:com.google.comms_stack_tests.DTE2EHistoryTests/instance")
            .expect("ServiceFqin should not be malformed");
    let ctx = ContextRef::create(fqin);

    let unit_name = "test-publisher";
    let unit_id = make_unit_identifier(ctx, unit_name);

    let publisher = create_publisher(ctx, unit_name).await.expect("Publisher creation failed");
    assert!(
        create_history_reader(ctx, &unit_id, 0).await.is_err(),
        "It should be impossible to create a zero-sized HistoryReader"
    );
    let history_reader = create_history_reader(ctx, &unit_id, history_size)
        .await
        .expect("HistoryReader creation failed");

    for x in 0..history_size {
        pub_buff.fill((x % 256) as u8);
        assert_eq!(
            (publisher.write_messages(&pub_buff)).expect("publisher.write_messages() failed"),
            1u32
        );
        thread::sleep(Duration::from_millis(50));
    }

    thread::sleep(Duration::from_millis(200));
    assert_eq!(
        history_reader.available_to_read().expect("Failed to get messages available to read"),
        history_size
    );
    assert_eq!(history_reader.read_last_messages(&mut sub_buff).unwrap(), 1u32);
    assert_eq!(pub_buff, sub_buff);
}

#[tokio::test]
async fn dt_instant_read_success() {
    let mut pub_buff = [0x11_u8; BUFFER_SIZE];

    let fqin = ServiceFqin::from_str(
        "vm-name:com.google.comms_stack_tests.DTE2EInstantReadTests/instance",
    )
    .expect("ServiceFqin should not be malformed");
    let ctx = ContextRef::create(fqin);

    let unit_name = "test-instant-read-publisher";
    let unit_id = make_unit_identifier(ctx, unit_name);

    let publisher = create_publisher(ctx, unit_name).await.expect("Publisher creation failed");

    // Try to fetch the last message from the empty publication
    let empty_message = InstantReader::get_last_message(ctx, &unit_id)
        .await
        .expect("Failed to read the last message from an empty publication");
    assert!(empty_message.is_empty(), "Expected empty buffer");

    // Write two messages
    pub_buff.fill(0x10_u8);
    assert_eq!(
        publisher.write_messages(&pub_buff).expect("publisher.write_messages() failed"),
        1u32
    );
    pub_buff.fill(0x20_u8);
    assert_eq!(
        publisher.write_messages(&pub_buff).expect("publisher.write_messages() failed"),
        1u32
    );

    // Fetch the last message and check its size
    let received_message = InstantReader::get_last_message(ctx, &unit_id)
        .await
        .expect("Failed to read the last message");
    assert_eq!(received_message.len(), pub_buff.len());

    // Ensure the fetched message is the last written one
    assert_eq!(pub_buff, &received_message[..]);
}

#[tokio::test]
async fn dt_e2e_stream_success() {
    // This test doesn't verify multipub functionality, as that is tested in dt_multipub_success
    let pub_buff = [0x11_u8; BUFFER_SIZE];

    let fqin =
        ServiceFqin::from_str("vm-name:com.google.comms_stack_tests.DTE2EStreamTests/instance")
            .expect("ServiceFqin should not be malformed");
    let ctx = ContextRef::create(fqin);

    let unit_name = "test-stream-publisher";
    let unit_id = make_unit_identifier(ctx, unit_name);

    let publisher = create_publisher(ctx, unit_name).await.expect("Publisher creation failed");
    let mut subscriber = create_subscriber_stream(ctx, &unit_id, false)
        .await
        .expect("Subscriber stream creation failed");

    let write_messages = publisher.write_messages(&pub_buff).expect("Failed to write messages");
    assert_eq!(write_messages, 1);

    if let Some(Ok((messages_read, data))) = subscriber.next().await {
        assert_eq!(messages_read, 1);
        assert_eq!(pub_buff, &data[..]);
    } else {
        panic!("Err in data received from stream");
    }

    let mut subscriber2 = create_subscriber_stream(ctx, &unit_id, true)
        .await
        .expect("Subscriber stream creation failed");

    // Check that new stream got last message.
    if let Some(Ok((messages_read, data))) = subscriber2.next().await {
        assert_eq!(messages_read, 1);
        assert_eq!(pub_buff, &data[..]);
    } else {
        panic!("Err in data received from stream");
    }

    // Do another write, check that both streams see it.
    let pub_buff2 = [0x22_u8; BUFFER_SIZE];
    let write_messages = publisher.write_messages(&pub_buff2).expect("Failed to write messages");
    assert_eq!(write_messages, 1);

    if let Some(Ok((messages_read, data))) = subscriber.next().await {
        assert_eq!(messages_read, 1);
        assert_eq!(pub_buff2, &data[..]);
    } else {
        panic!("Err in data received from stream");
    }

    if let Some(Ok((messages_read, data))) = subscriber2.next().await {
        assert_eq!(messages_read, 1);
        assert_eq!(pub_buff2, &data[..]);
    } else {
        panic!("Err in data received from stream");
    }

    // Get rid of one subscriber, do another write and make sure the other stream
    // still gets events.
    drop(subscriber);

    let pub_buff3 = [0x33_u8; BUFFER_SIZE];
    let write_messages = publisher.write_messages(&pub_buff3).expect("Failed to write messages");
    assert_eq!(write_messages, 1);

    if let Some(Ok((messages_read, data))) = subscriber2.next().await {
        assert_eq!(messages_read, 1);
        assert_eq!(pub_buff3, &data[..]);
    } else {
        panic!("Err in data received from stream");
    }
}

#[tokio::test]
async fn dt_e2e_notifications_success() {
    let pub_buff = [0x11_u8; BUFFER_SIZE];

    let fqin = ServiceFqin::from_str(
        "vm-name:com.google.comms_stack_tests.DTE2ENotificationsTests/instance",
    )
    .expect("ServiceFqin should not be malformed");
    let ctx = ContextRef::create(fqin);

    let unit_name = "test-notifications-publisher";
    let unit_id = make_unit_identifier(ctx, unit_name);

    let publisher = create_publisher(ctx, unit_name).await.expect("Publisher creation failed");
    let subscriber = create_subscriber_stream(ctx, &unit_id, false)
        .await
        .expect("Subscriber stream creation failed");

    // Try to write a lot of messages to exhaust the subscriber's notifications channel
    for _ in 1..2048 {
        let write_messages = publisher.write_messages(&pub_buff).expect("Failed to write messages");
        assert_eq!(write_messages, 1);
        thread::sleep(Duration::from_millis(10));
    }

    // Drop the publisher
    drop(publisher);

    // Ensure we know that the publication was destroyed by reading all the notifications until we get None
    let timeout_duration = Duration::from_secs(3);
    let mut subscriber_stream_op = subscriber.skip_while(|_| future::ready(true));
    tokio::select! {
        maybe_item = subscriber_stream_op.next() => {
            // This block executes if your stream operation finishes before the timeout.
            assert!(maybe_item.is_none(), "The last message expected to be empty but wasn't.");
        }
        // This branch runs the timer.
        _ = tokio::time::sleep(timeout_duration) => {
            // This block executes if the sleep() future completes first.
            panic!("Operation timed out after {timeout_duration:?}!");
        }
    }
}

#[tokio::test]
async fn dt_e2e_stream_no_zero_length_result() {
    let pub_buff = [0x11_u8; BUFFER_SIZE];

    let fqin =
        ServiceFqin::from_str("vm-name:com.google.comms_stack_tests.DTE2EStreamTests/instance")
            .expect("ServiceFqin should not be malformed");
    let ctx = ContextRef::create(fqin);

    let unit_name = "test-stream-publisher";
    let unit_id = make_unit_identifier(ctx, unit_name);

    let publisher = create_publisher(ctx, unit_name).await.expect("Publisher creation failed");
    let mut subscriber = create_subscriber_stream(ctx, &unit_id, false)
        .await
        .expect("Subscriber stream creation failed");

    let write_messages = publisher.write_messages(&pub_buff).expect("Failed to write messages");
    assert_eq!(write_messages, 1);

    // Write a second message as a separate call to generate a separate notification.
    let write_messages = publisher.write_messages(&pub_buff).expect("Failed to write messages");
    assert_eq!(write_messages, 1);

    if let Some(Ok((messages_read, data))) = subscriber.next().await {
        assert_eq!(messages_read, 2);
        assert_eq!(pub_buff, &data[..BUFFER_SIZE]);
        assert_eq!(pub_buff, &data[BUFFER_SIZE..]);
    } else {
        panic!("Err in data received from stream");
    }

    // Drop publisher; next read should be end-of-stream.
    drop(publisher);

    assert!(subscriber.next().await.is_none());
}

#[tokio::test]
async fn dt_e2e_stream_overflow_success() {
    let mut pub_buff = [0x11_u8; BUFFER_SIZE];

    let fqin = ServiceFqin::from_str(
        "vm-name:com.google.comms_stack_tests.DTE2EStreamOverflowTests/instance",
    )
    .expect("ServiceFqin should not be malformed");
    let ctx = ContextRef::create(fqin);

    let unit_name = "test-stream-overflow-publisher";
    let unit_id = make_unit_identifier(ctx, unit_name);

    let publisher = create_publisher(ctx, unit_name).await.expect("Publisher creation failed");
    let mut subscriber = create_subscriber_stream(ctx, &unit_id, false)
        .await
        .expect("Subscriber stream creation failed");

    // Overflow the publisher's buffer
    for x in 0..=MESSAGE_COUNT {
        pub_buff.fill((x % 256) as u8);
        assert_eq!(
            (publisher.write_messages(&pub_buff)).expect("publisher.write_messages() failed"),
            1u32
        );
    }

    // Fill pub_buff with the content of the first message after trimming half of the buffer
    pub_buff.fill(((MESSAGE_COUNT / 2 + 1) % 256) as u8);

    let item = subscriber.next().await;
    if let Some(Err(e)) = item {
        assert_eq!(
            e.status_code(),
            SdvStatusCode::DataLoss,
            "Expecting SdvStatusCode::DataLoss status after overflow"
        );
    } else {
        panic!("Expecting SdvStatusCode::DataLoss status after overflow. Actual result {item:?}");
    }

    let item2 = subscriber.next().await;
    if let Some(Ok((messages_read, data))) = item2 {
        assert_eq!(
            messages_read,
            MESSAGE_COUNT / 2,
            "Expecting half of buffer capacity is available to read"
        );
        assert_eq!(
            pub_buff,
            &data[0..(MESSAGE_SIZE as usize)],
            "Expecting the content of the first message after trimming half of the buffer"
        );
    } else {
        panic!("Expecting SdvStatusCode::Ok status. Actual result {item2:?}");
    }
}

#[tokio::test]
async fn dt_e2e_stream_read_all_messages_after_overflow_success() {
    let mut pub_buff = [0x11_u8; BUFFER_SIZE];

    let fqin = ServiceFqin::from_str(
        "vm-name:com.google.comms_stack_tests.DTE2EStreamReadAllMessagesAfterOverflowTests/instance",
    )
    .expect("ServiceFqin should not be malformed");
    let ctx = ContextRef::create(fqin);

    let unit_name = "test-stream-overflow-publisher";
    let unit_id = make_unit_identifier(ctx, unit_name);

    let publisher = create_publisher(ctx, unit_name).await.expect("Publisher creation failed");
    let mut subscriber = create_subscriber_stream(ctx, &unit_id, false)
        .await
        .expect("Subscriber stream creation failed");

    // Overflow the publisher's buffer
    for x in 0..=MESSAGE_COUNT {
        pub_buff.fill((x % 256) as u8);
        assert_eq!(
            (publisher.write_messages(&pub_buff)).expect("publisher.write_messages() failed"),
            1u32
        );
    }

    // Fill pub_buff with the content of the first message after trimming half of the buffer
    pub_buff.fill(((MESSAGE_COUNT / 2 + 1) % 256) as u8);

    let item = subscriber.next().await;
    if let Some(Err(e)) = item {
        assert_eq!(
            e.status_code(),
            SdvStatusCode::DataLoss,
            "Expecting SdvStatusCode::DataLoss status after overflow"
        );
    } else {
        panic!("Expecting SdvStatusCode::DataLoss status after overflow. Actual result {item:?}");
    }

    match subscriber.read_all_available_messages() {
        Ok((messages_read, data)) => {
            assert_eq!(
                messages_read,
                MESSAGE_COUNT / 2,
                "Expecting half of buffer capacity is available to read"
            );
            assert_eq!(
                pub_buff,
                &data[0..(MESSAGE_SIZE as usize)],
                "Expecting the content of the first message after trimming half of the buffer"
            );
        }
        Err(e) => {
            panic!("Expecting SdvStatusCode::Ok status. Actual result {e:?}");
        }
    }
}

#[tokio::test]
async fn dt_e2e_multipub_stream_success() {
    // Multipub depends on a SD listener that needs a running binder thread.
    binder::ProcessState::start_thread_pool();

    let pub_buff = [0x11_u8; BUFFER_SIZE];

    let fqin = ServiceFqin::from_str(
        "vm-name:com.google.comms_stack_tests.DTE2EMultipubStreamTests/instance",
    )
    .expect("ServiceFqin should not be malformed");
    let ctx = ContextRef::create(fqin);

    let unit_type = make_unit_type("MultipubStreamTestTopic").expect("Valid UnitType");
    let publisher = create_publisher_with_unittype(ctx, &unit_type, "test-publisher-a")
        .await
        .expect("Publisher creation failed");

    let options = SubscriberOptions { fetch_last_message: false };
    let multipub_subscriber =
        SubscriberStream::create_for_multipub(ctx, &unit_type, Duration::ZERO, options)
            .await
            .expect("SubscriberStream creation failed");

    // Ensure the multipub subscriber had enough time to init
    thread::sleep(Duration::from_millis(200));

    let write_messages = publisher.write_messages(&pub_buff).expect("Failed to write messages");

    let mut v: Vec<_> = multipub_subscriber.take(1).collect().await;

    if let Some(Ok((messages_read, data))) = v.pop() {
        assert_eq!(write_messages, 1);
        assert_eq!(messages_read, 1);
        assert_eq!(pub_buff, &data[..]);
    } else {
        panic!("Err in data received from stream");
    }
}

#[tokio::test]
async fn dt_get_publisher_metadata_success() {
    let fqin =
        ServiceFqin::from_str("vm-name:com.google.comms_stack_tests.DTE2EMetadataTest/instance")
            .expect("ServiceFqin should not be malformed");

    let ctx = ContextRef::create(fqin);
    let unit_name = "test-publisher3";
    let publisher = create_publisher(ctx, unit_name).await.expect("Publisher creation failed");

    let metadata = publisher.get_publisher_unit_metadata();
    assert_eq!(metadata.get_message_size(), MESSAGE_SIZE);
    assert_eq!(metadata.get_message_count(), MESSAGE_COUNT);
}

#[tokio::test]
async fn dt_validate_publisher_metadata_on_create() {
    let fqin = ServiceFqin::from_str(
        "vm-name:com.google.comms_stack_tests.DtValidatePublisherMetadataOnCreateTest/instance",
    )
    .expect("ServiceFqin should not be malformed");
    let ctx = ContextRef::create(fqin);
    let unit_type = make_unit_type("InvalidMetadataUnitType").expect("Valid UnitType");

    let unit_name = "invalid-message-size-zero-pub";
    assert!(
        create_publisher_with_unittype_and_metadata(
            ctx, &unit_type, unit_name, /* message_size = */ 0,
            /* message_count = */ 128,
        )
        .await
        .is_err(),
        "Registering a publication with message_size = 0 should return an error"
    );

    let unit_name = "invalid-message-count-zero-pub";
    assert!(
        create_publisher_with_unittype_and_metadata(
            ctx, &unit_type, unit_name, /* message_size = */ 128,
            /* message_count = */ 0,
        )
        .await
        .is_err(),
        "Registering a publication with message_size = 0 should return an error"
    );

    let unit_name = "invalid-giant-pub";
    assert!(
        create_publisher_with_unittype_and_metadata(
            ctx,
            &unit_type,
            unit_name,
            /* message_size = */ 1 << 31,
            /* message_count = */ 1 << 31,
        )
        .await
        .is_err(),
        "Registering a publication with a huge buffer size should return an error",
    );
}

#[tokio::test]
async fn dt_subscriber_channel_success() {
    // Channels depends on a SD listener that needs a running binder thread.
    binder::ProcessState::start_thread_pool();

    // Spawn a new task to ensure all futures are Send.
    tokio::spawn(async {
        let pub_buff = [0x11_u8; BUFFER_SIZE];
        let mut sub_buff = [0x00_u8; BUFFER_SIZE];

        let fqin = ServiceFqin::from_str(
            "vm-name:com.google.comms_stack_tests.DTSubChannelTests/instance",
        )
        .expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_type = make_unit_type("SubscriberChannelTestTopic").expect("Valid UnitType");
        let channel_name = "test-channel";

        let publisher =
            create_publisher_with_channel(ctx, &unit_type, "test-publisher-channel", channel_name)
                .await
                .expect("Publisher creation failed");

        let options = SubscriberOptions { fetch_last_message: false };
        let subscriber = Subscriber::create_for_channel(ctx, &unit_type, channel_name, options)
            .await
            .expect("Subscriber creation failed");

        // Ensure the subscriber had enough time to init
        tokio::time::sleep(Duration::from_millis(200)).await;
        let write_messages = publisher.write_messages(&pub_buff).expect("Failed to write messages");

        // Ensure the subscriber had enough time to propagate messages
        tokio::time::sleep(Duration::from_millis(200)).await;

        let available_messages =
            subscriber.available_to_read().expect("Failed to get messages available to read");
        let read_messages =
            subscriber.read_messages(&mut sub_buff).expect("Failed to read messages");

        assert_eq!(write_messages, 1);
        assert_eq!(available_messages, 1);
        assert_eq!(read_messages, 1);
        assert_eq!(pub_buff, sub_buff);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dt_subscriber_multiple_channels_success() {
    // Channels depends on a SD listener that needs a running binder thread.
    binder::ProcessState::start_thread_pool();

    // Spawn a new task to ensure all futures are Send.
    tokio::spawn(async {
        let mut pub_buff = [0x11_u8; BUFFER_SIZE];
        let mut sub_buff = [0x00_u8; BUFFER_SIZE];

        let fqin = ServiceFqin::from_str(
            "vm-name:com.google.comms_stack_tests.DTMultiChannelTests/instance",
        )
        .expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_type = make_unit_type("MultiChannelTestTopic").expect("Valid UnitType");
        let channel_a = "channel-a";
        let channel_b = "channel-b";

        // Create publishers on different channels
        let publisher_a = create_publisher_with_channel(ctx, &unit_type, "pub-a", channel_a)
            .await
            .expect("Publisher A creation failed");
        let publisher_b = create_publisher_with_channel(ctx, &unit_type, "pub-b", channel_b)
            .await
            .expect("Publisher B creation failed");

        // Create subscribers for each channel
        let options_a = SubscriberOptions { fetch_last_message: false };
        let subscriber_a = Subscriber::create_for_channel(ctx, &unit_type, channel_a, options_a)
            .await
            .expect("Subscriber A creation failed");
        let options_b = SubscriberOptions { fetch_last_message: false };
        let subscriber_b = Subscriber::create_for_channel(ctx, &unit_type, channel_b, options_b)
            .await
            .expect("Subscriber B creation failed");

        // Wait for subscribers to be ready
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Write to channel A
        pub_buff.fill(0xAA);
        assert_eq!(publisher_a.write_messages(&pub_buff).expect("Write to A failed"), 1);

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Check subscriber A
        assert_eq!(subscriber_a.available_to_read().unwrap(), 1);
        assert_eq!(subscriber_a.read_messages(&mut sub_buff).unwrap(), 1);
        assert_eq!(sub_buff, [0xAA_u8; BUFFER_SIZE]);

        // Check subscriber B (should have no messages)
        assert_eq!(subscriber_b.available_to_read().unwrap(), 0);

        // Write to channel B
        pub_buff.fill(0xBB);
        assert_eq!(publisher_b.write_messages(&pub_buff).expect("Write to B failed"), 1);

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Check subscriber A (should have no new messages)
        assert_eq!(subscriber_a.available_to_read().unwrap(), 0);

        // Check subscriber B
        assert_eq!(subscriber_b.available_to_read().unwrap(), 1);
        assert_eq!(subscriber_b.read_messages(&mut sub_buff).unwrap(), 1);
        assert_eq!(sub_buff, [0xBB_u8; BUFFER_SIZE]);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dt_subscriber_channel_create_before_publish() {
    // Channels depends on a SD listener that needs a running binder thread.
    binder::ProcessState::start_thread_pool();

    tokio::spawn(async {
        let fqin = ServiceFqin::from_str(
            "vm-name:com.google.comms_stack_tests.DTSubChanCreateBefore/instance",
        )
        .expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_type = make_unit_type("SubChanCreateBeforeTopic").expect("Valid UnitType");
        let channel_name = "before-channel";

        // Create subscriber before publisher
        let options = SubscriberOptions { fetch_last_message: false };
        let subscriber =
            Subscriber::create_for_channel(ctx, &unit_type, channel_name, options).await;

        assert!(subscriber.is_err());
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dt_subscriber_channel_metadata() {
    tokio::spawn(async {
        let fqin = ServiceFqin::from_str(
            "vm-name:com.google.comms_stack_tests.DTSubChanMetadata/instance",
        )
        .expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_type = make_unit_type("SubscriberChannelMetadataTopic").expect("Valid UnitType");
        let channel_name = "metadata-channel";

        let publisher =
            create_publisher_with_channel(ctx, &unit_type, "test-pub-metadata", channel_name)
                .await
                .expect("Publisher creation failed");

        let options = SubscriberOptions { fetch_last_message: false };
        let subscriber = Subscriber::create_for_channel(ctx, &unit_type, channel_name, options)
            .await
            .expect("Subscriber creation failed");

        let pub_meta = publisher.get_publisher_unit_metadata();
        let sub_meta = subscriber.get_publisher_unit_metadata();

        assert_eq!(pub_meta.get_message_size(), sub_meta.get_message_size());
        assert_eq!(pub_meta.get_message_count(), sub_meta.get_message_count());
        assert_eq!(pub_meta.get_channel(), sub_meta.get_channel());
        assert_eq!(sub_meta.get_channel(), channel_name);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dt_subscriber_channel_invalid_name() {
    tokio::spawn(async {
        let fqin =
            ServiceFqin::from_str("vm-name:com.google.comms_stack_tests.DTSubChanInvalid/instance")
                .expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_type = make_unit_type("SubscriberChannelInvalidTopic").expect("Valid UnitType");
        let channel_name = "INVALID\x07CHANNEL-NAME"; // Invalid channel name

        let options = SubscriberOptions { fetch_last_message: false };
        let subscriber_result =
            Subscriber::create_for_channel(ctx, &unit_type, channel_name, options).await;

        assert!(subscriber_result.is_err());
        match subscriber_result {
            Err(error_object) => {
                assert_eq!(error_object.status_code(), SdvStatusCode::Internal);
            }
            Ok(_subscriber) => {
                panic!("Test failure: Expected an error result, but successfully got a Subscriber object.");
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dt_subscriber_channel_incompatible_message_sizes() {
    // Channels depends on a SD listener that needs a running binder thread.
    binder::ProcessState::start_thread_pool();

    tokio::spawn(async {
        let fqin = ServiceFqin::from_str(
            "vm-name:com.google.comms_stack_tests.DTSubChanIncompatibleSizes/instance",
        )
        .expect("ServiceFqin should not be malformed");

        let ctx = ContextRef::create(fqin);

        let unit_type =
            make_unit_type("SubscriberChannelIncompatibleSizesTopic").expect("Valid UnitType");
        let channel_name = "incompatible-sizes-channel";

        // Create two publishers with different message sizes
        let mut unit_registration_args_a = UnitRegistrationArgs::new();
        let mut publisher_metadata_a = PublisherUnitMetadata::new();
        publisher_metadata_a.set_message_size(MESSAGE_SIZE);
        publisher_metadata_a.set_message_count(MESSAGE_COUNT);
        publisher_metadata_a.set_channel(channel_name);
        unit_registration_args_a.set_unit_type(&unit_type);
        unit_registration_args_a.set_unit_name("pub-a");
        unit_registration_args_a.set_unit_metadata(&UnitMetadata::Publisher(publisher_metadata_a));

        let _publisher_a = Publisher::create(ctx, unit_registration_args_a)
            .await
            .expect("Publisher A creation failed");

        let mut unit_registration_args_b = UnitRegistrationArgs::new();
        let mut publisher_metadata_b = PublisherUnitMetadata::new();
        publisher_metadata_b.set_message_size(MESSAGE_SIZE * 2); // Different message size
        publisher_metadata_b.set_message_count(MESSAGE_COUNT);
        publisher_metadata_b.set_channel(channel_name);
        unit_registration_args_b.set_unit_type(&unit_type);
        unit_registration_args_b.set_unit_name("pub-b");
        unit_registration_args_b.set_unit_metadata(&UnitMetadata::Publisher(publisher_metadata_b));

        let _publisher_b = Publisher::create(ctx, unit_registration_args_b)
            .await
            .expect("Publisher B creation failed");

        // Attempt to create a subscriber for the channel
        let options = SubscriberOptions { fetch_last_message: false };
        let subscriber_result =
            Subscriber::create_for_channel(ctx, &unit_type, channel_name, options).await;

        assert!(subscriber_result.is_err());
        match subscriber_result {
            Err(e) => {
                assert_eq!(e.status_code(), SdvStatusCode::InvalidArgument);
            }
            Ok(_) => panic!("Subscriber creation should have failed"),
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dt_e2e_channel_stream_success() {
    // Channels depends on a SD listener that needs a running binder thread.
    binder::ProcessState::start_thread_pool();

    let pub_buff = [0x11_u8; BUFFER_SIZE];

    let fqin = ServiceFqin::from_str(
        "vm-name:com.google.comms_stack_tests.DTE2EChannelStreamTests/instance",
    )
    .expect("ServiceFqin should not be malformed");
    let ctx = ContextRef::create(fqin);

    let unit_type = make_unit_type("ChannelStreamTestTopic").expect("Valid UnitType");
    let channel_name = "test-channel-stream";

    let publisher = create_publisher_with_channel(
        ctx,
        &unit_type,
        "test-publisher-channel-stream",
        channel_name,
    )
    .await
    .expect("Publisher creation failed");

    let options = SubscriberOptions { fetch_last_message: false };
    let channel_subscriber = SubscriberStream::create_for_channel(
        ctx,
        &unit_type,
        channel_name,
        Duration::ZERO,
        options,
    )
    .await
    .expect("SubscriberStream creation failed");

    // Ensure the channel subscriber had enough time to init
    tokio::time::sleep(Duration::from_millis(200)).await;

    let write_messages = publisher.write_messages(&pub_buff).expect("Failed to write messages");

    let mut v: Vec<_> = channel_subscriber.take(1).collect().await;

    if let Some(Ok((messages_read, data))) = v.pop() {
        assert_eq!(write_messages, 1);
        assert_eq!(messages_read, 1);
        assert_eq!(pub_buff, &data[..]);
    } else {
        panic!("Err in data received from stream");
    }
}

#[tokio::test]
async fn dt_subscriber_channel_stream_create_before_publish() {
    // Channels depends on a SD listener that needs a running binder thread.
    binder::ProcessState::start_thread_pool();

    tokio::spawn(async {
        let fqin = ServiceFqin::from_str(
            "vm-name:com.google.comms_stack_tests.DTSubChanStreamCreateBefore/instance",
        )
        .expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_type = make_unit_type("SubChanStreamCreateBeforeTopic").expect("Valid UnitType");
        let channel_name = "before-channel-stream";

        // Create subscriber before publisher
        let options = SubscriberOptions { fetch_last_message: false };
        let subscriber = SubscriberStream::create_for_channel(
            ctx,
            &unit_type,
            channel_name,
            Duration::ZERO,
            options,
        )
        .await;

        assert!(subscriber.is_err());
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn dt_subscriber_channel_stream_invalid_name() {
    tokio::spawn(async {
        let fqin =
            ServiceFqin::from_str("vm-name:com.google.comms_stack_tests.DTSubChanStreamInvalid/instance")
                .expect("ServiceFqin should not be malformed");
        let ctx = ContextRef::create(fqin);

        let unit_type =
            make_unit_type("SubscriberChannelStreamInvalidTopic").expect("Valid UnitType");
        let channel_name = "INVALID\x07CHANNEL-NAME"; // Invalid channel name

        let options = SubscriberOptions { fetch_last_message: false };
        let subscriber_result = SubscriberStream::create_for_channel(
            ctx,
            &unit_type,
            channel_name,
            Duration::ZERO,
            options,
        )
        .await;

        assert!(subscriber_result.is_err());
        match subscriber_result {
            Err(error_object) => {
                assert_eq!(error_object.status_code(), SdvStatusCode::Internal);
            }
            Ok(_subscriber) => {
                panic!("Test failure: Expected an error result, but successfully got a Subscriber object.");
            }
        }
    })
    .await
    .unwrap();
}
