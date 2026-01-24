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

#![allow(missing_docs)]
/// In-Process Pub/Sub Implementation
///
/// WARNING: this is an experimental sample implementation of the sdv_mw_rs_comms traits. It doesn't reflect
/// the exact behavior of SdvComms. It's a naive implementation of a (process-)local comms stack in order to
/// compile, run, and test the sdv_mw_rs_comms crate.
use crate::LocalComms;
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::Stream;
use sdv_mw_rs_comms::{
    Availability, ByteBuffersStream, InstantRead, Publication, Publish, ReadHistory, Subscribe,
    SubscribeOptions,
};
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::cmp::min;
use std::collections::{HashSet, VecDeque};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

pub struct LocalPublisher {
    msg_q: Arc<Mutex<LocalMessageQueue>>,
    comms: Arc<LocalComms>,
    publication_name: String,
}

impl LocalPublisher {
    pub fn new(
        comms: Arc<LocalComms>,
        msg_q: Arc<Mutex<LocalMessageQueue>>,
        publication_name: String,
    ) -> Self {
        // Send Availability::Available to all registered streams
        comms.send_availability_event(&publication_name, Availability::Available);

        Self { comms, msg_q, publication_name }
    }
}

impl Drop for LocalPublisher {
    fn drop(&mut self) {
        // Send Availability::Unavailable to all registered streams
        self.comms.send_availability_event(&self.publication_name, Availability::Unavailable);
    }
}

impl Publish for LocalPublisher {
    fn publish_messages(&self, messages: &[Box<[u8]>]) -> SdvResult<()> {
        self.msg_q.lock().unwrap().publish_messages(messages)
    }
}

pub struct LocalPublication {
    msg_q: Arc<Mutex<LocalMessageQueue>>,
}

impl LocalPublication {
    pub fn new(msg_queue: Arc<Mutex<LocalMessageQueue>>) -> Self {
        Self { msg_q: msg_queue }
    }
}

#[async_trait]
impl Publication for LocalPublication {
    /// Creates and registers a Subscribe trait object as a polling message reader.
    async fn subscribe(&self, options: &SubscribeOptions) -> SdvResult<Box<dyn Subscribe>> {
        let mut msg_q = self.msg_q.lock().unwrap();
        let id = msg_q.subscribe(options);
        drop(msg_q);
        Ok(Box::new(LocalSubscriber::new(self.msg_q.clone(), id)))
    }

    /// Returns an SdvResult containing a stream of byte buffers.
    ///
    /// A list of messages with len > 0 is provided per stream invocation of `poll_next`.
    /// If a min_interval bigger than Duration::ZERO is specified, the underlying stream won't
    /// call 'poll_next' more than once per specified interval.
    async fn observe(&self, _options: &SubscribeOptions) -> SdvResult<ByteBuffersStream> {
        let mut msg_q = self.msg_q.lock().unwrap();
        let stream = msg_q.observe();
        drop(msg_q);

        Ok(stream)
    }

    /// Creates a consumer-side cache of messages for a publication.
    async fn create_history(&self, options: &SubscribeOptions) -> SdvResult<Box<dyn ReadHistory>> {
        let mut msg_q = self.msg_q.lock().unwrap();
        let id = msg_q.enable_history(options.get_message_count());
        drop(msg_q);
        Ok(Box::new(LocalHistory::new(self.msg_q.clone(), id)))
    }

    /// TODO: documentation
    fn create_instant_read(&self) -> Box<dyn InstantRead> {
        Box::new(LocalInstantReader::new(self.msg_q.clone()))
    }
}

pub struct LocalSubscriber {
    msg_q: Arc<Mutex<LocalMessageQueue>>,
    id: usize,
}

impl LocalSubscriber {
    pub fn new(msg_queue: Arc<Mutex<LocalMessageQueue>>, id: usize) -> Self {
        Self { msg_q: msg_queue, id }
    }
}

impl Subscribe for LocalSubscriber {
    // Returns the n (message_count) next messages from the index of last read, if available.
    // If less than message_count messages are available, it will return all available messages.
    // Returns OutOfRange if Overflow occurred.
    fn read_next_messages(&self, message_count: u32) -> SdvResult<Vec<Box<[u8]>>> {
        self.msg_q.lock().unwrap().read_next_messages(self.id, message_count)
    }
}

pub struct LocalHistory {
    msg_q: Arc<Mutex<LocalMessageQueue>>,
    id: usize,
}

impl LocalHistory {
    pub fn new(msg_queue: Arc<Mutex<LocalMessageQueue>>, id: usize) -> Self {
        Self { msg_q: msg_queue, id }
    }
}

impl ReadHistory for LocalHistory {
    /// Returns the n (message_count) latests messages from subscriber history, if available.
    fn read_from_history(&self, message_count: u32) -> SdvResult<Vec<Box<[u8]>>> {
        self.msg_q.lock().unwrap().read_from_history(self.id, message_count)
    }
}

pub struct LocalInstantReader {
    msg_q: Arc<Mutex<LocalMessageQueue>>,
}

impl LocalInstantReader {
    pub fn new(msg_queue: Arc<Mutex<LocalMessageQueue>>) -> Self {
        Self { msg_q: msg_queue }
    }
}

