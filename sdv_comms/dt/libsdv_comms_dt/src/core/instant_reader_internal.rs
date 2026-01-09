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

use google_sdv_identity::aidl::google::sdv::identity::UnitIdentifier::UnitIdentifier as UnitIdentifierAidl;

use sdv_comms_ctx::ContextImpl;
use sdv_comms_id::UnitIdentifier;
use sdv_status_rs::SdvResult;

#[allow(dead_code)]
pub async fn get_last_publication_message(
    ctx: &ContextImpl,
    unit_identifier: &UnitIdentifier,
    message: &mut [i8],
) -> SdvResult<u32> {
    let dt_binder = ctx.get_agents().get_dt_binder();
    super::state::get_state(ctx)?;

    let unit_identifier_aidl: UnitIdentifierAidl = unit_identifier.into();
    let mut buffer: Vec<u8> = vec![];
    dt_binder.GetLastPublicationMessage(&unit_identifier_aidl, &mut buffer)?;

    // SAFETY: we just transmuting data type of the slice from [i8] to [u8]
    let message = unsafe { std::mem::transmute::<&mut [i8], &mut [u8]>(message) };

    let message_size: usize = buffer.len();
    message[..message_size].copy_from_slice(&buffer);

    Ok(message_size as u32)
}
