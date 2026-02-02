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

use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::{
    IServiceUnitChangeListener::BnServiceUnitChangeListener,
    IServiceUnitChangeListener::IServiceUnitChangeListener,
    ServiceUnitDefinition::ServiceUnitDefinition as ServiceUnitDefinitionAidl,
};
use log::error;

use sdv_comms_common::callback::ffi::Callback;
use sdv_comms_ctx::ContextImpl;
use sdv_comms_id::names::TopicName;
use sdv_comms_id::UnitIdentifier;
use sdv_comms_sd::discovery::{ServiceUnitDefinition, UnitType};
use sdv_comms_sd::manager as sd_manager;
use sdv_comms_sd::metadata::{PublisherUnitMetadata, UnitMetadata};
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::sync::{Arc, Mutex};

use crate::core::subscriber_internal::RegularSubscriber;
use crate::ffi::ffi_types;

type NotificationCallback = Callback<ffi_types::ASDV_NotificationType>;

pub struct MultiplexerInternal {
    ctx: &'static ContextImpl,
    unit_type: UnitType,
    metadata: PublisherUnitMetadata,
    topic_name: Option<TopicName>,
    multiplexer_impl: Arc<Mutex<MultiplexerImpl>>,
    listener: binder::Strong<dyn IServiceUnitChangeListener>,
}

impl MultiplexerInternal {
    pub async fn subscribe_to_unit_type(
        ctx: &'static ContextImpl,
        unit_type: &UnitType,
        subscription_flags: u64,
    ) -> SdvResult<MultiplexerInternal> {
        // List all the publication with given UnitType
        let sud_list = sd_manager::list_service_units_by_type(ctx, unit_type).await?;
        // Return Error if no publication was found
        // TODO(b/365408445): Fix this workaround when UnitType metadata is introduced.
        if sud_list.is_empty() {
            return Err(SdvStatus::with_message(
                SdvStatusCode::NotFound,
                "No services registered with the specified UnitType were found.",
            ));
        }

        // Retrieve publisher metadata for the first ServiceUnitDefinition
        let metadata =
            sud_list[0].get_unit_metadata().get_publisher_metadata().ok_or_else(|| {
                SdvStatus::with_message(
                    SdvStatusCode::InvalidArgument,
                    format!("Unit type {} is not a publication unit type.", unit_type),
                )
            })?;

        let multiplexer_impl = MultiplexerImpl::create_shared(&metadata);

        let listener = PublisherListener::create(ctx, subscription_flags, multiplexer_impl.clone());

        // Register listener with SD Agent.
        let sd_agent = ctx.get_agents().get_sd_binder();
        sd_agent
            .registerServiceUnitChangeListenerByType(&unit_type.clone().into(), &listener)
            .map_err(SdvStatus::from)?;

        Ok(MultiplexerInternal {
            ctx,
            unit_type: unit_type.clone(),
            topic_name: None,
            metadata,
            multiplexer_impl,
            listener,
        })
    }

