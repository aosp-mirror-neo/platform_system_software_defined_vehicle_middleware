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

//! User-facing Middleware Pub/Sub APIs

use crate::codec;
use crate::{ByteBuffersStream, InstantRead, Publish, ReadHistory, Subscribe};
use futures::stream::Stream;
use futures::StreamExt;
use log::warn;
use protobuf::Message;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::cmp::min;
use std::marker::PhantomData;
use std::pin::Pin;

/// Publisher represents the user-facing, typed producer for a message queue.
pub struct Publisher<M: Message> {
    publish_handle: Box<dyn Publish>,
    write_buffer: Vec<u8>,
    max_msg_size: Option<usize>,
    queue_capacity: Option<usize>,
    ptd: PhantomData<M>,
}

impl<M: Message> Publisher<M> {
    /// Create an instance of a typed Publisher
    pub fn new(publish_handle: Box<dyn Publish>) -> Self {
        Self {
            publish_handle,
            write_buffer: Vec::new(),
            max_msg_size: None,
            queue_capacity: None,
            ptd: PhantomData,
        }
    }

    /// Serializes and publishes a proto message.
    #[tracing::instrument(name = "MW::Publisher<M>::publish", skip(self, message))]
    pub fn publish(&self, message: &M) -> SdvResult<()> {
        self.publish_batch([message])
    }

    /// Serializes and publishes a single proto message using an internal, resizable buffer.
    ///
    /// This is a more efficient alternative to the standard `publish` method. While `publish`
    /// is immutable and performs a new allocation per call, this method reuses a pre-allocated
    /// internal buffer to minimize heap overhead during frequent publishing.
    ///
    /// Unlike `publish`, this method requires a **mutable reference** (`&mut self`) because
    /// it must modify the state of the internal serialization buffer. Use this version when
    /// your workflow allows for exclusive access to the `Publisher` to gain better performance.
    ///
    /// This is a convenience wrapper around `publish_batch_buffered`.
    #[tracing::instrument(name = "MW::Publisher<M>::publish_buffered", skip(self, message))]
    pub fn publish_buffered(&mut self, message: &M) -> SdvResult<()> {
        self.publish_batch_buffered(std::slice::from_ref(message))
    }

    /// Serializes and publishes a batch of proto messages.
    #[tracing::instrument(name = "MW::Publisher<M>::publish_batch", skip(self, messages))]
    pub fn publish_batch<'a>(&self, messages: impl IntoIterator<Item = &'a M>) -> SdvResult<()> {
        // Prepend the message with the message length, because the message byte array
        // returned on subscription may be larger than the actual message.
        let byte_buffers: SdvResult<Vec<Box<[u8]>>> =
            messages.into_iter().map(|msg| codec::encode(msg)).collect();
        match byte_buffers {
            Err(e) => Err(e),
            Ok(_byte_buffers) => self.publish_handle.publish_messages(&_byte_buffers),
        }
    }

    /// Serializes and publishes a batch of proto messages using an internal, resizable buffer.
    ///
    /// This is a more efficient alternative to `publish_batch`. While `publish_batch` (and `publish`)
    /// are immutable and perform allocations per call, this method reuses an internal buffer
    /// to minimize heap allocations during frequent publishing.
    ///
    /// Unlike the legacy `publish_batch`, this method requires a **mutable reference** (`&mut self`)
    /// to manage and grow the underlying serialization buffer. Use this when your workflow allows
    /// for exclusive access to the `Publisher` and you prioritize throughput.
    ///
    /// Each message is serialized and padded with zeros to `max_msg_size`.
    #[tracing::instrument(name = "MW::Publisher<M>::publish_batch_buffered", skip(self, messages))]
    pub fn publish_batch_buffered(&mut self, messages: &[M]) -> SdvResult<()> {
        let max_msg_size = self.get_max_message_size()?;
        let queue_capacity = self.get_queue_capacity()?;

        if messages.len() > queue_capacity {
            return Err(SdvStatus::with_message(
                SdvStatusCode::Internal,
                format!(
                    "Publishing {} messages, but queue capacity is only {}.",
                    messages.len(),
                    queue_capacity
                ),
            ));
        }

        let message_count = messages.len();
        if message_count == 0 {
            return Ok(());
        }

        // Vec::clear() is expected O(1) for vector of primitives
        // Capacity is retained.
        self.write_buffer.clear();

        // We know ahead of time the buffer size.
        // Reserve (i.e. potentially reallocate) more memory once.
        // Does nothing if the capacity is already sufficient.
        self.write_buffer.reserve(max_msg_size * message_count);

        // Serialize, pad, and publish.
        Self::encode_fixed_size_messages_into_vec(
            messages.iter(),
            &mut self.write_buffer,
            max_msg_size,
        )?;
        self.publish_handle.publish_flat_batch(&self.write_buffer)
    }

    fn get_max_message_size(&mut self) -> SdvResult<usize> {
        if let Some(size) = self.max_msg_size {
            return Ok(size);
        }
        let size = self.publish_handle.get_max_message_size()?;
        self.max_msg_size = Some(size);
        Ok(size)
    }

    fn get_queue_capacity(&mut self) -> SdvResult<usize> {
        if let Some(capacity) = self.queue_capacity {
            return Ok(capacity);
        }
        let capacity = self.publish_handle.get_queue_capacity()?;
        self.queue_capacity = Some(capacity);
        Ok(capacity)
    }

    /// Encodes an iterator of messages into a buffer, where each message is padded to a fixed size.
    ///
    /// This helper function serializes each message and then resizes the buffer to ensure each
    /// message occupies a `fixed_msg_size` block of memory, padding with zeros if necessary.
    /// This is required by underlying communication layers that expect fixed-size message chunks.
    ///
    /// # Errors
    ///
    /// Returns an `InvalidArgument` error if any single message, after serialization, is larger
    /// than `fixed_msg_size`.
    fn encode_fixed_size_messages_into_vec<'a>(
        messages: impl IntoIterator<Item = &'a M>,
        buffer: &mut Vec<u8>,
        fixed_msg_size: usize,
    ) -> SdvResult<()> {
        for message in messages {
            let current_len = buffer.len();
            codec::encode_to_vec(message, buffer)?;
            let encoded_len = buffer.len() - current_len;
            if encoded_len > fixed_msg_size {
                return Err(SdvStatus::with_message(
                    SdvStatusCode::InvalidArgument,
                    format!(
                        "Message is too long: {} bytes (max: {} bytes)",
                        encoded_len, fixed_msg_size
                    ),
                ));
            }
            buffer.resize(current_len + fixed_msg_size, 0);
        }
        Ok(())
    }
}

