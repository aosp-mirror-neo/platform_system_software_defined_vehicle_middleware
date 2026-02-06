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

use sdv_comms_common::callback::ffi::Callback;
use sdv_comms_common::callback::sdk::CallbackWrapper;
use sdv_comms_ctx::ContextImpl;
use sdv_comms_id::UnitIdentifier;
use sdv_comms_sd::metadata::PublisherUnitMetadata;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::sync::{Arc, Mutex};

use log::{error, warn};

use crate::core::subscriber_internal::SubscriberInternal;
use crate::ffi::ffi_types;

struct HistoryBuffer {
    message_size: usize,
    message_count: usize,
    buffer: Vec<i8>,
    buffer_filled: bool,
    write_ptr: usize,
}

impl HistoryBuffer {
    pub fn new(message_size: usize, message_count: usize) -> Self {
        HistoryBuffer {
            message_size,
            message_count,
            buffer: vec![0; message_size * message_count],
            buffer_filled: false,
            write_ptr: 0,
        }
    }

    pub fn available_to_read(&self) -> usize {
        if self.buffer_filled {
            self.message_count
        } else {
            self.write_ptr
        }
    }

    pub fn reset(&mut self) {
        self.buffer_filled = false;
        self.write_ptr = 0;
    }

    // writes a single message into the buffer
    pub fn write_message(&mut self, message: &[i8]) {
        assert!(message.len() == self.message_size);
        let write_idx = self.write_ptr * self.message_size;
        self.buffer[write_idx..(write_idx + self.message_size)].copy_from_slice(message);
        self.write_ptr = (self.write_ptr + 1) % self.message_count;
        if !self.buffer_filled && self.write_ptr == 0 {
            self.buffer_filled = true;
        }
    }

    pub fn read_messages(&self, messages: &mut [i8], num_messages: usize) -> usize {
        let available = self.available_to_read();
        let to_read = if num_messages > available { available } else { num_messages };
        if to_read == 0 {
            return 0;
        }
        if to_read <= self.write_ptr {
            // need to copy only one segment
            let read_length = to_read * self.message_size;
            let read_idx = (self.write_ptr - to_read) * self.message_size;
            messages[..read_length]
                .copy_from_slice(&self.buffer[read_idx..(read_idx + read_length)]);
        } else {
            // need to copy two segments
            let read_length_a = (to_read - self.write_ptr) * self.message_size;
            let read_idx_a = (self.message_count * self.message_size) - read_length_a;
            messages[0..read_length_a].copy_from_slice(&self.buffer[read_idx_a..]);
            // if the second segment is empty
            if self.write_ptr > 0 {
                let read_length_b = self.write_ptr * self.message_size;
                messages[read_length_a..(read_length_a + read_length_b)]
                    .copy_from_slice(&self.buffer[..read_length_b]);
            }
        }
        to_read
    }
}

pub struct HistoryInternal {
    // The subscriber must be kept alive to maintain the subscription and receive notifications,
    // even though it is not accessed directly by the public API of this struct.
    #[allow(dead_code)]
    subscriber: Arc<Mutex<SubscriberInternal>>,
    metadata: PublisherUnitMetadata,
    history_buffer: Arc<Mutex<HistoryBuffer>>,
}

