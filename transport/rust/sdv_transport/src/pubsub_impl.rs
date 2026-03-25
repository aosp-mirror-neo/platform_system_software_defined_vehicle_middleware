/*
 * Copyright (C) 2026 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use async_trait::async_trait;
use sdv_comms::dt::HistoryReader as DtHistory;
use sdv_comms::dt::InstantReader as DtInstantReader;
use sdv_comms::dt::Publisher as DtPublisher;
use sdv_comms::dt::Subscriber as DtSubscriber;
use sdv_comms::dt::SubscriberOptions as DtSubscriberOptions;
use sdv_comms::dt::SubscriberStream as DtStream;
use sdv_comms::id::UnitIdentifier;
use sdv_comms::sd::ServiceUnitDefinition;
use sdv_comms::ContextRef;
use sdv_mw_rs_comms::{
    ByteBuffersStream, InstantRead, Publication, Publish, ReadHistory, Subscribe, SubscribeOptions,
};
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::cmp::min;
use std::io::Write;

fn allocate_buffer(buffer_size: usize) -> SdvResult<Vec<u8>> {
    let mut buffer = Vec::<u8>::new();
    buffer.try_reserve(buffer_size).map_err(|_| {
        SdvStatus::with_message(SdvStatusCode::ResourceExhausted, "Failed to allocate buffer")
    })?;
    buffer.resize(buffer_size, 0u8);
    Ok(buffer)
}

static_assertions::assert_impl_all!(SdvPublisher: Send);
/// SdvComms-specific implementation of a producer-side endpoint to a publication (message queue).
///
/// Intended to be used within the MW library code. Is not exposed to the user.
pub struct SdvPublisher {
    publish_handle: DtPublisher,
}

impl SdvPublisher {
    pub fn new(dt_publisher: DtPublisher) -> Self {
        Self { publish_handle: dt_publisher }
    }
}

impl Publish for SdvPublisher {
    #[tracing::instrument(name = "MW::SdvPublisher::publish_messages",
        skip(self, messages),
        fields(number_of_msgs = messages.len(), msg_sizes = ?messages.iter().map(|msg| msg.len()).collect::<Vec<_>>(),
        dt_msg_capacity = self.publish_handle.get_publisher_unit_metadata().get_message_size(),))]
    fn publish_messages(&self, messages: &[Box<[u8]>]) -> SdvResult<()> {
        // Dt uses the (maximum) message size specified in publication registration for all messages
        let size_for_one_msg =
            self.publish_handle.get_publisher_unit_metadata().get_message_size() as usize;
        let span = tracing::info_span!("Create Buffer").entered();
        let buffer_size = size_for_one_msg * messages.len();
        let mut buffer = allocate_buffer(buffer_size)?;
        span.exit();
        let span = tracing::info_span!("Create Cursor").entered();
        let mut cursor = std::io::Cursor::new(&mut *buffer);
        span.exit();
        let span = tracing::info_span!("Pad Msgs").entered();
        for (i, message) in messages.iter().enumerate() {
            if message.len() > size_for_one_msg {
                return Err(SdvStatus::with_message(
                    SdvStatusCode::InvalidArgument,
                    format!(
                        "Message at index {} (size {}) exceeds maximum allowed size {}",
                        i,
                        message.len(),
                        size_for_one_msg
                    ),
                ));
            }

            let pos = (i * size_for_one_msg) as u64;
            cursor.set_position(pos);
            cursor.write_all(message).map_err(|_| {
                SdvStatus::with_message(
                    SdvStatusCode::Internal,
                    "Failed to write the message into cursor",
                )
            })?;
        }
        span.exit();
        let span = tracing::info_span!("DT::Publisher::write_messages").entered();
        self.publish_handle.write_messages(&buffer)?;
        span.exit();
        Ok(())
    }
}

static_assertions::assert_impl_all!(SdvPublication: Send);
static_assertions::assert_impl_all!(SdvPublication: Sync);
/// SdvComms-specific representation of a message queue, which offers different methods for consuming messages.
///
/// Intended to be used within the MW library code. Is not exposed to the user.
#[deprecated(since = "2.0.0", note = "Please use SdvComms instead")]
pub struct SdvPublication {
    context: ContextRef,
    service_unit: ServiceUnitDefinition,
}

impl SdvPublication {
    pub fn new(context: ContextRef, service_unit: ServiceUnitDefinition) -> Self {
        Self { context, service_unit }
    }
}

#[async_trait]
impl Publication for SdvPublication {
    async fn subscribe(&self, options: &SubscribeOptions) -> SdvResult<Box<dyn Subscribe>> {
        let dt_subscriber = DtSubscriber::create(
            self.context,
            &self.service_unit.get_unit_identifier(),
            DtSubscriberOptions { fetch_last_message: options.read_on_subscribe() },
        )
        .await?;
        Ok(Box::new(SdvSubscriber::new(dt_subscriber)))
    }

    async fn observe(&self, options: &SubscribeOptions) -> SdvResult<ByteBuffersStream> {
        let dt_stream = DtStream::create(
            self.context,
            &self.service_unit.get_unit_identifier(),
            options.get_min_interval(),
            DtSubscriberOptions { fetch_last_message: options.read_on_subscribe() },
        )
        .await?;
        Ok(Box::pin(dt_stream))
    }

    async fn create_history(&self, options: &SubscribeOptions) -> SdvResult<Box<dyn ReadHistory>> {
        let dt_history = DtHistory::create(
            self.context,
            &self.service_unit.get_unit_identifier(),
            options.get_message_count(),
        )
        .await?;
        Ok(Box::new(SdvHistory::new(dt_history)))
    }

    fn create_instant_read(&self) -> Box<dyn InstantRead> {
        Box::new(SdvInstantReader::new(self.context, self.service_unit.get_unit_identifier()))
    }
}

static_assertions::assert_impl_all!(SdvSubscriber: Send);
/// SdvComms-specific implementation of a consumer-side endpoint to a publication, which enables polling message.
///
/// Intended to be used within the MW library code. Is not exposed to the user.
pub struct SdvSubscriber {
    dt_handle: DtSubscriber,
}

impl SdvSubscriber {
    pub fn new(dt_subscriber: DtSubscriber) -> Self {
        Self { dt_handle: dt_subscriber }
    }
}

impl Subscribe for SdvSubscriber {
    #[tracing::instrument(name = "MW::SdvSubscriber::read_next_messages", skip(self))]
    fn read_next_messages(&self, message_count: u32) -> SdvResult<Vec<Box<[u8]>>> {
        // Ensure we do not attempt to read more messages than the DT agent has otherwise
        // no messages will be read at all
        let mut message_count = min(self.dt_handle.available_to_read()?, message_count);
        if message_count == 0 {
            return Ok(Vec::new());
        }

        let metadata = self.dt_handle.get_publisher_unit_metadata();

        // If message_count is greater than the publisher capacity, then available_to_read() must
        // have returned a value greater than publisher capacity, meaning that this subscriber is
        // in an overflow state and the read_messages() call below will fail with a DataLoss error.
        // Given that, revise the message count to 1 to minimize the size the allocation that won't
        // actually be used, and avoid a possible out-of-memory error.
        if message_count > metadata.get_message_count() {
            message_count = 1;
        }

        // Dt uses the (maximum) message size specified in publication registration for all messages
        let size_for_one_msg = metadata.get_message_size() as usize;
        let buffer_size = size_for_one_msg
            .checked_mul(message_count as usize)
            .unwrap_or(usize::MAX - usize::MAX % size_for_one_msg);
        let mut bytes = allocate_buffer(buffer_size)?;
        self.dt_handle.read_messages(&mut bytes)?;
        Ok(bytes
            .chunks(size_for_one_msg)
            .map(|chunk| chunk.to_owned().into_boxed_slice())
            .collect())
    }
}

static_assertions::assert_impl_all!(SdvHistory: Send);
/// SdvComms-specific implementation of a consumer-side message cache for a publication.
///
/// Intended to be used within the MW library code. Is not exposed to the user.
pub struct SdvHistory {
    dt_handle: DtHistory,
}

impl SdvHistory {
    pub fn new(dt_history: DtHistory) -> Self {
        Self { dt_handle: dt_history }
    }
}

impl ReadHistory for SdvHistory {
    #[tracing::instrument(name = "MW::SdvHistory::read_from_history", skip(self))]
    fn read_from_history(&self, message_count: u32) -> SdvResult<Vec<Box<[u8]>>> {
        // Ensure we do not attempt to read more messages than the DT agent has
        let message_count = min(self.dt_handle.available_to_read()?, message_count);
        if message_count == 0 {
            return Ok(Vec::new());
        }

        // Dt uses the (maximum) message size specified in publication registration for all messages
        let size_for_one_msg =
            self.dt_handle.get_publisher_unit_metadata().get_message_size() as usize;
        let mut bytes = allocate_buffer(size_for_one_msg * (message_count as usize))?;
        self.dt_handle.read_last_messages(&mut bytes)?;
        Ok(bytes
            .chunks(size_for_one_msg)
            .map(|chunk| chunk.to_owned().into_boxed_slice())
            .collect())
    }
}

static_assertions::assert_impl_all!(SdvInstantReader: Send);
static_assertions::assert_impl_all!(SdvInstantReader: Sync);
/// SdvComms-specific implementation of a handle to a publication, which enables reading directly from the publication.
///
/// Intended to be used within the MW library code. Is not exposed to the user.
pub struct SdvInstantReader {
    context: ContextRef,
    unit_id: UnitIdentifier,
}

impl SdvInstantReader {
    pub fn new(context: ContextRef, unit_id: UnitIdentifier) -> Self {
        Self { context, unit_id }
    }
}

#[async_trait]
impl InstantRead for SdvInstantReader {
    #[tracing::instrument(name = "MW::InstantRead::read_latest_message", skip(self))]
    async fn read_latest_message(&self) -> SdvResult<Option<Box<[u8]>>> {
        let bytes = DtInstantReader::get_last_message(self.context, &self.unit_id).await?;
        if bytes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(bytes.into()))
        }
    }
}

static_assertions::assert_impl_all!(SdvTopicInstantReader: Send);
static_assertions::assert_impl_all!(SdvTopicInstantReader: Sync);

/// SdvComms-specific implementation of a handle to a topic, which enables reading directly from the topic.
///
/// Intended to be used within the MW library code. Is not exposed to the user.
pub struct SdvTopicInstantReader {
    dt_handle: DtInstantReader,
}

impl SdvTopicInstantReader {
    pub fn new(dt_handle: DtInstantReader) -> Self {
        Self { dt_handle }
    }
}

#[async_trait]
impl InstantRead for SdvTopicInstantReader {
    #[tracing::instrument(name = "MW::TopicInstantRead::read_latest_message", skip(self))]
    async fn read_latest_message(&self) -> SdvResult<Option<Box<[u8]>>> {
        let bytes = self.dt_handle.read_last_message().await?;

        if bytes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(bytes.into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocate_buffer_success() {
        // GIVEN a buffer size
        let buffer_size = 1024;

        // WHEN calling the allocate buffer function
        let result = allocate_buffer(buffer_size);

        // THEN result is ok
        assert!(result.is_ok());
        let buffer = result.unwrap();
        assert_eq!(buffer.len(), buffer_size);
    }

    #[test]
    fn allocate_buffer_zero_size() {
        // GIVEN a 0 buffer size, WHEN calling the allocate buffer function
        let result = allocate_buffer(0);

        // THEN result is ok
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn allocate_buffer_resource_exhausted() {
        // GIVEN a big buffer size, WHEN calling the allocate buffer function
        let result = allocate_buffer(usize::MAX);

        // THEN result is err
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status_code(), SdvStatusCode::ResourceExhausted);
        assert_eq!(
            err.to_string(),
            "SdvStatusCode::ResourceExhausted(8): Failed to allocate buffer"
        );
    }
}
