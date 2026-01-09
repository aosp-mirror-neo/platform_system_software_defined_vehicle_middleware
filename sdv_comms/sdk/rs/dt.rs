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

//! Module that defines the SDK API for Data Tunnel.

mod publisher;
pub use publisher::Publisher;

mod subscriber;
pub use subscriber::Subscriber;

mod subscriber_options;
pub use subscriber_options::SubscriberOptions;

mod subscriber_stream;
pub use subscriber_stream::SubscriberStream;

mod history_reader;
pub use history_reader::HistoryReader;

mod instant_reader;
pub use instant_reader::InstantReader;

#[cfg(test)]
mod e2e_test;
