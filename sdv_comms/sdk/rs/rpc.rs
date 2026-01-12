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

//! Module that defines the SDK API for RPC and IPC.

/// SDV RPC Client APIs
pub mod client;

/// SDV RPC Server APIs
mod server;

mod codec;
pub use client::{internal, Client, ClientOptions};
pub use server::{
    ClientStreamingRpcMethod, RpcMethod, Server, ServerOptions, ServerStreamingRpcMethod,
    UnaryRpcMethod,
};

#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod testing;

mod certificate_provider;
mod streams;
mod tls;
mod util;
