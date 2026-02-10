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

use google_sdv_data_tunnel::aidl::google::sdv::data_tunnel::{
    IAgentService::ERROR_ACCESS_DENIED,
    ISubscriptionReadinessListener::{
        BnSubscriptionReadinessListener, ISubscriptionReadinessListener,
    },
    SubscriptionParams::SubscriptionParams,
    SubscriptionResult::SubscriptionResult,
};
use google_sdv_identity::binder::{BinderFeatures, Interface, Result as BinderResult, Strong};
use sdv_comms_common::callback::ffi::Callback;
use sdv_comms_ctx::ContextImpl;
use sdv_comms_id::names::TopicName;
use sdv_comms_id::UnitIdentifier;
use sdv_comms_sd::discovery::UnitType;
use sdv_comms_sd::metadata::PublisherUnitMetadata;
use sdv_fmq_rust::FmqWrapperRust;
use sdv_status_rs::{derive, SdvResult, SdvStatus, SdvStatusCode};

use crate::core::multiplexer_internal::MultiplexerInternal;
use crate::core::shared_internal;
use crate::ffi::ffi_types;
use futures::channel::oneshot;
use log::error;
use std::cmp::min;
use std::option::Option;
use std::sync::Mutex;

struct SubscriptionReadinessListener {
    sender: Mutex<Option<oneshot::Sender<i32>>>,
}

impl SubscriptionReadinessListener {
    fn create() -> (Strong<dyn ISubscriptionReadinessListener>, oneshot::Receiver<i32>) {
        let (sender, receiver) = oneshot::channel();
        let listener = BnSubscriptionReadinessListener::new_binder(
            SubscriptionReadinessListener { sender: Mutex::new(Some(sender)) },
            BinderFeatures::default(),
        );
        (listener, receiver)
    }
}

impl Interface for SubscriptionReadinessListener {}

#[allow(non_snake_case)] // AIDL generated
impl ISubscriptionReadinessListener for SubscriptionReadinessListener {
    fn onSubscriptionReady(&self, status_code: i32) -> BinderResult<()> {
        if let Some(sender) = self.sender.lock().unwrap().take() {
            let _ = sender.send(status_code);
        } else {
            error!("Got multiple onSubscriptionReady notifications!");
        }
        Ok(())
    }
}

pub struct RegularSubscriber {
    ctx: &'static ContextImpl,
    fmq: FmqWrapperRust,
    publication_id: i32,
    callback_id: Option<i32>,
    metadata: PublisherUnitMetadata,
}

impl Drop for RegularSubscriber {
    fn drop(&mut self) {
        if let Err(e) = self.clear_notification_handler() {
            error!("Unable to clear notification handler: {e}");
        }
    }
}

impl RegularSubscriber {
    pub async fn subscribe_to_publication(
        ctx: &'static ContextImpl,
        unit_identifier: &UnitIdentifier,
        // subscription_flags will be used when Read-on-Subscriber feature is enabled
        subscription_flags: u64,
    ) -> SdvResult<Self> {
        let dt_binder = ctx.get_agents().get_dt_binder();
        super::state::get_state(ctx)?;

        // We only set up a readiness listener for remote topics; local topics are always ready,
        // and we want to avoid a possible increase in startup time due to waiting for a readiness
        // notification we don't need in that case.
        let publisher_is_remote = ctx.get_identity().fqin().vm_name()
            != unit_identifier.service_identity().fqin().vm_name();
        let (opt_readiness_listener, opt_readiness_result) = if publisher_is_remote {
            let (listener, result) = SubscriptionReadinessListener::create();
            (Some(listener), Some(result))
        } else {
            (None, None)
        };

        let mut sub_result: SubscriptionResult = Default::default();
        dt_binder
            .SubscribeToPublicationExtended(
                &SubscriptionParams {
                    unit_identifier: unit_identifier.into(),
                    readiness_listener: opt_readiness_listener,
                },
                &mut sub_result,
            )
            .map_err(|err| {
                if err.service_specific_error() == ERROR_ACCESS_DENIED {
                    SdvStatus::with_cause(SdvStatusCode::PermissionDenied, err)
                } else {
                    SdvStatus::with_cause(SdvStatusCode::Unknown, err)
                }
            })?;

        // Wait for subscription to be ready for correct fetch_last_message behavior.
        if let Some(readiness_result) = opt_readiness_result {
            let status_code =
                readiness_result.await.map_err(|_| SdvStatus::new(SdvStatusCode::Unavailable))?;
            SdvResult::from(SdvStatus::new(SdvStatusCode::from(status_code)))?;
        }

        let publication_descriptor = &sub_result.publication_descriptor;

        let mut metadata = PublisherUnitMetadata::default();
        metadata.set_version(1);
        metadata.set_message_size(publication_descriptor.message_size as u32);
        metadata.set_message_count(publication_descriptor.message_count as u32);

        let mut fmq = FmqWrapperRust::from_desc(&publication_descriptor.mq_desc, false);
        // Move the reader pointer according to the subscription_flags
        shared_internal::adjust_read_pointer(
            &mut fmq,
            publication_descriptor.message_size as usize,
            publication_descriptor.message_count as usize,
            subscription_flags,
        )?;

        Ok(RegularSubscriber {
            ctx,
            fmq,
            publication_id: publication_descriptor.publication_id,
            callback_id: None,
            metadata,
        })
    }