    pub async fn subscribe_to_topic(
        ctx: &'static ContextImpl,
        unit_type: &UnitType,
        topic_name: &TopicName,
        subscription_flags: u64,
    ) -> SdvResult<MultiplexerInternal> {
        // List all the publication with given UnitType
        let sud_list = sd_manager::list_service_units_by_type(ctx, unit_type).await?;

        // Filter publication by topic_name
        let mut metadata = PublisherUnitMetadata::default();
        let mut found_pub_for_topic = false;
        for sud in sud_list {
            if let Some(pub_metadata) = sud.get_unit_metadata().get_publisher_metadata() {
                if pub_metadata.get_topic_name() == topic_name.as_ref() {
                    if !found_pub_for_topic {
                        metadata = pub_metadata;
                        found_pub_for_topic = true;
                    } else {
                        // TODO (b/476033721)
                        // Several Service Units may publish the same Proto-message type on this
                        // topic, but using different versions of the schema. Because Protobuf
                        // is backward compatible, newer versions may include additional fields
                        // (e.g., extensions or new optional fields) that increase the total
                        // serialized size.
                        //
                        // This is not supported at the moment and Multiplexer will return an error
                        // if multiple message size are detected
                        if metadata.get_message_size() != pub_metadata.get_message_size() {
                            return Err(SdvStatus::with_message(
                                SdvStatusCode::InvalidArgument,
                                format!(
                                    "Incompatible message sizes on topic {}: {} != {}",
                                    topic_name,
                                    metadata.get_message_size(),
                                    pub_metadata.get_message_size()
                                ),
                            ));
                        }
                    }
                }
            }
        }

        // Return Error if no publication was found
        // TODO(b/365408445): Fix this workaround when UnitType metadata is introduced.
        if !found_pub_for_topic {
            return Err(SdvStatus::with_message(
                SdvStatusCode::NotFound,
                "No services registered with the specified UnitType were found.",
            ));
        }

        let multiplexer_impl = MultiplexerImpl::create_shared(&metadata);

        let listener = PublisherListener::create(ctx, subscription_flags, multiplexer_impl.clone());

        // Register listener with SD Agent.
        let sd_agent = ctx.get_agents().get_sd_binder();
        sd_agent
            .registerServiceUnitChangeListenerByTopic(
                &unit_type.clone().into(),
                topic_name.as_ref(),
                &listener,
            )
            .map_err(SdvStatus::from)?;

        Ok(MultiplexerInternal {
            ctx,
            unit_type: unit_type.clone(),
            topic_name: Some(topic_name.clone()),
            metadata,
            multiplexer_impl,
            listener,
        })
    }

    pub fn get_message_size(&self) -> u32 {
        self.metadata.get_message_size()
    }

    pub fn get_message_count(&self) -> u32 {
        self.metadata.get_message_count()
    }

    pub fn available_to_read(&self) -> SdvResult<usize> {
        self.multiplexer_impl.lock().unwrap().available_to_read()
    }

    pub fn read(&mut self, messages: &mut [i8], num_messages: usize) -> SdvResult<usize> {
        self.multiplexer_impl.lock().unwrap().read(messages, num_messages)
    }

    pub fn set_notification_handler(
        &mut self,
        callback: Callback<ffi_types::ASDV_NotificationType>,
        min_interval_ms: u32,
    ) -> SdvResult<()> {
        self.multiplexer_impl.lock().unwrap().set_notification_handler(callback, min_interval_ms)
    }

    pub fn clear_notification_handler(&mut self) -> SdvResult<()> {
        self.multiplexer_impl.lock().unwrap().clear_notification_handler()
    }

    pub fn get_publisher_unit_metadata(&self) -> &PublisherUnitMetadata {
        &self.metadata
    }

    // Meant for use in tests.
    #[allow(dead_code)]
    pub(crate) fn get_publication_count(&self) -> usize {
        self.multiplexer_impl.lock().unwrap().subscriptions.len()
    }
}

impl Drop for MultiplexerInternal {
    fn drop(&mut self) {
        // Immediately disable further notifications to the application.
        let _ = self.clear_notification_handler();

        // Clean up SD unit change listener; since this drop handler is invoked in an application
        // thread via ASDV_UnsubscribeToPublication we don't want to block, so spawn a task for
        // the blocking call to SD Agent.
        let ctx = self.ctx;
        let listener = self.listener.clone();
        let unit_type = self.unit_type.clone();
        let topic_name = self.topic_name.clone();
        ctx.get_executor().spawn(Box::pin(async move {
            let sd_agent = ctx.get_agents().get_sd_binder();
            match topic_name {
                Some(topic_name) => {
                    if let Err(err) = sd_agent.unregisterServiceUnitChangeListenerByTopic(
                        &unit_type.into(),
                        topic_name.as_ref(),
                        &listener,
                    ) {
                        error!("Error unregistering multipub ServiceUnitChangeListener: {err}");
                    }
                }
                None => {
                    if let Err(err) = sd_agent
                        .unregisterServiceUnitChangeListenerByType(&unit_type.into(), &listener)
                    {
                        error!("Error unregistering multipub ServiceUnitChangeListener: {err}");
                    }
                }
            }
        }));
    }
}

// PublisherListener implements IServiceUnitChangeListener and is used to listen for
// reg / unreg events for publisher units matching the multipub's unit type, and to
// update the set of subscribers maintained by the multipub in response to those events.
struct PublisherListener {
    ctx: &'static ContextImpl,
    subscription_flags: u64,
    multiplexer_impl: Arc<Mutex<MultiplexerImpl>>,
}