/// Subscriber represents the user-facing, polling, typed consumer for a named queue.
pub struct Subscriber<M: Message> {
    handle: Box<dyn Subscribe>,
    read_buffer: Vec<u8>,
    max_msg_size: Option<usize>,
    ptd: PhantomData<M>,
}

impl<M: Message> Subscriber<M> {
    /// Create an instance of a typed Subscriber.
    pub fn new(handle: Box<dyn Subscribe>) -> Self {
        Self { handle, read_buffer: Vec::new(), max_msg_size: None, ptd: PhantomData }
    }

    /// Returns an SdvResult containing a vector of messages from the publication queue.
    /// The returned message count is min(message_count, available_to_read).
    #[tracing::instrument(name = "MW::Subscriber<M>::read_next_messages", skip(self))]
    pub fn read_next_messages(&self, message_count: u32) -> SdvResult<Vec<M>> {
        self.handle
            .read_next_messages(message_count)?
            .iter()
            .map(|bytes| codec::decode(bytes))
            .collect()
    }

    /// Returns an SdvResult containing a vector of next messages from the publication queue.
    /// The returned message count is min(message_count, available_to_read).
    ///
    /// This method reuses an internal buffer to minimize allocations, offering better performance
    /// for frequent reads. It requires a mutable reference to the `Subscriber`.
    #[tracing::instrument(name = "MW::Subscriber<M>::read_next_messages_buffered", skip(self))]
    pub fn read_next_messages_buffered(&mut self, message_count: u32) -> SdvResult<Vec<M>> {
        let max_msg_size = self.get_max_message_size()?;

        let buffer_size = self.get_buffer_size(message_count)?;
        if buffer_size == 0 {
            return Ok(Vec::new());
        }

        // Resize instead of reservere.
        // The buffer needs a min length (not capacity).
        // Optimized for allocations, i.e. to only ever grow, not truncate and re-allocate upon repeat calls.
        if self.read_buffer.len() < buffer_size {
            self.read_buffer.resize(buffer_size, 0);
        }

        // Pass the exact buffer size, required by the underlying implementation.
        let count = self.handle.read_flat_batch(&mut self.read_buffer[..buffer_size])?;
        self.read_buffer
            .chunks_exact(max_msg_size)
            .take(count)
            .map(|chunk| codec::decode(chunk))
            .collect()
    }

