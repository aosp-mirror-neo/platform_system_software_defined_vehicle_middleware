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
/// Local middleware fake.
///
/// WARNING: this is an experimental sample implementation of the sdv_mw_rs_comms traits. It doesn't reflect
/// the exact behavior of SdvComms. It's a naive implementation of a (process-)local comms stack in order to
/// compile, run, and test.
mod communicate_impl;
pub use communicate_impl::LocalComms;

mod pubsub_impl;
use pubsub_impl::{LocalMessageQueue, LocalPublisher};
