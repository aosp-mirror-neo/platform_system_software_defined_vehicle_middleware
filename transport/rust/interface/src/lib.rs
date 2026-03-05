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

//! SDV Middleware Communication APIs

/// User-facing Middleware Pub/Sub APIs
mod pubsub;
pub use pubsub::{observe, History, InstantReader, Publisher, Subscriber};

/// User-facing Middleware RPC APIs
mod rpc;
pub use rpc::{
    call_client_streaming_method, call_method, call_server_streaming_method, MethodMapBuilder,
};

/// Middleware Pub/Sub Traits
mod pubsub_traits;
pub use pubsub_traits::{
    ByteBuffersStream, InstantRead, MockPublish, Publication, Publish, ReadHistory, Subscribe,
};

/// Middleware RPC Traits
mod rpc_traits;
pub use rpc_traits::{ClientTransport, RpcClient, ServerTransport};

/// Middleware Factory Traits
mod communicate;
pub use communicate::{Communicate, MockCommunicate};

/// Middleware Types for Discovery and Registration
mod types;
pub use types::{
    Availability, ClientAttributes, ClientOptions, PublisherAttributes, PublisherMetadata,
    ServerAttributes, ServerOptions, SubscribeOptions, TypeName, UnitName, UnitQuery,
};

/// Middleware Utilities
mod utils;
/// Feel free to replace this with e.g. `heck::to_kebab_case` if additional formatting functions are needed.
/// For now, this is the only one we need, so we keep it in-tree to avoid bloat.
pub(crate) use utils::format_to_kebab_case;

/// Codec Utils
pub mod codec;
