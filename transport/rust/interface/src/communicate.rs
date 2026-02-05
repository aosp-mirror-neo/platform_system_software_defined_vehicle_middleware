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

//! Middleware Factory Traits
use crate::{
    pubsub_traits::{Publication, Publish, Subscribe},
    rpc_traits::{ClientTransport, ServerTransport},
    types::{Availability, ClientAttributes, PublisherAttributes, ServerAttributes},
    ByteBuffersStream, SubscribeOptions, TypeName, UnitQuery,
};
use async_trait::async_trait;
use futures::Stream;
use mockall::automock;
use sdv_comms::rpc::{RpcMethod, UnaryRpcMethod};
use sdv_status_rs::SdvResult;
use std::{collections::HashMap, pin::Pin, sync::Arc};

/// The Communicate trait needs to be implemented by a comms stack adapters to enable
/// ServiceBundles to communicate with each other using pub/sub.
#[async_trait]
#[automock]
pub trait Communicate: Send + Sync {
    /// Create an object implementing Publish with the specified attributes.
    async fn create_publish(&self, attributes: &PublisherAttributes)
        -> SdvResult<Box<dyn Publish>>;

    /// Look for a publication with the attributes specified in UnitQuery.
    #[allow(useless_deprecated)]
    #[allow(unused_attributes)]
    #[deprecated(
        since = "2.0.0",
        note = "Channel-based subscribe methods are called instead, e.g. `create_subscribe_handle`"
    )]
    async fn find_publication(&self, attributes: &UnitQuery) -> SdvResult<Arc<dyn Publication>>;

    /// Create a Subscriber for a message type, published by zero to many publishers.
    #[allow(useless_deprecated)]
    #[allow(unused_attributes)]
    #[deprecated(
        since = "2.0.0",
        note = "Channel-based subscribe methods are called instead, e.g. `create_subscribe_handle`"
    )]
    async fn subscribe_by_type(
        &self,
        type_name: &TypeName,
        options: &SubscribeOptions,
    ) -> SdvResult<Box<dyn Subscribe>>;

    /// Create a Subscriber for a message type on a specific channel, published by zero to many publishers.
    async fn create_subscribe_handle(
        &self,
        type_name: &TypeName,
        topic_name: &str,
        options: &SubscribeOptions,
    ) -> SdvResult<Box<dyn Subscribe>>;

    /// Create a subscriber-side Stream for a message type, published by zero to many publishers.
    #[allow(useless_deprecated)]
    #[allow(unused_attributes)]
    #[deprecated(
        since = "2.0.0",
        note = "Channel-based observe methods are called instead, e.g. `create_message_stream`"
    )]
    async fn observe_by_type(
        &self,
        type_name: &TypeName,
        options: &SubscribeOptions,
    ) -> SdvResult<ByteBuffersStream>;

    /// Create a subscriber-side Stream for a message type on a specific topic, published by zero to many publishers.
    async fn create_message_stream(
        &self,
        type_name: &TypeName,
        topic_name: &str,
        options: &SubscribeOptions,
    ) -> SdvResult<ByteBuffersStream>;

    /// create an object implementing ServerTransport with the specified attributes
    /// only for UnaryRpcMethod
    #[allow(useless_deprecated)]
    #[allow(unused_attributes)]
    #[deprecated(
        since = "1.1.0",
        note = "please use `create_server_transport_all_methods` instead"
    )]
    async fn create_server_transport(
        &self,
        attributes: &ServerAttributes,
        method_map: HashMap<String, UnaryRpcMethod>,
    ) -> SdvResult<Box<dyn ServerTransport>>;

    /// Creates a server-side transport for handling RPC requests.
    ///
    /// This function sets up a transport layer for a server based on the provided
    /// attributes. The server will be capable of handling various types of RPC
    /// methods, including unary, client-streaming, and server-streaming
    ///
    /// # Arguments
    ///
    /// * `attributes` - The [`ServerAttributes`] that define the properties of the
    ///   server, include server_name, type_name, and server options.
    /// * `method_map` - A map from method names to their [`RpcMethod`]
    ///   implementations. This tells the transport how to handle incoming
    ///   requests for each method.
    ///
    /// # Returns
    ///
    /// A `SdvResult` containing a `Box<dyn ServerTransport>` on success, which
    /// represents the active server transport. On failure, it returns an error.
    async fn create_server_transport_all_methods(
        &self,
        attributes: &ServerAttributes,
        method_map: HashMap<String, RpcMethod>,
    ) -> SdvResult<Box<dyn ServerTransport>>;

    /// create an object implementing ClientTransport with the specified attributes
    async fn create_client_transport(
        &self,
        attributes: &ClientAttributes,
    ) -> SdvResult<Box<dyn ClientTransport>>;

    /// creates a stream of registration events
    async fn create_registration_event_stream(
        &self,
        attributes: &UnitQuery,
    ) -> SdvResult<Pin<Box<dyn Stream<Item = Availability> + Send>>>;
}