impl PublisherListener {
    fn create(
        ctx: &'static ContextImpl,
        subscription_flags: u64,
        multiplexer_impl: Arc<Mutex<MultiplexerImpl>>,
    ) -> binder::Strong<dyn IServiceUnitChangeListener> {
        BnServiceUnitChangeListener::new_binder(
            PublisherListener { ctx, subscription_flags, multiplexer_impl },
            binder::BinderFeatures::default(),
        )
    }

    fn extract_unit_id(
        unit_definition: &ServiceUnitDefinitionAidl,
    ) -> binder::Result<UnitIdentifier> {
        let sud = ServiceUnitDefinition::try_from(unit_definition.clone()).map_err(|err| {
            error!("PublisherListener received invalid ServiceUnitDefinition");
            binder::Status::new_exception_str(
                binder::ExceptionCode::ILLEGAL_ARGUMENT,
                Some(format!("Invalid ServiceUnitDefinition: {}", err)),
            )
        })?;
        Ok(sud.get_unit_identifier())
    }
}

impl binder::Interface for PublisherListener {}

impl IServiceUnitChangeListener for PublisherListener {
    fn onServiceUnitRegistered(
        &self,
        unit_definition: &ServiceUnitDefinitionAidl,
    ) -> binder::Result<()> {
        let unit_id = Self::extract_unit_id(unit_definition)?;

        // TODO (b/476033721)
        // Handle publishers with different message sizes. For now, we will reject
        // Subscriptions to publishers with different message sizes.
        if let Ok(unit_metadata) = UnitMetadata::try_from(unit_definition.appMetadata.clone()) {
            if let Some(pub_metadata) = unit_metadata.get_publisher_metadata() {
                if pub_metadata.get_message_size()
                    > self.multiplexer_impl.lock().unwrap().message_size
                {
                    error!(
                        "Error adding publisher {:?} with different messages than multiplexer: {} != {}",
                        unit_id,
                        pub_metadata.get_message_size(),
                        self.multiplexer_impl.lock().unwrap().message_size
                    );
                    return Ok(());
                }
            }
        }

        // Spawn an async task to set up the subscription so that we don't tie up the binder
        // thread with the blocking call to DT Agent.
        let ctx = self.ctx;
        let flags = self.subscription_flags;
        let multiplexer_impl = self.multiplexer_impl.clone();
        self.ctx.get_executor().spawn(Box::pin(async move {
            match RegularSubscriber::subscribe_to_publication(ctx, &unit_id, flags).await {
                Ok(subscriber) => {
                    if let Err(err) =
                        multiplexer_impl.lock().unwrap().add_subscription(unit_id, subscriber)
                    {
                        error!("Error adding multipub contributing subscription: {}", err);
                    }
                }
                Err(err) => {
                    error!(
                        "Error creating new contributing subscription to add to multipub: {}",
                        err
                    );
                }
            };
        }));

        Ok(())
    }

    fn onServiceUnitUnregistered(
        &self,
        unit_definition: &ServiceUnitDefinitionAidl,
    ) -> binder::Result<()> {
        let unit_id = Self::extract_unit_id(unit_definition)?;

        if let Err(err) = self.multiplexer_impl.lock().unwrap().remove_subscription(unit_id) {
            error!("Error removing multiplexer contributing subscription: {}", err);
        }

        Ok(())
    }

    fn onServiceUnitChanged(&self, _unit: &ServiceUnitDefinitionAidl) -> binder::Result<()> {
        error!("Got unexpected onServiceUnitChanged!");
        Ok(())
    }
}

// If we get an notice from Service Discovery that a publication has been unregistered, but
// there is still data in the FMQ that hasn't been read, we need to keep our underlying
// subscriber for that publication around until all of the data is read.
//
// This is partly to ensure there's no hidden data loss and multipub subscribers behave
// like single-pub subscribers (where it's always possible to read the final messages of
// a topic even after the publisher has died), and partly to avoid situations where
// read_messages() is unable to read as many messages as available_to_read() had previously
// reported as available because a contributing subscription was removed between the two calls.
//
// This enum defines the different lifetime states for a Subscription to enable proper cleanup.
#[derive(PartialEq)]
enum SubscriptionState {
    // Normal publisher-in-operation state.
    Active,

