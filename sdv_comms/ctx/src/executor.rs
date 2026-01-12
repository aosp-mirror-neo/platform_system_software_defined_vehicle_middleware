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

use futures_executor::{ThreadPool, ThreadPoolBuilder};
use sdv_status_rs::{derive, SdvResult, SdvStatusCode};
use std::future::Future;
use std::pin::Pin;

/// Trait to encapsulate Async operations within the Comms Stack.
/// This trait is modelled after the trait [executor_trait::Executor] (ref [1]).
///
/// [1] https://docs.rs/executor-trait/latest/executor_trait/trait.Executor.html
pub trait AsyncExecutor: Send + Sync {
    /// Concurrently spawn a future as a task on the executor, and returns immediately.
    fn spawn(&self, f: Pin<Box<dyn Future<Output = ()> + Send>>);
}

/// Implement the AsyncExecutor using a two-threaded thread-pool for scheduling tasks.
///
/// NOTE: This is a temporary fix to get the Comms Stack to work, this ThreadPool should be managed
/// by the ServiceBundleRunner instead.
#[derive(Debug)]
struct LocalExecutor {
    delegate: ThreadPool,
}

impl AsyncExecutor for LocalExecutor {
    fn spawn(&self, f: Pin<Box<dyn Future<Output = ()> + Send>>) {
        self.delegate.spawn_ok(f);
    }
}

/// Start a local Executor for enabling async scheduling in the Comms Stack.
///
/// NOTE: This is a temporary fix to get the Comms Stack to work, this ThreadPool should be managed
/// by the ServiceBundleRunner instead.
pub(crate) fn start() -> SdvResult<Box<dyn AsyncExecutor>> {
    let tp = ThreadPoolBuilder::new()
        .pool_size(2)
        .name_prefix("comms-stack-thread-")
        .create()
        .map_err(derive!(SdvStatusCode::Internal, "Unable to start Comms Stack ThreadPool"))?;

    Ok(Box::new(LocalExecutor { delegate: tp }))
}