impl HistoryInternal {
    fn validate_history_buffer_size(history_buffer_size: u32) -> SdvResult<()> {
        if history_buffer_size == 0 {
            return Err(SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                "History buffer size can't be 0",
            ));
        }
        Ok(())
    }

    pub async fn subscribe_to_history(
        ctx: &'static ContextImpl,
        unit_identifier: &UnitIdentifier,
        history_buffer_size: u32,
    ) -> SdvResult<HistoryInternal> {
        Self::validate_history_buffer_size(history_buffer_size)?;

        let subscriber = SubscriberInternal::subscribe_to_publication(
            ctx,
            unit_identifier,
            ffi_types::ASDV_SubscriptionFlags_None,
        )
        .await?;
        Self::create_history_internal(subscriber, history_buffer_size)
    }

    pub async fn subscribe_to_topic(
        ctx: &'static ContextImpl,
        unit_type: &sdv_comms_sd::discovery::UnitType,
        topic_name: &sdv_comms_id::names::TopicName,
        history_buffer_size: u32,
    ) -> SdvResult<HistoryInternal> {
        Self::validate_history_buffer_size(history_buffer_size)?;

        let subscriber = SubscriberInternal::subscribe_to_topic(
            ctx,
            unit_type,
            topic_name,
            ffi_types::ASDV_SubscriptionFlags_None,
        )
        .await?;
        Self::create_history_internal(subscriber, history_buffer_size)
    }

    fn create_history_internal(
        subscriber: SubscriberInternal,
        history_buffer_size: u32,
    ) -> SdvResult<HistoryInternal> {
        let metadata = subscriber.get_publisher_unit_metadata().clone();
        let message_size = metadata.get_message_size() as usize;

        let subscriber = Arc::new(Mutex::new(subscriber));
        let history_buffer =
            Arc::new(Mutex::new(HistoryBuffer::new(message_size, history_buffer_size as usize)));

        Self::register_notification_handler(
            subscriber.clone(),
            history_buffer.clone(),
            message_size,
        )?;

        Ok(HistoryInternal { subscriber, metadata, history_buffer })
    }

    fn register_notification_handler(
        subscriber: Arc<Mutex<SubscriberInternal>>,
        history_buffer: Arc<Mutex<HistoryBuffer>>,
        message_size: usize,
    ) -> SdvResult<()> {
        // The callback needs to access the subscriber to read messages, but the subscriber
        // owns the callback. To avoid a reference cycle (memory leak), we use a Weak reference.
        let subscriber_weak = Arc::downgrade(&subscriber);

        // TODO(b/479450652)
        // Refactor SubscriberInternal to accept FnMut via register_notification_closure
        // and handles the Callback ABI wrapping logic.
        let wrapper = CallbackWrapper::new(move |_: ffi_types::ASDV_NotificationType| {
            if let Some(subscriber_strong) = subscriber_weak.upgrade() {
                if let Ok(mut subscriber) = subscriber_strong.lock() {
                    if let Ok(mut history) = history_buffer.lock() {
                        Self::fetch_all_messages(&mut subscriber, message_size, &mut history);
                    }
                }
            }
        });

        let callback = Callback::new(
            CallbackWrapper::trampoline_func,
            Some(CallbackWrapper::<ffi_types::ASDV_NotificationType>::trampoline_clear),
            wrapper.into_void_ptr(),
        );

        subscriber
            .lock()
            .unwrap()
            .set_notification_handler(callback, /*min_interval_ms*/ 0)?;

        Ok(())
    }

    fn fetch_all_messages(
        subscriber: &mut SubscriberInternal,
        message_size: usize,
        history: &mut HistoryBuffer,
    ) {
        let mut read_buffer = vec![0i8; message_size];
        while subscriber.available_to_read().unwrap_or(0) > 0 {
            match subscriber.read(&mut read_buffer, 1) {
                Ok(1) => {
                    history.write_message(&read_buffer);
                }
                Ok(n) => {
                    // This should never happen
                    error!(
                        "HistoryInternal: Unexpected number of messages read: {}, expected 1",
                        n
                    );
                    break;
                }
                Err(e) if e.status_code() == SdvStatusCode::DataLoss => {
                    history.reset();
                    warn!(
                        "HistoryInternal: FMQ::read() failed due to overflow! Resetting history buffer."
                    );
                    // Reset complete. Continue looping to process any valid messages remaining.
                }
                Err(e) => {
                    error!("HistoryInternal: Read failed in HistoryInternal: {}", e);
                    break;
                }
            }
        }
    }

    pub fn available_in_history(&self) -> SdvResult<usize> {
        Ok(self.history_buffer.lock().unwrap().available_to_read())
    }

    pub fn read(&mut self, messages: &mut [i8], num_messages: u32) -> SdvResult<u32> {
        Ok(self.history_buffer.lock().unwrap().read_messages(messages, num_messages as usize)
            as u32)
    }

    pub fn get_message_size(&self) -> u32 {
        self.metadata.get_message_size()
    }

    pub fn get_publisher_unit_metadata(&self) -> &PublisherUnitMetadata {
        &self.metadata
    }
}