    // Publication has been unregistered but we haven't read all of the data from its FMQ
    // and need to hold on to it until we have.
    Unregistered,

    // Publication has been unregistered and all data read; subscription is ready for removal.
    // This is a transient state that is only used inside of MultiplexerImpl::read().
    // It's included here to avoid needing a separate heap-allocated structure in that
    // function to keep track of which subscriptions to remove following a read operation.
    Exhausted,
}

// Represents one of a multipub subscriber's underlying subscriptions to a single publication.
struct Subscription {
    unit_id: UnitIdentifier,
    subscriber: RegularSubscriber,
    state: SubscriptionState,
}

// MultiplexerImpl stores state that needs to be read and written both from MultiplexerInternal
// and PublisherListener, and provides most of the implementation of the methods on those types.
//
// No synchronization is built into this struct itself; it's meant to be stored in
// an Arc<Mutex<>> so that all calls to it are synchronized.
//
// Lock contention shouldn't be a problem here: Publication registration / deregistration are
// infrequent operations, so most of the time calls to available_to_read() and read_message()
// will not have to wait to get the lock.
struct MultiplexerImpl {
    // All current subscriptions to publications matching the multipub unit type.
    subscriptions: Vec<Subscription>,

    // read() round-robins across subscriptions.  If we always started from the 0th index in the
    // subscriptions array, we may end up consistently under-polling subscribers towards the end.
    //
    // This may happen even for a client that is calling available_to_read() and then calling
    // read_messages() for all of the data reported available, since new data can arrive between
    // those calls.
    //
    // Storing a next_read_index across calls to read() and always picking up where we left off
    // should ensure fairness.
    next_read_index: usize,

    // App-provided callback to fire when we get a notification for any publication.
    // This is wrapped in a mutex to serialize callback invocations.  We store this here
    // so that if we get a registration event for a new subscription we can set up its callback.
    notification_callback: Option<Arc<Mutex<NotificationCallback>>>,

    // Min interval between notifications; we don't actually respect this, because the interval
    // is applied separately to each underlying publication.
    notification_min_interval_ms: u32,

    // Needed by read().
    message_size: u32,
}

impl MultiplexerImpl {
    fn create_shared(metadata: &PublisherUnitMetadata) -> Arc<Mutex<MultiplexerImpl>> {
        Arc::new(Mutex::new(MultiplexerImpl {
            subscriptions: Vec::new(),
            next_read_index: 0,
            notification_callback: None,
            notification_min_interval_ms: 0,
            // TODO (b/476033721)
            // Message size is initialized with the original biggest size founded across existing
            // publishers MultiplexerImpl creation time. This will cause reading issues if after
            // the initialization a new publisher is created with a newer version of the schema,
            // which requires bigger messages.
            message_size: metadata.get_message_size(),
        }))
    }

    fn available_to_read(&self) -> SdvResult<usize> {
        self.subscriptions.iter().map(|contrib| contrib.subscriber.available_to_read()).sum()
    }

