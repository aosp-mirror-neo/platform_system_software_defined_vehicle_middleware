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

//! SDV COMMS DT Client<->Agent Communication Channel Rust implementation

#![allow(unused_imports)]
#![allow(dead_code)]

use crate::ffi::ffi_types;

use core::ffi::c_void;
use log::{debug, error, warn};
use parking_lot::{Condvar, Mutex};
use sdv_comms_common::callback::ffi::Callback;
use sdv_status_rs::derive;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::cmp;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::convert::{From, Into, TryFrom};
use std::fs::File;
use std::io::{Read, Write};
use std::option::Option;
use std::os::fd::OwnedFd;
use std::sync::{atomic, Arc};
use std::thread::{spawn, JoinHandle, Thread};
use std::time::{Duration, Instant};
use zerocopy::{FromBytes, Immutable, IntoBytes};

#[derive(Clone, Copy, Debug)]
enum MessageType {
    Undefined,
    DataAvailable,
    Exception,
    SetNotificationState,
}

type NotificationCallback = Callback<ffi_types::ASDV_NotificationType>;
struct CallbackEntry {
    callback_id: i32,
    callback: Box<dyn FnMut(ffi_types::ASDV_NotificationType) -> SdvResult<()> + Send>,
    min_interval: Option<Duration>,
    last_invoke: Option<Instant>,
    scheduled_count: u32,
}

type NotificationEntriesMap = HashMap<i32, Vec<CallbackEntry>>;

const MESSAGE_HEADER_V1: u16 = 0xF0F1;

#[derive(Debug, FromBytes, Immutable, IntoBytes)]
#[repr(C)]
pub(crate) struct Message {
    header: u16,
    message_type: u16,
    args: [i32; 3],
}

impl From<u16> for MessageType {
    fn from(u: u16) -> Self {
        match u {
            1 => MessageType::DataAvailable,
            2 => MessageType::Exception,
            3 => MessageType::SetNotificationState,
            _ => MessageType::Undefined,
        }
    }
}

impl From<MessageType> for u16 {
    fn from(mt: MessageType) -> Self {
        match mt {
            MessageType::Undefined => 0,
            MessageType::DataAvailable => 1,
            MessageType::Exception => 2,
            MessageType::SetNotificationState => 3,
        }
    }
}

// TryFrom because not every message type that sent over the CommChannel
// is visible to the application.
impl TryFrom<MessageType> for ffi_types::ASDV_NotificationType {
    type Error = SdvStatus;

    fn try_from(mt: MessageType) -> SdvResult<Self> {
        match mt {
            MessageType::DataAvailable => Ok(ffi_types::ASDV_NotificationType::DataAvailable),
            MessageType::Exception => Ok(ffi_types::ASDV_NotificationType::Exception),
            _ => Err(SdvStatus::with_message(
                SdvStatusCode::Internal,
                format!("MessageType {mt:?} has no ABI representation."),
            )),
        }
    }
}

impl Message {
    pub fn new_data_available(publication_id: i32) -> Self {
        Message {
            header: MESSAGE_HEADER_V1,
            message_type: MessageType::DataAvailable.into(),
            args: [publication_id, 0, 0],
        }
    }

    pub fn new_exception(publication_id: i32, code: i32) -> Self {
        Message {
            header: MESSAGE_HEADER_V1,
            message_type: MessageType::Exception.into(),
            args: [publication_id, code, 0],
        }
    }

    pub fn new_set_notification_state(publication_id: i32, enabled: bool) -> Self {
        Message {
            header: MESSAGE_HEADER_V1,
            message_type: MessageType::SetNotificationState.into(),
            args: [publication_id, if enabled { 1 } else { 0 }, 0],
        }
    }

    fn is_valid(&self) -> bool {
        self.header == MESSAGE_HEADER_V1
            && !matches!(
                <u16 as Into<MessageType>>::into(self.message_type),
                MessageType::Undefined
            )
    }
}

impl Default for Message {
    fn default() -> Self {
        Message {
            header: MESSAGE_HEADER_V1,
            message_type: MessageType::Undefined.into(),
            args: [0, 0, 0],
        }
    }
}

enum ScheduleAction {
    Invoke(ScheduleEntry),
    Wait,
    Exception,
}

