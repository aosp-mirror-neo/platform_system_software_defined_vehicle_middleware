// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//! Contains a trait for a death watcher.
//! It exists to split the Binder-specific logic from the business logic of the callback.

use anyhow;
use mockall::automock;
use std::fmt;
use std::marker::{Send, Sync};

// Fn, not FnOnce is required by the binder DeathRecipient even though it should be called once.
pub(crate) type Callback = Box<dyn Fn() + Sync + Send>;
/// Wrapper for the death callback that allows to track client process death.
#[automock]
pub(crate) trait DeathWatcher: fmt::Debug + Send + Sync {
    fn link_to_death(&mut self, callback: Callback) -> anyhow::Result<()>;
}