    pub fn available_to_read(&self) -> SdvResult<usize> {
        Ok(self.fmq.available_to_read() / self.metadata.get_message_size() as usize)
    }

    pub fn read(&mut self, messages: &mut [i8], num_messages: usize) -> SdvResult<usize> {
        let available_messages = self.available_to_read()?;
        let messages_to_read = min(available_messages, num_messages);
        let num_bytes = messages_to_read * self.metadata.get_message_size() as usize;
        if self.fmq.read_from_buffer(messages, num_bytes) {
            Ok(messages_to_read)
        } else {
            // We calculated num_bytes <= available, so a failure here must be an overflow.
            Err(SdvStatus::with_message(
                SdvStatusCode::DataLoss,
                "Data loss due to FMQ buffer overflow",
            ))
        }
    }

    pub fn set_notification_closure<
        C: FnMut(ffi_types::ASDV_NotificationType) -> SdvResult<()> + 'static + Send,
    >(
        &mut self,
        closure: C,
        min_interval_ms: u32,
    ) -> SdvResult<()> {
        let dt_state = super::state::get_state(self.ctx).map_err(|e| {
            Err(SdvStatus::with_message(
                SdvStatusCode::Internal,
                format!("Unable to get internal DT state from the context object: {e}"),
            ))
        })?;

        let mut comm_channel = dt_state.comm_channel.write().map_err(|e| {
            Err(SdvStatus::with_message(
                SdvStatusCode::Internal,
                format!("Unable to lock internal comm channel: {e}"),
            ))
        })?;
        // Register a callback, replacing any existing one we have registered for this Subscriber.
        self.callback_id = Some(comm_channel.register_or_replace_closure(
            self.publication_id,
            self.callback_id.take(),
            closure,
            min_interval_ms,
        )?);
        Ok(())
    }

    pub(crate) fn clear_notification_handler(&mut self) -> SdvResult<()> {
        if let Some(callback_id) = self.callback_id.take() {
            let dt_state = super::state::get_state(self.ctx).map_err(derive!(
                SdvStatusCode::Internal,
                "Unable to get internal DT state from the context object"
            ))?;

            let mut comm_channel = dt_state.comm_channel.write().map_err(|e| {
                Err(SdvStatus::with_message(
                    SdvStatusCode::Internal,
                    format!("Unable to lock internal comm channel: {e}"),
                ))
            })?;
            comm_channel.unregister_callback(self.publication_id, callback_id)?;
        }
        Ok(())
    }

    pub fn get_message_size(&self) -> u32 {
        self.metadata.get_message_size()
    }
}

pub enum SubscriberKind {
    Regular(RegularSubscriber),
    Multiplexer(MultiplexerInternal),
}

pub struct SubscriberInternal {
    delegate: SubscriberKind,
}

