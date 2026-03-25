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

use futures::channel::mpsc;
use futures::stream::Stream;
use pin_project::pin_project;
use sdv_comms_bindgen::{ASDV_CallbackVTable_SetNotificationHandler, ASDV_NotificationType};
use sdv_comms_common::callback::sdk::CallbackWrapper;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use std::vec::Vec;

use crate::dt::{Subscriber, SubscriberOptions};
use crate::id::UnitIdentifier;
use crate::sd::{PublisherUnitMetadata, UnitType};
use crate::ContextRef;

/// Data Tunnel Subscriber Stream
/// This class is intended for asynchronous reading messages from a subscription
/// using [futures::Stream] interface.
/// In SubscriberStream, instead of notification callbacks, we provide an iterable
/// stream for asynchronous message reception. While you can iterate using for_each
/// with a closure to process each available message, the messages are wrapped in packets.
/// Each packet contains one or more messages along with the message retrieval result.
/// The packet rate is dependent on the minimum time interval value. For example, if
/// messages are published every 200ms, but your subscriber stream has a 1000ms minimum
/// interval, you'll receive batches of 5 messages every second. This is an under-the-hood
/// optimization to improve performance.
#[pin_project]
pub struct SubscriberStream {
    subscriber: Subscriber,
    #[pin]
    receiver: mpsc::Receiver<ASDV_NotificationType>,
    metadata: PublisherUnitMetadata,
}

static_assertions::assert_impl_all!(SubscriberStream: Send);
static_assertions::assert_not_impl_any!(SubscriberStream: Sync);

/// Implementing futures::Stream on top of Data Tunnel Notifications with time intervals
/// Every poll_next returns a buffer to the next available message.
impl Stream for SubscriberStream {
    type Item = SdvResult<(u32, Vec<u8>)>;

    fn poll_next(
        mut self: Pin<&mut SubscriberStream>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        loop {
            match self.as_mut().project().receiver.poll_next(_cx) {
                Poll::Ready(Some(sdv_comms_bindgen::ASDV_NotificationType_DataAvailable)) => {
                    match self.read_all_available_messages() {
                        Ok((0, _)) => continue, // Poll again rather than produce empty result
                        other => break Poll::Ready(Some(other)),
                    }
                }
                Poll::Ready(Some(_other_notification_type)) => {
                    break Poll::Ready(Some(Err(SdvStatus::new(SdvStatusCode::Internal))))
                }
                Poll::Ready(None) => break Poll::Ready(None), // Stream ended
                Poll::Pending => break Poll::Pending,
            }
        }
    }
}

impl SubscriberStream {
    /// Non blocking.
    /// Subscribes to a publication specified in unit_identifier and creates
    /// a DataTunnel subscription. Creating more than one Subscriber instance for
    /// the same UnitIdentifier will create independent objects with separate
    /// subscription related resources.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_identifier: Unit Identifier of the publication to subscribe
    ///    [IN] min_interval: Specifies the minimal interval between nearing
    ///                       callbacks for internal notifications implementation.
    ///                       Using Duration::ZERO disables minimal interval.
    ///                       See SubscriberStream description for details.
    ///    [IN] options: see SubscriberOptions for details
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains SdvStatus of the
    ///   method call and, if method succeeded, a new Subscriber instance.
    #[allow(useless_deprecated)]
    #[allow(unused_attributes)]
    #[deprecated(since = "2.0.0", note = "Use SubscriberStream.create_for_topic instead")]
    pub async fn create(
        context: ContextRef,
        unit_identifier: &UnitIdentifier,
        min_interval: Duration,
        options: SubscriberOptions,
    ) -> SdvResult<SubscriberStream> {
        let subscriber = Subscriber::create(context, unit_identifier, options).await?;
        type SetNotificationsCb = CallbackWrapper<ASDV_NotificationType>;
        let (cb, receiver) = SetNotificationsCb::new_async();
        let vtable = ASDV_CallbackVTable_SetNotificationHandler {
            func: Some(SetNotificationsCb::trampoline_func),
            clear: Some(SetNotificationsCb::trampoline_clear),
            cb_data: cb.into_void_ptr(),
        };

        let min_interval_u32 = min_interval.as_millis().try_into().unwrap_or(u32::MAX);

        // SAFETY: subscriber.0 is a valid pointer to [ASDV_SubscriptionHandle].
        let status = unsafe {
            sdv_comms_bindgen::ASDV_SetNotificationHandler(
                subscriber.0.as_ptr(),
                vtable,
                min_interval_u32,
            )
        };
        // Propagate error if setting the handler failed
        SdvResult::from(SdvStatus::new(SdvStatusCode::from(status)))?;

        let metadata = subscriber.get_publisher_unit_metadata();
        Ok(SubscriberStream { subscriber, receiver, metadata })
    }