#[async_trait]
impl InstantRead for LocalInstantReader {
    async fn read_latest_message(&self) -> SdvResult<Option<Box<[u8]>>> {
        self.msg_q.lock().unwrap().read_latest_message()
    }
}

pub struct LocalMessageQueue {
    message_queue: VecDeque<Box<[u8]>>,
    publication_buffer_size: usize,
    read_pointers: Vec<usize>,
    read_overflow: HashSet<usize>,
    subscriber_cache: Vec<VecDeque<Box<[u8]>>>,
    senders: Vec<mpsc::Sender<SdvResult<(u32, Vec<u8>)>>>,
}

impl Default for LocalMessageQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalMessageQueue {
    pub fn new() -> Self {
        Self {
            message_queue: VecDeque::new(),
            publication_buffer_size: 0,
            read_pointers: Vec::new(),
            read_overflow: HashSet::new(),
            subscriber_cache: Vec::new(),
            senders: Vec::new(),
        }
    }

    pub fn register_publisher(&mut self, message_count: u32) {
        self.publication_buffer_size = message_count as usize;
        self.message_queue.reserve(self.publication_buffer_size);
    }

    fn publish_messages(&mut self, messages: &[Box<[u8]>]) -> SdvResult<()> {
        if self.publication_buffer_size == 0 {
            return Err(SdvStatus::new(SdvStatusCode::Unavailable));
        }

        self.feed_streams(messages);

        // update queue
        for message in messages {
            self.message_queue.push_back(message.clone());
            // TODO(b/352248016): respect history size
            for cache in &mut self.subscriber_cache {
                cache.push_back(message.clone());
            }
        }

        self.handle_overflow();
        Ok(())
    }

    fn handle_overflow(&mut self) {
        if self.message_queue.len() <= self.publication_buffer_size {
            return;
        }
        let drop_count = self.message_queue.len() - self.publication_buffer_size;
        for i in 0..self.read_pointers.len() {
            let read_ptr = &mut self.read_pointers[i];
            if *read_ptr < drop_count {
                // messages were not read fast enough, reset pointer
                *read_ptr = 0;
                self.read_overflow.insert(*read_ptr);
            } else {
                // no overflow, correct read position to reflect dropped messages
                *read_ptr -= drop_count;
            }
        }

        self.message_queue.drain(..drop_count);
    }

    fn subscribe(&mut self, options: &SubscribeOptions) -> usize {
        // The next published message is the first one for this subscriber
        let mut read_idx = self.message_queue.len();
        if options.read_on_subscribe() && read_idx != 0 {
            read_idx -= 1;
        }
        self.read_pointers.push(read_idx);
        self.read_pointers.len() - 1
    }

    fn read_next_messages(
        &mut self,
        subscriber_id: usize,
        message_count: u32,
    ) -> SdvResult<Vec<Box<[u8]>>> {
        if self.read_overflow.contains(&subscriber_id) {
            self.read_overflow.remove(&subscriber_id);
            return Err(SdvStatus::new(SdvStatusCode::OutOfRange));
        }

        let read_ptr = &mut self.read_pointers[subscriber_id];
        if *read_ptr > self.message_queue.len() {
            // Overflowed read pointer: read_ptr < message_queue.len()
            // Waiting for next published message: read_ptr == message_queue.len()
            *read_ptr = self.message_queue.len();
            return Err(SdvStatus::new(SdvStatusCode::OutOfRange));
        }

        let range_end = min(*read_ptr + (message_count as usize), self.message_queue.len());
        let res = self.message_queue.range(*read_ptr..range_end).cloned().collect();
        *read_ptr = range_end;

        Ok(res)
    }

    fn observe(&mut self) -> Pin<Box<dyn Stream<Item = SdvResult<(u32, Vec<u8>)>> + Send>> {
        let (sender, receiver) = mpsc::channel::<SdvResult<(u32, Vec<u8>)>>(10);
        self.senders.push(sender);
        Box::pin(receiver)
    }

    fn feed_streams(&mut self, messages: &[Box<[u8]>]) {
        let message_count = messages.len() as u32;
        let concat: Vec<u8> = messages.iter().flat_map(|arr| arr.iter().copied()).collect();

        self.senders.retain_mut(|s| match s.try_send(Ok((message_count, concat.clone()))) {
            Ok(()) => true,
            Err(_) => false, // Receiver is gone
        });
    }

    fn read_latest_message(&mut self) -> SdvResult<Option<Box<[u8]>>> {
        let published_count = self.message_queue.len();
        match published_count {
            0 => Ok(None),
            1.. => Ok(Some(self.message_queue[published_count - 1].clone())),
        }
    }

    fn enable_history(&mut self, _message_count: u32) -> usize {
        //TODO(b/352248016): Limit cache to message_count
        self.subscriber_cache.push(VecDeque::new());
        self.subscriber_cache.len() - 1
    }

    fn read_from_history(
        &mut self,
        history_id: usize,
        message_count: u32,
    ) -> SdvResult<Vec<Box<[u8]>>> {
        let cache = match self.subscriber_cache.get(history_id) {
            None => return Err(SdvStatus::new(SdvStatusCode::Unavailable)),
            Some(cache) => cache,
        };

        let num_msgs_returned = min(cache.len(), message_count as usize);
        let start = cache.len() - num_msgs_returned;
        Ok(cache.range(start..).cloned().collect())
    }
}
