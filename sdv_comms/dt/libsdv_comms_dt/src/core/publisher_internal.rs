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

use crate::core::comm_channel::Message;
use google_sdv_data_tunnel::aidl::google::sdv::data_tunnel::PublicationDescriptor::PublicationDescriptor;
use log::warn;
use sdv_comms_ctx::ContextImpl;
use sdv_comms_id::names::ServiceUnitName;
use sdv_comms_sd::manager::{register_service_unit, unregister_service_unit};
use sdv_comms_sd::metadata::PublisherUnitMetadata;
use sdv_comms_sd::UnitRegistrationArgs;
use sdv_fmq_rust::FmqWrapperRust;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};

pub struct PublisherInternal {
    fmq: FmqWrapperRust,
    metadata: PublisherUnitMetadata,
    publication_id: i32,
    ctx: &'static ContextImpl,
    unit_name: ServiceUnitName,
}

impl PublisherInternal {
    pub async fn create_publication(
        ctx: &'static ContextImpl,
        args: UnitRegistrationArgs,
    ) -> SdvResult<PublisherInternal> {
        let unit_metadata = args.get_unit_metadata().ok_or_else(|| {
            SdvStatus::with_message(SdvStatusCode::FailedPrecondition, "UnitMetadata is not set")
        })?;
        let unit_name = args
            .get_unit_name()
            .ok_or_else(|| {
                SdvStatus::with_message(
                    SdvStatusCode::FailedPrecondition,
                    "ServiceUnitName is not set",
                )
            })?
            .clone();

        let publisher_metadata = unit_metadata
            .get_publisher_metadata()
            .ok_or_else(|| {
                SdvStatus::with_message(
                    SdvStatusCode::FailedPrecondition,
                    "PublisherUnitMetadata is not set",
                )
            })?
            .clone();

        // Calculate FMQ size and construct.
        if !(publisher_metadata.get_message_size() > 0
            && publisher_metadata.get_message_count() > 0)
        {
            return Err(SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                "message_size and message_count must both be > 0.",
            ));
        }
        let fmq_size = (publisher_metadata.get_message_size() as usize)
            .checked_mul(publisher_metadata.get_message_count() as usize)
            .ok_or_else(|| {
                SdvStatus::with_message(
                    SdvStatusCode::InvalidArgument,
                    "publisher buffer size too large",
                )
            })?;
        let fmq = FmqWrapperRust::new(fmq_size);
        if !fmq.is_valid() {
            return Err(SdvStatus::with_message(
                SdvStatusCode::Internal,
                "Unable to create valid FMQ for publication!",
            ));
        }

        // Register the publisher as a service unit
        let registration_token = register_service_unit(ctx, args).await?;
        let dt_binder = ctx.get_agents().get_dt_binder();
        super::state::get_state(ctx)?;

        let mut publication_descriptor: PublicationDescriptor = Default::default();
        dt_binder.RegisterPublication(
            &registration_token,
            &fmq.dupe_desc(),
            &mut publication_descriptor,
        )?;

        Ok(PublisherInternal {
            fmq,
            metadata: publisher_metadata,
            publication_id: publication_descriptor.publication_id,
            ctx,
            unit_name,
        })
    }

    // Writes messages to FMQ without notifying agent that new data is available [FOR TESTING ONLY]
    pub fn write_messages_quiet(&mut self, messages: &[i8], message_count: u32) -> SdvResult<bool> {
        let buffer_length =
            message_count.checked_mul(self.metadata.get_message_size()).ok_or_else(|| {
                SdvStatus::with_message(SdvStatusCode::InvalidArgument, "message count too high")
            })? as usize;

        if messages.len() < buffer_length {
            return Err(SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                "Length of buffer smaller than expected",
            ));
        }
        if !self.fmq.write_to_buffer(messages, buffer_length) {
            return Err(SdvStatus::with_message(
                SdvStatusCode::Internal,
                "FMQ::WriteMessages() failed",
            ));
        }

        Ok(true)
    }

    pub fn write_messages(&mut self, messages: &[i8], message_count: u32) -> SdvResult<bool> {
        self.write_messages_quiet(messages, message_count)?;

        let dt_state = super::state::get_state(self.ctx).map_err(|e| {
            Err(SdvStatus::with_message(
                SdvStatusCode::Internal,
                format!("Unable to get internal DT state from the context object: {e}"),
            ))
        })?;

        // TODO: No unwrap here!
        // comm_channel.write_message() errors aren't critical and all the errors are logged in
        // implementation
        let _ = dt_state
            .comm_channel
            .write()
            .unwrap()
            .write_message(&Message::new_data_available(self.publication_id));
        Ok(true)
    }

    pub fn get_message_size(&self) -> u32 {
        self.metadata.get_message_size()
    }

    pub fn get_publisher_unit_metadata(&self) -> &PublisherUnitMetadata {
        &self.metadata
    }
}

impl Drop for PublisherInternal {
    fn drop(&mut self) {
        let dt_binder = self.ctx.get_agents().get_dt_binder();
        let _ = dt_binder.DestroyPublication(self.publication_id);

        let res = unregister_service_unit(self.ctx, self.unit_name.name());
        if res.is_err() {
            warn!("Failed to unregister {:?}", self.unit_name);
        }
    }
}