impl SubscriberInternal {
    pub async fn subscribe_to_publication(
        ctx: &'static ContextImpl,
        unit_identifier: &UnitIdentifier,
        // subscription_flags will be used when Read-on-Subscriber feature is enabled
        subscription_flags: u64,
    ) -> SdvResult<SubscriberInternal> {
        let regular =
            RegularSubscriber::subscribe_to_publication(ctx, unit_identifier, subscription_flags)
                .await?;
        Ok(SubscriberInternal { delegate: SubscriberKind::Regular(regular) })
    }

    pub async fn subscribe_to_unit_type(
        ctx: &'static ContextImpl,
        unit_type: &UnitType,
        subscription_flags: u64,
    ) -> SdvResult<SubscriberInternal> {
        let multiplexer =
            MultiplexerInternal::subscribe_to_unit_type(ctx, unit_type, subscription_flags).await?;
        Ok(SubscriberInternal { delegate: SubscriberKind::Multiplexer(multiplexer) })
    }

    pub async fn subscribe_to_topic(
        ctx: &'static ContextImpl,
        unit_type: &UnitType,
        topic_name: &TopicName,
        subscription_flags: u64,
    ) -> SdvResult<SubscriberInternal> {
        let multiplexer =
            MultiplexerInternal::subscribe_to_topic(ctx, unit_type, topic_name, subscription_flags)
                .await?;
        Ok(SubscriberInternal { delegate: SubscriberKind::Multiplexer(multiplexer) })
    }

    pub fn get_message_size(&self) -> u32 {
        match &self.delegate {
            SubscriberKind::Regular(regular) => regular.metadata.get_message_size(),
            SubscriberKind::Multiplexer(multiplexer) => multiplexer.get_message_size(),
        }
    }

    pub fn get_message_count(&self) -> u32 {
        match &self.delegate {
            SubscriberKind::Regular(regular) => regular.metadata.get_message_count(),
            SubscriberKind::Multiplexer(multiplexer) => multiplexer.get_message_count(),
        }
    }

    pub fn get_publisher_unit_metadata(&self) -> &PublisherUnitMetadata {
        match &self.delegate {
            SubscriberKind::Regular(regular) => &regular.metadata,
            SubscriberKind::Multiplexer(multiplexer) => multiplexer.get_publisher_unit_metadata(),
        }
    }

    pub fn available_to_read(&self) -> SdvResult<usize> {
        match &self.delegate {
            SubscriberKind::Regular(regular) => regular.available_to_read(),
            SubscriberKind::Multiplexer(multiplexer) => multiplexer.available_to_read(),
        }
    }

    pub fn read(&mut self, messages: &mut [i8], num_messages: usize) -> SdvResult<usize> {
        let message_size = self.get_message_size();
        assert!(messages.len() >= (num_messages * message_size as usize));
        match &mut self.delegate {
            SubscriberKind::Regular(ref mut regular) => regular.read(messages, num_messages),
            SubscriberKind::Multiplexer(ref mut multiplexer) => {
                multiplexer.read(messages, num_messages)
            }
        }
    }

    // Set notification handler callback function
    pub fn set_notification_handler(
        &mut self,
        callback: Callback<ffi_types::ASDV_NotificationType>,
        min_interval_ms: u32,
    ) -> SdvResult<()> {
        match &mut self.delegate {
            SubscriberKind::Regular(regular) => {
                regular.set_notification_closure(move |ev| callback.invoke(ev), min_interval_ms)
            }
            SubscriberKind::Multiplexer(multiplexer) => {
                multiplexer.set_notification_handler(callback, min_interval_ms)
            }
        }
    }

    pub fn clear_notification_handler(&mut self) -> SdvResult<()> {
        match &mut self.delegate {
            SubscriberKind::Regular(regular) => regular.clear_notification_handler(),
            SubscriberKind::Multiplexer(multiplexer) => multiplexer.clear_notification_handler(),
        }
    }

    #[cfg(test)]
    pub(crate) fn get_multiplexer(&self) -> &MultiplexerInternal {
        match &self.delegate {
            SubscriberKind::Regular(_) => {
                panic!("Called get_multiplexer on regular subscriber!")
            }
            SubscriberKind::Multiplexer(multiplexer) => multiplexer,
        }
    }
}
