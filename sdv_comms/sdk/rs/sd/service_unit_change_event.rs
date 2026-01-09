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

use crate::sd::ServiceUnitDefinition;
use crate::ContextRef;
use futures::channel::mpsc;
use futures::stream::Stream;
use futures::task::{Context, Poll};
use log::error;
use pin_project::{pin_project, pinned_drop};
use sdv_comms_bindgen::{
    ASDV_ServiceDiscoveryManager_unregisterServiceUnitChangeListener, ASDV_ServiceUnitChangeEvent,
    ASDV_ServiceUnitChangeEventType,
    ASDV_ServiceUnitChangeEventType_ASDV_SERVICE_UNIT_CHANGED as ASDV_SERVICE_UNIT_CHANGED,
    ASDV_ServiceUnitChangeEventType_ASDV_SERVICE_UNIT_REGISTERED as ASDV_SERVICE_UNIT_REGISTERED,
    ASDV_ServiceUnitChangeEventType_ASDV_SERVICE_UNIT_UNREGISTERED as ASDV_SERVICE_UNIT_UNREGISTERED,
    ASDV_ServiceUnitChangeEvent_extractDefinition, ASDV_ServiceUnitChangeEvent_getEventType,
    ASDV_ServiceUnitChangeListenerHandle,
};
use sdv_comms_common::callback::sdk::OnceCallbackReceivable;
use sdv_comms_common::owned_opaque_ptr::OwnedOpaquePtr;

use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::pin::Pin;

/// An event received by the [ServiceUnitChangeListener] indicating event change
/// notification.
#[derive(Debug)]
pub enum ServiceUnitChangeEvent {
    /// An Event fired when a new Service Unit was registered.
    /// The event includes a copy of the registered unit's [ServiceUnitDefinition].
    Registered(ServiceUnitDefinition),

    /// An Event fired when a new Service Unit was unregistered.
    /// The event includes a copy of the registered unit's [ServiceUnitDefinition].
    Unregistered(ServiceUnitDefinition),

    /// An Event fired when a new Service Unit was changed.
    /// The event includes a copy of the registered unit's [ServiceUnitDefinition].
    Changed(ServiceUnitDefinition),
}

/// [ServiceUnitChangeListener] is a stream of [ServiceUnitChangeEvent].
///
/// This class is intended for asynchronous reading messages from a subscription
/// using [futures::Stream] trait.
#[pin_project(PinnedDrop)]
pub struct UnitChangeEventStream {
    ctx: ContextRef,
    handle: OwnedOpaquePtr<ASDV_ServiceUnitChangeListenerHandle>,
    #[pin]
    receiver: mpsc::Receiver<OwnedOpaquePtr<ASDV_ServiceUnitChangeEvent>>,
}

impl UnitChangeEventStream {
    pub(crate) fn new(
        ctx: ContextRef,
        handle: OwnedOpaquePtr<ASDV_ServiceUnitChangeListenerHandle>,
        receiver: mpsc::Receiver<OwnedOpaquePtr<ASDV_ServiceUnitChangeEvent>>,
    ) -> Self {
        Self { ctx, handle, receiver }
    }
}

impl TryFrom<OwnedOpaquePtr<ASDV_ServiceUnitChangeEvent>> for ServiceUnitChangeEvent {
    type Error = SdvStatus;

    fn try_from(event: OwnedOpaquePtr<ASDV_ServiceUnitChangeEvent>) -> SdvResult<Self> {
        let mut event_type: ASDV_ServiceUnitChangeEventType = 0;

        // Safety: event is valid pointer to [ASDV_ServiceUnitChangeEvent], and event_type is a valid mutable pointer to
        // [ASDV_ServiceUnitChangeEventType].
        let sud = unsafe {
            let status =
                ASDV_ServiceUnitChangeEvent_getEventType(event.as_ptr(), &mut event_type).into();
            if status != SdvStatusCode::Ok {
                return Err(SdvStatus::with_message(
                    status,
                    format!(
                        "ASDV_ServiceUnitChangeEvent_getEventType returned error code: {status:?}"
                    ),
                ));
            }
            // Safety: event is valid pointer to [ASDV_ServiceUnitChangeEvent], and this pointer will not be used hereafter.
            let sud_ptr = ASDV_ServiceUnitChangeEvent_extractDefinition(event.as_mut_ptr());

            // We own the extracted pointer and can build a [ServiceUnitDefinition] from it.
            ServiceUnitDefinition(OwnedOpaquePtr::from_ffi_value(sud_ptr)?)
        };

        let sdk_event = match event_type {
            ASDV_SERVICE_UNIT_REGISTERED => ServiceUnitChangeEvent::Registered(sud),
            ASDV_SERVICE_UNIT_UNREGISTERED => ServiceUnitChangeEvent::Unregistered(sud),
            ASDV_SERVICE_UNIT_CHANGED => ServiceUnitChangeEvent::Changed(sud),
            _ => {
                return Err(SdvStatus::with_message(
                    SdvStatusCode::InvalidArgument,
                    format!("ASDV_ServiceUnitChangeEvent: Unknown event type: {event_type:?}"),
                ));
            }
        };

        Ok(sdk_event)
    }
}

#[pinned_drop]
impl PinnedDrop for UnitChangeEventStream {
    fn drop(self: Pin<&mut Self>) {
        // Safety: `ctx` and `handle` are valid pointers.
        unsafe {
            ASDV_ServiceDiscoveryManager_unregisterServiceUnitChangeListener(
                self.ctx.as_raw(),
                self.handle.as_mut_ptr(),
            );
        }
    }
}

impl Stream for UnitChangeEventStream {
    type Item = ServiceUnitChangeEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let pinned_stream = self.as_mut().project();
        // Pinned reference to the field
        let recv = pinned_stream.receiver;
        match recv.poll_next(cx) {
            Poll::Ready(Some(event)) => match event.try_into() {
                Ok(event) => Poll::Ready(Some(event)),
                Err(err) => {
                    error!("Unexpected event received. Stream is ended and should be dropped by reader: {err:?}");
                    Poll::Ready(None)
                }
            },
            Poll::Ready(None) => {
                // The stream has ended
                Poll::Ready(None)
            }
            Poll::Pending => {
                // No more data available right now, but the stream is not closed
                Poll::Pending
            }
        }
    }
}
