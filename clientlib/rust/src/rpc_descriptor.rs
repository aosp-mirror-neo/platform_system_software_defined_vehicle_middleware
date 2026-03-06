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

//! Contains traits related to RPC client channels.
///
/// The traits in this module are implemented by the `vsidlc` code generator
/// for channel types. Service bundle authors interact with types that implement
/// these traits, but should not implement them manually.
pub mod client {
    use crate::ToTypeName;
    use std::fmt::Debug;

    /// A descriptor that binds a specific RPC client type to a communication channel.
    ///
    /// This trait carries the channel identifier required to connect to the service and
    /// enforces type safety by associating the specific `ClientType` stub with that channel.
    pub trait Descriptor: ToTypeName + Debug {
        /// The client type associated with this descriptor.
        type ClientType: sdv_mw_rs_comms::RpcClient;

        #[doc(hidden)]
        fn get_channel(&self) -> &str;
    }
}