    /// Non blocking.
    /// Subscribes to all publications of a specified unit_type and wires them into a single
    /// DataTunnel subscription. The subscription is made dynamically to all current and future
    /// publications of the specified UnitType. As a result, the object is created even if there
    /// are no publications of this UnitType at the moment. Creating more than one Subscriber
    /// instance for the same UnitType will create independent objects with separate
    /// subscription related resources.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_type: Unit Type of the publications to subscribe
    ///    [IN] min_interval: Specifies the minimal interval between nearing
    ///                       callbacks for internal notifications implementation.
    ///                       Using Duration::ZERO disables minimal interval.
    ///                       See SubscriberStream description for details.
    ///    [IN] options: see SubscriberOptions for details
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains SdvStatus of the
    ///   method call and, if method succeeded, a new Subscriber instance.
    #[allow(useless_deprecated)]
    #[allow(unused_attributes)]
    #[deprecated(since = "2.0.0", note = "Use SubscriberStream.create_for_topic instead")]
    pub async fn create_for_multipub(
        context: ContextRef,
        unit_type: &UnitType,
        min_interval: Duration,
        options: SubscriberOptions,
    ) -> SdvResult<SubscriberStream> {
        let subscriber = Subscriber::create_for_multipub(context, unit_type, options).await?;

        type SetNotificationsCb = CallbackWrapper<ASDV_NotificationType>;
        let (cb, receiver) = SetNotificationsCb::new_async();
        let vtable = ASDV_CallbackVTable_SetNotificationHandler {
            func: Some(SetNotificationsCb::trampoline_func),
            clear: Some(SetNotificationsCb::trampoline_clear),
            cb_data: cb.into_void_ptr(),
        };

        let min_interval_u32 = min_interval.as_millis().try_into().unwrap_or(u32::MAX);

        // SAFETY: subscriber.0 is a valid pointer to [ASDV_SubscriptionHandle].
        let status = unsafe {
            sdv_comms_bindgen::ASDV_SetNotificationHandler(
                subscriber.0.as_ptr(),
                vtable,
                min_interval_u32,
            )
        };
        // Propagate error if setting the handler failed
        SdvResult::from(SdvStatus::new(SdvStatusCode::from(status)))?;

        let metadata = subscriber.get_publisher_unit_metadata();
        Ok(SubscriberStream { subscriber, receiver, metadata })
    }

    /// Non blocking.
    /// Subscribes to all publications of a specified channel and wires them into a single
    /// DataTunnel subscription. The subscription is made dynamically to all current and future
    /// publications of the specified channel. Creating more than one Subscriber
    /// instance for the same channel will create independent objects with separate
    /// subscription related resources.
    ///
    /// Parameters:
    ///    [IN] context: Reference to the local SDV Comm Stack Context object
    ///    [IN] unit_type: Unit Type of the publications to subscribe
    ///    [IN] topic_name: string of the channel name to subscribe
    ///    [IN] min_interval: Specifies the minimal interval between nearing
    ///                       callbacks for internal notifications implementation.
    ///                       Using Duration::ZERO disables minimal interval.
    ///                       See SubscriberStream description for details.
    ///    [IN] options: see SubscriberOptions for details
    ///
    /// Return value:
    ///   Returns a Future of SdvResult that contains SdvStatus of the
    ///   method call and, if method succeeded, a new Subscriber instance.
    pub async fn create_for_topic(
        context: ContextRef,
        unit_type: &UnitType,
        topic_name: &str,
        min_interval: Duration,
        options: SubscriberOptions,
    ) -> SdvResult<SubscriberStream> {
        let subscriber =
            Subscriber::create_for_topic(context, unit_type, topic_name, options).await?;

        type SetNotificationsCb = CallbackWrapper<ASDV_NotificationType>;
        let (cb, receiver) = SetNotificationsCb::new_async();
        let vtable = ASDV_CallbackVTable_SetNotificationHandler {
            func: Some(SetNotificationsCb::trampoline_func),
            clear: Some(SetNotificationsCb::trampoline_clear),
            cb_data: cb.into_void_ptr(),
        };

        let min_interval_u32 = min_interval.as_millis().try_into().unwrap_or(u32::MAX);

        // SAFETY: subscriber.0 is a valid pointer to [ASDV_SubscriptionHandle].
        let status = unsafe {
            sdv_comms_bindgen::ASDV_SetNotificationHandler(
                subscriber.0.as_ptr(),
                vtable,
                min_interval_u32,
            )
        };
        // Propagate error if setting the handler failed
        SdvResult::from(SdvStatus::new(SdvStatusCode::from(status)))?;

        let metadata = subscriber.get_publisher_unit_metadata();
        Ok(SubscriberStream { subscriber, receiver, metadata })
    }

    /// Non blocking.
    /// Returns the publication metadata that contains the message size in bytes
    /// along with other publication related metadata.
    ///
    /// Parameters:
    ///
    /// Return value:
    ///    publisher unit metadata struct object.
    pub fn get_publisher_unit_metadata(&self) -> PublisherUnitMetadata {
        self.metadata.clone()
    }

    /// Non blocking.
    /// Consumes all available messages in the stream.
    /// TODO(b/430180681): Deprecate public use in favor of `read_last_message`.
    #[tracing::instrument(skip(self), fields(available_to_read))]
    pub fn read_all_available_messages(&mut self) -> SdvResult<(u32, Vec<u8>)> {
        let available = self.subscriber.available_to_read()?;
        let capacity = (available * self.metadata.get_message_size()) as usize;
        let mut data: Vec<u8> = vec![0; capacity];
        if available == 0 {
            return Ok((0, data));
        }
        let num_read = self.subscriber.read_messages(data.as_mut_slice())?;
        Ok((num_read, data))
    }
}