    // This method may modify buffer even when returning an error; it cannot reliably detect
    // all error conditions before beginning to write to the buffer.
    fn read(&mut self, mut buffer: &mut [i8], num_messages: usize) -> SdvResult<usize> {
        if num_messages == 0 {
            return Ok(0);
        }

        let subscription_count = self.subscriptions.len();
        if subscription_count == 0 {
            return Ok(0);
        }

        let bytes_to_read =
            num_messages.checked_mul(self.message_size as usize).ok_or_else(|| {
                SdvStatus::with_message(
                    SdvStatusCode::InvalidArgument,
                    "Read size would overflow representable buffer size.",
                )
            })?;
        assert!(buffer.len() >= bytes_to_read);

        let mut remove_exhausted_subscriptions = false;
        let mut remaining_messages_to_read = num_messages;
        for _ in 0..subscription_count {
            // next_read_index could have wrapped around or could have been made out-of-bound
            // by a subscription removal since the last call to read_messages().  This handles both.
            self.next_read_index %= subscription_count;
            let subscription = self.subscriptions.get_mut(self.next_read_index).unwrap();

            // The only error read can return is DataLoss, which we just propagate.  This also
            // means that any data from other subscriptions that has already been read into buffer
            // will be lost as well since we can't return a count of read messages with the error.
            let current_read_count =
                subscription.subscriber.read(buffer, remaining_messages_to_read)?;

            // If we didn't fill the buffer then we must have read everything available in the FMQ,
            // so if this subscriber was already unregistered it's now exhausted and can be removed.
            if subscription.state == SubscriptionState::Unregistered
                && (current_read_count < remaining_messages_to_read
                    || subscription.subscriber.available_to_read().unwrap_or(0) == 0)
            {
                subscription.state = SubscriptionState::Exhausted;
                remove_exhausted_subscriptions = true;
            }

            // Update buffer and remaining_messages_to_read.
            buffer = &mut buffer[(current_read_count * self.message_size as usize)..];
            remaining_messages_to_read -= current_read_count;
            if remaining_messages_to_read == 0 {
                break;
            }

            // If we reach this point then we must have read all currently available from the
            // current subscription and can move on to the next.
            self.next_read_index += 1;
        }

        // Remove any subscriptions that were exhausted by this read.
        if remove_exhausted_subscriptions {
            self.subscriptions.retain(|e| e.state != SubscriptionState::Exhausted);
        }

        Ok(num_messages - remaining_messages_to_read)
    }

    fn make_notification_closure(
        callback: Arc<Mutex<NotificationCallback>>,
    ) -> impl FnMut(ffi_types::ASDV_NotificationType) -> SdvResult<()> + 'static + Send {
        move |ev| {
            // Only forward DataAvailable notifications to the app.
            // An exception in one of the subscriptions doesn't mean the
            // multipub subscriber as a whole has an issue.
            if ev == ffi_types::ASDV_NotificationType::DataAvailable {
                callback.lock().unwrap().invoke(ev)
            } else {
                Ok(())
            }
        }
    }

    fn add_subscription(
        &mut self,
        unit_id: UnitIdentifier,
        mut subscriber: RegularSubscriber,
    ) -> SdvResult<()> {
        if self.subscriptions.iter().any(|e| e.unit_id == unit_id) {
            return Err(SdvStatus::with_message(
                SdvStatusCode::AlreadyExists,
                format!("Duplicate registration event for {:?}", unit_id),
            ));
        }

        // If there's a callback for this multiplexer, set it up on the new subscriber.
        if let Some(callback) = &self.notification_callback {
            subscriber.set_notification_closure(
                Self::make_notification_closure(callback.clone()),
                self.notification_min_interval_ms,
            )?;
        };

        self.subscriptions.push(Subscription {
            unit_id,
            subscriber,
            state: SubscriptionState::Active,
        });

        Ok(())
    }

    fn remove_subscription(&mut self, unit_id: UnitIdentifier) -> SdvResult<()> {
        let idx =
            self.subscriptions.iter().position(|me| me.unit_id == unit_id).ok_or_else(|| {
                Err(SdvStatus::with_message(
                    SdvStatusCode::NotFound,
                    format!("Missing subscription for unregistered unit {:?}", unit_id),
                ))
            })?;

        // Check if there's any unread data to determine whether we still need the subscription record.
        let available = self.subscriptions[idx].subscriber.available_to_read().unwrap_or(0);
        if available > 0 {
            self.subscriptions[idx].state = SubscriptionState::Unregistered;
        } else {
            self.subscriptions.swap_remove(idx);
        }

        Ok(())
    }

    pub fn set_notification_handler(
        &mut self,
        callback: Callback<ffi_types::ASDV_NotificationType>,
        min_interval_ms: u32,
    ) -> SdvResult<()> {
        let notification_callback = Arc::new(Mutex::new(callback));
        for subscription in self.subscriptions.iter_mut() {
            subscription.subscriber.set_notification_closure(
                Self::make_notification_closure(notification_callback.clone()),
                min_interval_ms,
            )?;
        }
        self.notification_callback = Some(notification_callback);
        self.notification_min_interval_ms = min_interval_ms;
        Ok(())
    }

    pub fn clear_notification_handler(&mut self) -> SdvResult<()> {
        for sub in self.subscriptions.iter_mut() {
            sub.subscriber.clear_notification_handler()?;
        }
        self.notification_callback = None;
        Ok(())
    }
}
