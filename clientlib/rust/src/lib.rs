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
//! SDV Middleware Clientlib

use sdv_mw_rs_comms::TypeName;
use sdv_status_rs::SdvResult;

/// Channel
pub mod channel;
pub use channel::{publisher, subscriber};

/// PubSub
pub mod subscribe;
pub use subscribe::*;
pub mod publish;
pub use publish::*;

/// RPC
pub mod rpc_client;
pub use rpc_client::*;
pub mod rpc_server;
pub use rpc_server::*;

/// Availability
pub mod availability;
pub use availability::*;

/// A trait for vsidlc-generated types (channels and services) to provide their logical type
/// identifier.
///
/// vsidlc generates an implementation for each channel and service interface.
/// # Stability
/// The implementation details of this trait (methods and associated constants) are not part of the public API and are unstable.
/// They are subject to breaking changes or removal in any future version. Service bundle authors must not rely on these internal details.
pub trait ToTypeName {
    #[doc(hidden)]
    fn type_name() -> SdvResult<TypeName>;
}
