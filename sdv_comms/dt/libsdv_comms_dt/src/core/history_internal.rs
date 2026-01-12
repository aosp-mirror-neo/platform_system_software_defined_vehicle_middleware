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

use google_sdv_data_tunnel::aidl::google::sdv::data_tunnel::PublicationDescriptor::PublicationDescriptor;
use google_sdv_identity::aidl::google::sdv::identity::UnitIdentifier::UnitIdentifier as UnitIdentifierAidl;
use log::{error, warn};
use sdv_comms_ctx::ContextImpl;
use sdv_comms_id::UnitIdentifier;
use sdv_comms_sd::metadata::PublisherUnitMetadata;
use sdv_fmq_rust::FmqWrapperRust;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::sync::{Arc, Mutex};

use crate::core::shared_internal;
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
    ctx: &'static ContextImpl,
    metadata: PublisherUnitMetadata,
    publication_id: i32,
    callback_id: i32,
    history_buffer: Arc<Mutex<HistoryBuffer>>,
}

impl Drop for HistoryInternal {
    fn drop(&mut self) {
        match super::state::get_state(self.ctx) {
            Ok(dt_state) => {
                let _ = dt_state
                    .comm_channel
                    .write()
                    .unwrap()
                    .unregister_callback(self.publication_id, self.callback_id);
            }
            Err(e) => {
                error!("Unable to get internal DT state from the context object: {e}");
            }
        };
    }
}

impl HistoryInternal {
    pub async fn subscribe_to_history(
        ctx: &'static ContextImpl,
        unit_identifier: &UnitIdentifier,
        history_buffer_size: u32,
    ) -> SdvResult<HistoryInternal> {
        if history_buffer_size == 0 {
            return Err(SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                "History buffer size can't be 0",
            ));
        }

        let dt_binder = ctx.get_agents().get_dt_binder();
        let dt_state = super::state::get_state(ctx)?;

        let mut publication_descriptor: PublicationDescriptor = Default::default();

        let unit_identifier_aidl: UnitIdentifierAidl = unit_identifier.into();
        dt_binder.SubscribeToPublication(&unit_identifier_aidl, &mut publication_descriptor)?;
        let publication_id = publication_descriptor.publication_id;
        let message_size = publication_descriptor.message_size as usize;

        let mut fmq = FmqWrapperRust::from_desc(&publication_descriptor.mq_desc, false);
        let history_buffer =
            Arc::new(Mutex::new(HistoryBuffer::new(message_size, history_buffer_size as usize)));

        // Move the reader pointer according to the subscription_flags
        shared_internal::adjust_read_pointer(
            &mut fmq,
            message_size,
            publication_descriptor.message_count as usize,
            ffi_types::ASDV_SubscriptionFlags_None,
        )?;

        let callback_id = dt_state.comm_channel.write().unwrap().register_closure(
            publication_id,
            {
                let history_buffer = Arc::clone(&history_buffer);
                move |_| {
                    if let Ok(mut history) = history_buffer.lock() {
                        Self::fetch_all_messages(&mut fmq, message_size, &mut history);
                    }
                    Ok(())
                }
            },
            /*min_interval_ms*/ 0,
        )?;

        let mut metadata = PublisherUnitMetadata::default();
        metadata.set_version(1);
        metadata.set_message_size(publication_descriptor.message_size as u32);
        metadata.set_message_count(publication_descriptor.message_count as u32);

        Ok(HistoryInternal { ctx, metadata, publication_id, callback_id, history_buffer })
    }

    fn fetch_all_messages(
        fmq: &mut FmqWrapperRust,
        message_size: usize,
        history: &mut HistoryBuffer,
    ) {
        let mut read_buffer = vec![0i8; message_size];
        while fmq.available_to_read() > 0 {
            if fmq.read_from_buffer(&mut read_buffer, message_size) {
                history.write_message(&read_buffer);
            } else {
                history.reset();
                warn!(
                    "HistoryReader: FMQ::read() failed due to overflow! Resetting history buffer."
                );
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