    #[tracing::instrument(name = "get_buffer_size", skip(self))]
    fn get_buffer_size(&mut self, requested_count: u32) -> SdvResult<usize> {
        let message_count =
            min(self.handle.get_available_to_read_count()?, requested_count as usize);

        // Emit an info event with the updated value
        tracing::info!(message_count, "Actually read message count");
        self.get_max_message_size()?.checked_mul(message_count).ok_or_else(|| {
            SdvStatus::with_message(SdvStatusCode::Internal, "Buffer size calculation overflowed.")
        })
    }

    fn get_max_message_size(&mut self) -> SdvResult<usize> {
        if let Some(size) = self.max_msg_size {
            return Ok(size);
        }

        let size = self.handle.get_max_message_size()?;
        self.max_msg_size = Some(size);
        Ok(size)
    }
}

/// Creates an asynchronous message stream, which is then used user-side.
///
/// Each stream item can contain multiple individual messages, in the form of Vec<M: Message>.
/// If a min_interval bigger than Duration::ZERO is specified, the underlying stream won't call 'poll_next'
/// more than once per specified interval.
pub fn observe<M: Message>(
    byte_buffers_stream: ByteBuffersStream,
) -> Pin<Box<dyn Stream<Item = SdvResult<Vec<M>>> + Send>> {
    Box::pin(byte_buffers_stream.map(|item| match item {
        Err(e) => Err(e),
        Ok((count, byte_array)) => {
            if count == 0 {
                Ok(Vec::new())
            } else {
                let _map_message_span =
                    tracing::info_span!("MW::observe<M>: Map Message Stream").entered();
                let size = byte_array.len() / (count as usize);
                let messages: SdvResult<Vec<M>> =
                    byte_array.chunks(size).map(|bytes| codec::decode(bytes)).collect();
                messages
            }
        }
    }))
}

/// History represents the user-facing, typed, subscriber-side message cache.
pub struct History<M: Message> {
    handle: Box<dyn ReadHistory>,
    ptd: PhantomData<M>,
}

impl<M: Message> History<M> {
    /// Create an instance of a typed History.
    pub fn new(handle: Box<dyn ReadHistory>) -> Self {
        Self { handle, ptd: PhantomData }
    }
    /// Returns an SdvResult containing a vector of messages from the subscriber cache.
    /// The returned message count is min(message_count, available_to_read).
    #[tracing::instrument(name = "MW::History<M>::read_from_history", skip(self))]
    pub fn read_from_history(&self, message_count: u32) -> SdvResult<Vec<M>> {
        self.handle
            .read_from_history(message_count)?
            .iter()
            .map(|bytes| codec::decode(bytes))
            .collect()
    }
}

/// InstantReader is a user-facing handle to a publication, which enables reading directly from the publication.
pub struct InstantReader<M: Message> {
    handle: Box<dyn InstantRead>,
    ptd: PhantomData<M>,
}

