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

//! Middleware Pub/Sub Traits

use crate::SubscribeOptions;
use async_trait::async_trait;
use futures::stream::Stream;
use mockall::automock;
use sdv_status_rs::SdvResult;
use std::pin::Pin;

/// A pinned Stream of Item = SdvResult(message_count, multi_message_byte_buffer).
pub type ByteBuffersStream = Pin<Box<dyn Stream<Item = SdvResult<(u32, Vec<u8>)>> + Send>>;

/// A Publish trait object is a producer-side endpoint to a publication (message queue).
///
/// Intented to be implemented as part of the MW library code. Is not exposed to the user.
#[automock]
pub trait Publish: Send {
    /// Publishes an array of messages
    ///
    /// Publish messages takes an array of message byte buffers as input instead of a single byte buffer,
    /// because the trait implementations may or may not use fixed sizes for the messages,
    /// and may or may not encode the message size in the message.
    fn publish_messages(&self, messages: &[Box<[u8]>]) -> SdvResult<()>;
}

/// A Publication trait object represents a message queue, which offers different methods for consuming messages.
///
/// Intented to be implemented as part of the MW library code. Is not exposed to the user.
#[async_trait]
pub trait Publication: Send + Sync {
    /// Creates a polling message reader. Returns a Subscribe trait object.
    async fn subscribe(&self, options: &SubscribeOptions) -> SdvResult<Box<dyn Subscribe>>;

    /// Creates an asynchronous message stream.
    ///
    /// Each stream item can contain multiple individual messages, in the form of message_count and a byte buffer.
    /// If a min_interval bigger than Duration::ZERO is specified, the underlying stream won't call 'poll_next'
    /// more than once per specified interval.
    async fn observe(&self, options: &SubscribeOptions) -> SdvResult<ByteBuffersStream>;

    /// Creates a consumer-side cache of messages.
    async fn create_history(&self, options: &SubscribeOptions) -> SdvResult<Box<dyn ReadHistory>>;

    /// Creates a InstantRead handle, which allows reading directly from the publication without a subscription.
    fn create_instant_read(&self) -> Box<dyn InstantRead>;
}

/// Represents a consumer-side endpoint to a publication, which enables polling message.
///
/// Intented to be implemented as part of the MW library code. Is not exposed to the user.
#[automock]
pub trait Subscribe: Send {
    /// Returns a vector of messages from the publication queue.
    ///
    /// Returns a vector of byte arrays instead of a single byte buffer, to ensure the messages
    /// are parseable: trait implementations may or may not use fixed sizes for the messages,
    /// and may or may not encode the message size in the message.
    fn read_next_messages(&self, message_count: u32) -> SdvResult<Vec<Box<[u8]>>>;
}

/// Represents a consumer-side message cache for a publication.
///
/// Intented to be implemented as part of the MW library code. Is not exposed to the user.
#[automock]
pub trait ReadHistory: Send {
    /// Returns a vector of messages from the subscriber cache.
    ///
    /// Returns a vector of byte arrays instead of a single byte buffer, to ensure the messages
    /// are parseable: trait implementations may or may not use fixed sizes for the messages,
    /// and may or may not encode the message size in the message.
    fn read_from_history(&self, message_count: u32) -> SdvResult<Vec<Box<[u8]>>>;
}

/// Represents a handle to a publication, which enables reading directly from the publication.
///
/// Intented to be implemented as part of the MW library code. Is not exposed to the user.
#[automock]
#[async_trait]
pub trait InstantRead: Send + Sync {
    /// Returns the latest message from a publication if there is one, independent of a subscription.
    async fn read_latest_message(&self) -> SdvResult<Option<Box<[u8]>>>;
}
