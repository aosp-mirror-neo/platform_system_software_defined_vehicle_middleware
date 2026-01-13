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

#![allow(unused)]

//! SDV Comms Stack SDK Context API declration and implementation.

mod internal;
pub use internal::initialise;
pub use internal::ContextImpl;

mod agents;
pub use agents::wait_for_agents;
mod executor;

/// Module for Context FFI types and bindings.
pub mod ffi;