impl<M: Message> InstantReader<M> {
    /// Create an instance of a typed InstantReader.
    pub fn new(handle: Box<dyn InstantRead>) -> Self {
        Self { handle, ptd: PhantomData }
    }
    /// Returns an SdvResult containing the latest published message for a publication, if there is one.
    #[tracing::instrument(name = "MW::InstantReader<M>::read_latest_message", skip(self))]
    pub async fn read_latest_message(&self) -> SdvResult<Option<M>> {
        let bytes = self.handle.read_latest_message().await?;
        match bytes {
            None => Ok(None),
            Some(bytes) => Ok(Some(codec::decode(&bytes)?)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::pubsub_traits::{MockInstantRead, MockPublish, MockReadHistory, MockSubscribe};

    use super::*;
    use futures::stream;
    use protobuf::well_known_types::wrappers::StringValue;
    use sdv_status_rs::{SdvStatus, SdvStatusCode};

    fn create_string_value_message(s: &str) -> StringValue {
        let mut msg = StringValue::new();
        msg.value = s.to_string();
        msg
    }

    fn encode_to_byte_array<M: Message>(message: &M) -> Vec<u8> {
        message.write_length_delimited_to_bytes().unwrap()
    }

    #[test]
    fn test_publisher_new() {
        // GIVEN a mock publish handle
        let mut mock_publish = MockPublish::new();
        mock_publish.expect_get_max_message_size().returning(|| Ok(1024));
        mock_publish.expect_get_queue_capacity().returning(|| Ok(10));
        mock_publish.expect_publish_messages().returning(|_| Ok(()));

        // WHEN create new publisher
        let publisher: Publisher<StringValue> = Publisher::new(Box::new(mock_publish));
        let test_message = create_string_value_message("test_message");
        // AND publish a message
        let result = publisher.publish(&test_message);

        // THEN result is ok
        assert!(result.is_ok());
    }

    #[test]
    fn test_publisher_publish_buffered() {
        // GIVEN a mock publish handle
        let mut mock_publish = MockPublish::new();
        mock_publish.expect_get_max_message_size().returning(|| Ok(1024));
        mock_publish.expect_get_queue_capacity().returning(|| Ok(10));
        mock_publish.expect_publish_flat_batch().returning(|_| Ok(()));

        // WHEN create new publisher
        let mut publisher: Publisher<StringValue> = Publisher::new(Box::new(mock_publish));
        let test_message = create_string_value_message("test_message_buffered");

        // AND publish a message buffered
        let result = publisher.publish_buffered(&test_message);

        // THEN result is ok
        assert!(result.is_ok());
    }

    #[test]
    fn test_subscriber_new() {
        let test_msg1_str = "hello subscriber";
        let test_msg2_str = "another message";
        let msg1 = create_string_value_message(test_msg1_str);
        let msg2 = create_string_value_message(test_msg2_str);

        let encoded_msg1 = codec::encode(&msg1).unwrap();
        let encoded_msg2 = codec::encode(&msg2).unwrap();

        // GIVEN a mock subscribe handle that expects to be called once and return two messages
        let mut mock_subscribe = MockSubscribe::new();
        mock_subscribe.expect_get_max_message_size().returning(|| Ok(1024));
        mock_subscribe
            .expect_read_next_messages()
            .returning(move |_| Ok(vec![encoded_msg1.clone(), encoded_msg2.clone()]));
        let subscriber: Subscriber<StringValue> = Subscriber::new(Box::new(mock_subscribe));

        // WHEN read message from Subscriber
        let decoded_messages = subscriber.read_next_messages(2).unwrap();

        // THEN there are 2 messages with correct value
        assert_eq!(decoded_messages.len(), 2);
        assert_eq!(decoded_messages[0].value, test_msg1_str);
        assert_eq!(decoded_messages[1].value, test_msg2_str);
    }

    #[test]
    fn test_subscriber_read_next_messages_buffered() {
        let test_msg1_str = "hello subscriber buffered";
        let msg1 = create_string_value_message(test_msg1_str);
        let encoded_msg1 = codec::encode(&msg1).unwrap();

        // GIVEN a mock subscribe handle
        let mut mock_subscribe = MockSubscribe::new();
        mock_subscribe.expect_get_max_message_size().returning(|| Ok(1024));
        mock_subscribe.expect_get_available_to_read_count().returning(|| Ok(1));
        mock_subscribe.expect_read_flat_batch().returning(move |buffer| {
            buffer[..encoded_msg1.len()].copy_from_slice(&encoded_msg1);
            Ok(1)
        });

        let mut subscriber: Subscriber<StringValue> = Subscriber::new(Box::new(mock_subscribe));

        // WHEN read message from Subscriber buffered
        let decoded_messages = subscriber.read_next_messages_buffered(1).unwrap();

        // THEN there is 1 message with correct value
        assert_eq!(decoded_messages.len(), 1);
        assert_eq!(decoded_messages[0].value, test_msg1_str);
    }

    #[test]
    fn test_history_new_and_read() {
        let test_msg1_str = "cached message 1";
        let test_msg2_str = "cached message 2";
        let msg1 = create_string_value_message(test_msg1_str);
        let msg2 = create_string_value_message(test_msg2_str);

        let encoded_msg1 = codec::encode(&msg1).unwrap();
        let encoded_msg2 = codec::encode(&msg2).unwrap();

        // GIVEN a mock read_history handle that expects to be called once and return two messages
        let mut mock_read_history = MockReadHistory::new();
        mock_read_history
            .expect_read_from_history()
            .returning(move |_| Ok(vec![encoded_msg1.clone(), encoded_msg2.clone()]));
        let history: History<StringValue> = History::new(Box::new(mock_read_history));

        // WHEN read message from ReadHistory
        let decoded_messages = history.read_from_history(2).expect("Read from history failed");

        // THEN there are 2 messages with correct value
        assert_eq!(decoded_messages.len(), 2);
        assert_eq!(decoded_messages[0].value, test_msg1_str);
        assert_eq!(decoded_messages[1].value, test_msg2_str);
    }

    #[tokio::test]
    async fn test_observe_empty_stream() {
        // GIVEN empty byte buffer stream
        let stream: ByteBuffersStream = Box::pin(stream::empty());

        // WHEN create new observe
        let mut observed_stream = observe::<StringValue>(stream);

        // THEN next is none
        assert!(observed_stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_observe_successful_decode() {
        let msg1 = create_string_value_message("hello");
        let msg2 = create_string_value_message("world");
        let bytes1 = encode_to_byte_array(&msg1);
        let bytes2 = encode_to_byte_array(&msg2);

        let mut combined_bytes = Vec::new();
        combined_bytes.extend_from_slice(&bytes1);
        combined_bytes.extend_from_slice(&bytes2);

        // GIVEN new byte buffer stream with 2 messages
        let stream_item: SdvResult<(u32, Vec<u8>)> = Ok((2, combined_bytes));
        let byte_buffers_stream: ByteBuffersStream = Box::pin(stream::once(async { stream_item }));

        // WHEN create new observe and observe the stream
        let mut observed_stream = observe::<StringValue>(byte_buffers_stream);
        let result = observed_stream.next().await;
        let messages = result.unwrap().unwrap();

        // THEN there are exactly 2 messages with correct value
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].value, "hello");
        assert_eq!(messages[1].value, "world");
        assert!(observed_stream.next().await.is_none());
    }

    #[tokio::test]
    async fn test_observe_count_zero() {
        // GIVEN new byte buffer stream with 0 message
        let stream_item: SdvResult<(u32, Vec<u8>)> = Ok((0, vec![1, 2, 3])); // byte_array will be ignored
        let byte_buffers_stream: ByteBuffersStream = Box::pin(stream::once(async { stream_item }));

        // WHEN create new observe and observe the stream
        let mut observed_stream = observe::<StringValue>(byte_buffers_stream);
        let result = observed_stream.next().await;

        // THEN result is empty
        let messages = result.unwrap();
        assert!(messages.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_observe_decode_error() {
        // GIVEN new byte buffer stream with invalid message
        let invalid_bytes = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let stream_item: SdvResult<(u32, Vec<u8>)> = Ok((1, invalid_bytes));
        let byte_buffers_stream: ByteBuffersStream = Box::pin(stream::once(async { stream_item }));

        // WHEN create new observe and observe the stream
        let mut observed_stream = observe::<StringValue>(byte_buffers_stream);
        let result = observed_stream.next().await;

        // THEN result is error
        let status = result.unwrap().unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::Internal);
    }

    #[tokio::test]
    async fn test_observe_stream_error() {
        // GIVEN new byte buffer stream with error
        let stream_item: SdvResult<(u32, Vec<u8>)> =
            Err(SdvStatus::new(SdvStatusCode::Unavailable));
        let byte_buffers_stream: ByteBuffersStream = Box::pin(stream::once(async { stream_item }));

        // WHEN create new observe and observe the stream
        let mut observed_stream = observe::<StringValue>(byte_buffers_stream);
        let result = observed_stream.next().await;

        // THEN result is error
        let status = result.unwrap().unwrap_err();
        assert_eq!(status.status_code(), SdvStatusCode::Unavailable);
    }

    #[tokio::test]
    async fn test_instant_reader_read_latest_message_some() {
        let test_msg_str = "latest message";
        let msg = create_string_value_message(test_msg_str);
        let encoded_msg = codec::encode(&msg).unwrap();

        // GIVEN a mock instant_read handle
        let mut mock_instant_read = MockInstantRead::new();
        mock_instant_read
            .expect_read_latest_message()
            .returning(move || Ok(Some(encoded_msg.clone())));
        let reader: InstantReader<StringValue> = InstantReader::new(Box::new(mock_instant_read));

        // WHEN read latest message
        let result = reader.read_latest_message().await;

        // THEN result is ok
        assert!(result.is_ok());
        let option_msg = result.unwrap();
        assert!(option_msg.is_some());
        assert_eq!(option_msg.unwrap().value, test_msg_str);
    }

    #[tokio::test]
    async fn test_instant_reader_read_latest_message_none() {
        // GIVEN a mock instant_read handle
        let mut mock_instant_read = MockInstantRead::new();
        mock_instant_read.expect_read_latest_message().returning(|| Ok(None));
        let reader: InstantReader<StringValue> = InstantReader::new(Box::new(mock_instant_read));

        // WHEN read latest message
        let result = reader.read_latest_message().await;

        // THEN result is ok
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        // GIVEN last result is none
        // WHEN read latest message again
        let result_after_take = reader.read_latest_message().await;

        // THEN result is still ok
        assert!(result_after_take.is_ok());
        assert!(result_after_take.unwrap().is_none());
    }
}