#[derive(Debug, Eq)]
struct ScheduleEntry {
    schedule_time: Instant,
    publication_id: i32,
    callback_id: i32,
}

impl Ord for ScheduleEntry {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.schedule_time.cmp(&other.schedule_time)
    }
}

impl PartialOrd for ScheduleEntry {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ScheduleEntry {
    fn eq(&self, other: &Self) -> bool {
        self.schedule_time == other.schedule_time
    }
}

struct Schedule {
    entries: Vec<ScheduleEntry>,
}

impl Schedule {
    fn create() -> Self {
        Schedule { entries: Vec::new() }
    }

    fn insert(&mut self, schedule_time: Instant, publication_id: i32, callback_id: i32) {
        let new_entry = ScheduleEntry { schedule_time, publication_id, callback_id };
        let pos = self.entries.binary_search(&new_entry).unwrap_or_else(|e| e);
        self.entries.insert(pos, new_entry);
    }

    fn delete(&mut self, publication_id: i32, callback_id: i32) {
        self.entries.retain(|entry| {
            entry.publication_id != publication_id || entry.callback_id != callback_id
        });
    }

    fn pop(&mut self) -> Option<ScheduleEntry> {
        if !self.entries.is_empty() {
            return Some(self.entries.remove(0));
        }
        None
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub(crate) struct CommChannel {
    write_pipe_stream: File,
    read_thread_handle: Option<JoinHandle<()>>,
    scheduler_thread_handle: Option<JoinHandle<()>>,
    // TODO: Replace with closing of both comm pipes to break on read() wait
    want_to_exit: Arc<atomic::AtomicBool>,
    callbacks: Arc<Mutex<NotificationEntriesMap>>,
    callback_id_counter: Arc<atomic::AtomicI32>,
    schedule: Arc<(Mutex<Schedule>, Condvar)>,
}

impl CommChannel {
    pub fn create(read_fd: OwnedFd, write_fd: OwnedFd) -> Self {
        debug!("Creating CommChannel with {read_fd:?}, {write_fd:?}");
        let read_stream: File = read_fd.into();
        let write_pipe_stream: File = write_fd.into();
        let callbacks = Arc::new(Mutex::new(HashMap::new()));
        let schedule = Arc::new((Mutex::new(Schedule::create()), Condvar::new()));
        let want_to_exit = Arc::new(atomic::AtomicBool::new(false));
        let read_thread_handle = spawn({
            let callbacks = Arc::clone(&callbacks);
            let schedule = Arc::clone(&schedule);
            let want_to_exit = want_to_exit.clone();
            move || {
                Self::read_thread(read_stream, want_to_exit, callbacks, schedule);
            }
        });
        let scheduler_thread_handle = spawn({
            let callbacks = Arc::clone(&callbacks);
            let schedule = Arc::clone(&schedule);
            let want_to_exit = want_to_exit.clone();
            move || {
                Self::scheduler_thread(want_to_exit, callbacks, schedule);
            }
        });
        CommChannel {
            write_pipe_stream,
            read_thread_handle: Some(read_thread_handle),
            scheduler_thread_handle: Some(scheduler_thread_handle),
            want_to_exit,
            callbacks,
            callback_id_counter: Arc::new(atomic::AtomicI32::new(0)),
            schedule,
        }
    }

    // Closures registered with this method (and removed via unregister_callback) are
    // triggered by per-topic notification messages sent by DT Agent.  When no closures
    // are set for a given topic, we have no other use for those messages.  So we send
    // SetNotificationState messages to DT Agent to turn notifications on or off whenever
    // our set of closures for a given publication_id transitions from empty to non-empty
    // (turning notifications on), or from non-empty to empty (turning them off).
    //
    // If opt_old_callback_id contains a value, the new callback will replace any callback
    // already registered with that ID for the same publication; however, a new callback ID
    // will still be returned and need to be used for subsequent calls to this method or
    // unregister_callback.
    pub fn register_or_replace_closure<
        C: FnMut(ffi_types::ASDV_NotificationType) -> SdvResult<()> + 'static + Send,
    >(
        &mut self,
        publication_id: i32,
        opt_old_callback_id: Option<i32>,
        closure: C,
        min_interval_ms: u32,
    ) -> SdvResult<i32> {
        debug!("Registering callback for publication_id {publication_id}");

        let mut map = self.callbacks.lock();
        let callback_id = self.callback_id_counter.fetch_add(1, atomic::Ordering::SeqCst);
        let min_interval = if min_interval_ms == 0 {
            None
        } else {
            Some(Duration::from_millis(min_interval_ms as u64))
        };

        let callback_entry = CallbackEntry {
            callback_id,
            callback: Box::new(closure),
            min_interval,
            last_invoke: None,
            scheduled_count: 0,
        };
        match map.entry(publication_id) {
            Entry::Vacant(e) => {
                e.insert(vec![callback_entry]);
            }
            Entry::Occupied(e) => {
                let cb_entry = e.into_mut();
                let existing_idx = opt_old_callback_id
                    .and_then(|id| cb_entry.iter().position(|e| e.callback_id == id));
                match existing_idx {
                    Some(idx) => {
                        cb_entry[idx] = callback_entry;
                    }
                    None => {
                        debug!(
                            "register_or_replace_closure called with old callback id but no entry found."
                        );
                        cb_entry.push(callback_entry);
                    }
                };
            }
        }

        // Ensure notifications enabled in DT Agent and trigger DataAvailable if FMQ not empty
        drop(map); // Release immutable borrow of self
        self.send_set_notification_state(publication_id, true)?;

        Ok(callback_id)
    }

    pub fn register_closure<
        C: FnMut(ffi_types::ASDV_NotificationType) -> SdvResult<()> + 'static + Send,
    >(
        &mut self,
        publication_id: i32,
        closure: C,
        min_interval_ms: u32,
    ) -> SdvResult<i32> {
        self.register_or_replace_closure(publication_id, None, closure, min_interval_ms)
    }

    pub fn register_or_replace_callback(
        &mut self,
        publication_id: i32,
        opt_old_callback_id: Option<i32>,
        callback: NotificationCallback,
        min_interval_ms: u32,
    ) -> SdvResult<i32> {
        self.register_or_replace_closure(
            publication_id,
            opt_old_callback_id,
            move |message_type| callback.invoke(message_type),
            min_interval_ms,
        )
    }

    pub fn unregister_callback(&mut self, publication_id: i32, callback_id: i32) -> SdvResult<()> {
        debug!("Unregistering callback with id {callback_id} for publication_id {publication_id}");
        let mut map = self.callbacks.lock();
        if let Entry::Occupied(mut entry) = map.entry(publication_id) {
            // Lookup for the required callback_id
            let cb_entry = entry.get_mut();
            if let Some(idx) = cb_entry.iter().position(|e| e.callback_id == callback_id) {
                cb_entry.swap_remove(idx);
                if cb_entry.is_empty() {
                    entry.remove_entry();
                    // Disable notifications from Agent since this we just removed the
                    // last closure for this publication_id.
                    drop(map); // Release immutable borrow of self
                    self.send_set_notification_state(publication_id, false)?;
                }
                Ok(())
            } else {
                debug!(
                    "No callback with id {callback_id} for publication_id {publication_id} found"
                );
                Ok(())
            }
        } else {
            debug!("No callback with id {callback_id} for publication_id {publication_id} found");
            Ok(())
        }
    }

    pub fn get_callback_count(&self) -> usize {
        self.callbacks.lock().values().map(|v| v.len()).sum()
    }

    fn send_set_notification_state(&mut self, publication_id: i32, enabled: bool) -> SdvResult<()> {
        let message = Message::new_set_notification_state(publication_id, enabled);
        self.write_message(&message).map_err(|e| {
            SdvStatus::with_cause_and_message(
                SdvStatusCode::Internal,
                e,
                format!("Unable to send SetNotificationState({enabled}) to Agent"),
            )
        })
    }

    /// Returns None if the specified callback entry must be triggered
    /// immediately or Some(Instant) if it suppose to be scheduled for later
    fn get_invoke_callback_time(
        time: &Instant,
        entry: &CallbackEntry,
        message_type: MessageType,
    ) -> Option<Instant> {
        // Only MessageType::DataAvailable can be scheduled. All other other callbacks
        // must be invoked immediately.
        if !matches!(message_type, MessageType::DataAvailable) {
            return None;
        }

        // Scheduling only if min_interval is set
        if let Some(min_interval) = entry.min_interval {
            // And it was already invoked before
            if let Some(last_invoke) = entry.last_invoke {
                // And last invocation was less than min_interval ago
                let earliest = last_invoke + min_interval;
                if &earliest > time {
                    return Some(earliest);
                }
            }
        }

        // Otherwise invoking the callback immediately
        None
    }

    fn handle_incoming_message(
        message: &Message,
        cb_entries: &Mutex<NotificationEntriesMap>,
        schedule: &(Mutex<Schedule>, Condvar),
    ) {
        if message.is_valid() {
            let message_type: MessageType = message.message_type.into();
            match message_type {
                MessageType::DataAvailable | MessageType::Exception => {
                    let publication_id = message.args[0];
                    if let Some(entries) = cb_entries.lock().get_mut(&publication_id) {
                        let now = Instant::now();
                        let Ok(notification_type) =
                            ffi_types::ASDV_NotificationType::try_from(message_type)
                        else {
                            error!(
                                "Got MessageType {message_type:?} that cannot be passed to application."
                            );
                            return;
                        };
                        entries.retain_mut(|entry| {
                            if notification_type == ffi_types::ASDV_NotificationType::Exception {
                                // Check if callback with this id is already scheduled
                                if entry.scheduled_count != 0 {
                                    let (lock, _) = schedule;
                                    let mut schedule = lock.lock();
                                    schedule.delete(publication_id, entry.callback_id);
                                    // Trigger DataAvailable callback if it was scheduled
                                    _ = (entry.callback)(ffi_types::ASDV_NotificationType::DataAvailable);
                                }

                                error!("Exception received for callback id {}, publication {}",
                                    entry.callback_id,
                                    publication_id
                                );
                                // Drop the callback
                                false
                            } else if let Some(sched_time) =
                                CommChannel::get_invoke_callback_time(&now, entry, message_type)
                            {
                                // Check if callback with this id wasn't already scheduled
                                if entry.scheduled_count == 0 {
                                    let (lock, cvar) = schedule;
                                    let mut schedule = lock.lock();
                                    schedule.insert(sched_time, publication_id, entry.callback_id);
                                    cvar.notify_one();
                                }
                                entry.scheduled_count += 1;
                                true
                            } else {
                                let res = (entry.callback)(notification_type);
                                if let Err(e) = res {
                                    if e.status_code() != SdvStatusCode::ResourceExhausted {
                                        // TODO: need to remove the problematic callback
                                        error!(
                                            "Callback invocation for callback id {}, publication {} has failed! {:?}",
                                            entry.callback_id,
                                            publication_id,
                                            e
                                        );
                                    }
                                }
                                // TODO: only if min_time_interval is set
                                entry.last_invoke = Some(now);
                                entry.scheduled_count = 0;
                                true
                            }
                        });
                    }
                }
                _ => {
                    warn!("Received a message with invalid type {}", message.message_type);
                }
            }
        } else {
            error!("Received a corrupted Comm Channel message from the pipe");
        }
    }

    fn scheduler_thread(
        want_to_exit: Arc<atomic::AtomicBool>,
        callbacks: Arc<Mutex<NotificationEntriesMap>>,
        schedule: Arc<(Mutex<Schedule>, Condvar)>,
    ) {
        loop {
            if want_to_exit.load(atomic::Ordering::Relaxed) {
                break;
            }

            let (lock, cvar) = &*schedule;
            let mut schedule_guard = lock.lock();
            let sleep_duration = if !schedule_guard.entries.is_empty() {
                let now = Instant::now();
                let next_action_time = schedule_guard.entries[0].schedule_time;
                if next_action_time > now {
                    next_action_time.duration_since(now)
                } else {
                    Duration::ZERO
                }
            } else {
                Duration::MAX
            };
            cvar.wait_for(&mut schedule_guard, sleep_duration);
            let now = Instant::now();
            let next_action = if !schedule_guard.entries.is_empty() {
                if now < schedule_guard.entries[0].schedule_time {
                    ScheduleAction::Wait
                } else {
                    ScheduleAction::Invoke(schedule_guard.entries.remove(0))
                }
            } else {
                ScheduleAction::Exception
            };
            drop(schedule_guard);

            match next_action {
                ScheduleAction::Invoke(sched_entry) => {
                    if let Some(cb_entries) = callbacks.lock().get_mut(&sched_entry.publication_id)
                    {
                        for cb_entry in cb_entries {
                            if sched_entry.callback_id == cb_entry.callback_id {
                                // All the scheduled callbacks have DataAvailable type
                                // TODO: We shouldn't ignore the invocation's result, but report
                                //       the subscriber via an Exception notification type to let
                                //       it react accordingly.
                                if let Err(e) = (cb_entry.callback)(
                                    ffi_types::ASDV_NotificationType::DataAvailable,
                                ) {
                                    error!("Callback invocation failed {e}");
                                }
                                // TODO: only if min_time_interval is set
                                cb_entry.last_invoke = Some(now);
                                cb_entry.scheduled_count = 0;
                            }
                        }
                    }
                }
                ScheduleAction::Wait => {
                    // Delay time is calculated right before wait_for()
                }
                ScheduleAction::Exception => {
                    break;
                }
            }
        }
    }

    fn read_thread(
        mut read_stream: File,
        want_to_exit: Arc<atomic::AtomicBool>,
        callbacks: Arc<Mutex<NotificationEntriesMap>>,
        schedule: Arc<(Mutex<Schedule>, Condvar)>,
    ) {
        let mut message = Message::default();
        loop {
            if want_to_exit.load(atomic::Ordering::Relaxed) {
                break;
            }

            match Self::read_message(&mut read_stream, &mut message) {
                Ok(()) => {
                    Self::handle_incoming_message(&message, &callbacks, &schedule);
                }
                _ => {
                    error!("Unable to read Comm Channel message from the pipe");
                    break;
                }
            }
        }
    }

    fn read_message(read_stream: &mut File, message: &mut Message) -> std::io::Result<()> {
        read_stream.read_exact(message.as_bytes_mut())?;
        Ok(())
    }

    pub fn write_message(&mut self, message: &Message) -> std::io::Result<()> {
        self.write_pipe_stream.write_all(message.as_bytes())?;
        self.write_pipe_stream.flush()?;
        Ok(())
    }
}

impl Drop for CommChannel {
    fn drop(&mut self) {
        self.want_to_exit.store(true, atomic::Ordering::Relaxed);
        if let Some(handle) = self.read_thread_handle.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.scheduler_thread_handle.take() {
            let (_, cvar) = &*self.schedule;
            cvar.notify_one();
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    // TODO(b/403804741): Add further unit tests to cover scheduling, callback
    // execution, sending messages, and any other testable behaviors in CommChannel.

    use super::*;
    use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
    use std::os::fd::AsFd;
    use std::os::unix::net::UnixStream;

    struct TestCommChannel {
        writer: UnixStream,
        reader: UnixStream,
        pub comm_channel: CommChannel,
    }

    impl Drop for TestCommChannel {
        fn drop(&mut self) {
            // Ensure the read thread isn't stuck on a blocking read; shutting
            // down the writer here will cause the read thread to get EOF and exit.
            self.writer
                .shutdown(std::net::Shutdown::Both)
                .expect("Writer shutdown should succeed.");
            self.reader
                .shutdown(std::net::Shutdown::Both)
                .expect("Reader shutdown should succeed.");
        }
    }

    impl TestCommChannel {
        fn new() -> TestCommChannel {
            // CommChannel is constructed with OwnedFds for reading and writing.  Normally those
            // fds are for pipes created by DT Agent.  But there's not yet a stable Rust wrapper
            // for pipes (see https://github.com/rust-lang/rust/issues/127154), and the os_pipe
            // crate isn't currently available in Android (see b/402757376).
            //
            // So instead we're using Unix domain socket pairs for these unit tests,
            // since they work like bidirectional pipes with a separate fd for each end;
            // CommChannel (and our test code) will ignore the fact that they're bidirectional.
            let (rd0, wr0) = UnixStream::pair().unwrap();
            let (rd1, wr1) = UnixStream::pair().unwrap();

            TestCommChannel {
                writer: wr0,
                reader: rd1,
                comm_channel: CommChannel::create(rd0.into(), wr1.into()),
            }
        }

        // Reads all messages sent by the CommChannel since last call.
        fn read_all_available(&mut self) -> Vec<Message> {
            let mut result = Vec::new();
            // Poll messages in a loop until we hit a 1s timeout.
            loop {
                let mut fds: [PollFd; 1] = [PollFd::new(self.reader.as_fd(), PollFlags::POLLIN)];
                let nfds = poll(&mut fds, PollTimeout::from(1000u16)).expect("poll() error!");
                if nfds == 0 {
                    break;
                }

                let mut message = Message::default();
                self.reader
                    .read_exact(message.as_bytes_mut())
                    .expect("Error reading ready Message!");
                result.push(message);
            }
            result
        }
    }

    #[test]
    fn test_enable_and_disable_notifications() {
        let mut fixture = TestCommChannel::new();

        const PUB1_ID: i32 = 15;
        const PUB2_ID: i32 = 17;

        // First closure registration generates a notification enable message
        let callback1_id = fixture
            .comm_channel
            .register_closure(PUB1_ID, |_| Ok(()), 0)
            .expect("Closure registration should have succeeded.");
        {
            let msgs = fixture.read_all_available();
            assert_eq!(msgs.len(), 1);
            assert_eq!(msgs[0].message_type, MessageType::SetNotificationState.into());
            assert_eq!(msgs[0].args[0], PUB1_ID); // publication_id
            assert_eq!(msgs[0].args[1], 1); // enabled
        }

        // Replacing a closure generates another SetNotificationState.
        let callback2_id = fixture
            .comm_channel
            .register_or_replace_closure(PUB1_ID, Some(callback1_id), |_| Ok(()), 0)
            .expect("Closure registration should have succeeded.");
        {
            let msgs = fixture.read_all_available();
            assert_eq!(msgs.len(), 1);
            assert_eq!(msgs[0].message_type, MessageType::SetNotificationState.into());
            assert_eq!(msgs[0].args[0], PUB1_ID); // publication_id
            assert_eq!(msgs[0].args[1], 1); // enabled
        }

        // Adding a closure for the same publication generates yet another SetNotificationState.a
        let callback3_id = fixture
            .comm_channel
            .register_closure(PUB1_ID, |_| Ok(()), 0)
            .expect("Closure registration should have succeeded.");
        {
            let msgs = fixture.read_all_available();
            assert_eq!(msgs.len(), 1);
            assert_eq!(msgs[0].message_type, MessageType::SetNotificationState.into());
            assert_eq!(msgs[0].args[0], PUB1_ID); // publication_id
            assert_eq!(msgs[0].args[1], 1); // enabled
        }

        // Different publication_id generates message.
        let callback4_id = fixture
            .comm_channel
            .register_closure(PUB2_ID, |_| Ok(()), 0)
            .expect("Closure registration should have succeeded.");
        {
            let msgs = fixture.read_all_available();
            assert_eq!(msgs.len(), 1);
            assert_eq!(msgs[0].message_type, MessageType::SetNotificationState.into());
            assert_eq!(msgs[0].args[0], PUB2_ID); // publication_id
            assert_eq!(msgs[0].args[1], 1); // enabled
        }

        // Unregistering a callbacks for a pub does not generate message
        // when there are other pubs remaining.
        fixture
            .comm_channel
            .unregister_callback(PUB1_ID, callback2_id)
            .expect("Closure removal should have succeeded");
        assert!(fixture.read_all_available().is_empty());

        // Unregistering last callback for a pub generates disable message.
        fixture
            .comm_channel
            .unregister_callback(PUB1_ID, callback3_id)
            .expect("Closure removal should have succeeded");
        {
            let msgs = fixture.read_all_available();
            assert_eq!(msgs.len(), 1);
            assert_eq!(msgs[0].message_type, MessageType::SetNotificationState.into());
            assert_eq!(msgs[0].args[0], PUB1_ID); // publication_id
            assert_eq!(msgs[0].args[1], 0); // disabled
        }

        // Unregistering only callback for a pub generates disable message.
        fixture
            .comm_channel
            .unregister_callback(PUB2_ID, callback4_id)
            .expect("Closure removal should have succeeded");
        {
            let msgs = fixture.read_all_available();
            assert_eq!(msgs.len(), 1);
            assert_eq!(msgs[0].message_type, MessageType::SetNotificationState.into());
            assert_eq!(msgs[0].args[0], PUB2_ID); // publication_id
            assert_eq!(msgs[0].args[1], 0); // disabled
        }
    }
}
