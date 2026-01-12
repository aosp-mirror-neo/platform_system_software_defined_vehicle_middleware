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

#include <android-base/logging.h>

#include <libsdv_comms_dt.h>

#include <callback_helpers.hpp>
#include <ffi_helpers.hpp>
#include <sdv/context.hpp>
#include <sdv/dt_instant_reader.hpp>
#include <sdv/identity.hpp>
#include <sdv/status.hpp>

using android::base::unexpected;
using sdv::SdvStatus;
using sdv::SdvStatusCode;

namespace sdv_comms::dt {

void InstantReader::GetLastMessageAsync(const ctx::Context& context,
                                        const id::UnitIdentifier& unit_id,
                                        std::function<void(InstantReader::Result)> callback) {
    if (!context.IsValid()) {
        callback(unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument));
        return;
    }

    // NOTE: This vector gets move-captured in a lambda.  If we ever *copy* that lambda rather than
    // moving it, std::vector's copy constructor will run and create a twin, and if that copy of the
    // lambda is run instead of the original, the buffer we return to the client won't be the one
    // that was actually written to.
    //
    // We only ever move the lambda that captures this vector (both here and in callback_helpers),
    // so this won't happen; a unit test should be able to catch it if it does.
    Message result_bytes(ASDV_MaxInstantReadMessageSize);

    // Get pointer to data before we move-capture the owning vector in the lambda below.
    void* result_buffer = result_bytes.data();

    // Can't use MakeOnceABICallback because we need the result_bytes vector which isn't statically
    // derivable from the length value the ABI passes back.
    auto abi_cb = [cb = std::move(callback),
                   rb = std::move(result_bytes)](uint32_t result_length,
                                                 sdv_status_t status) mutable {
        if (status != ASDV_STATUS_OK) {
            cb(unexpected(SdvStatus(status)));
            return;
        }

        rb.resize(result_length);
        cb(std::move(rb));
    };
    auto vtable = MakeOnceVTable<ASDV_GetLastPublicationMessage, 4>(std::move(abi_cb));

    ASDV_GetLastPublicationMessage(context.ToABIPtr(), ToFFI(unit_id), result_buffer,
                                   ASDV_MaxInstantReadMessageSize, vtable);
}

std::future<InstantReader::Result> InstantReader::GetLastMessageFuture(
        const ctx::Context& context, const id::UnitIdentifier& unit_id) {
    auto [result_future, callback] = MakeFutureSatisfyingCallback<Result>();
    GetLastMessageAsync(context, unit_id, std::move(callback));
    return std::move(result_future);
}

InstantReader::Result InstantReader::GetLastMessage(const ctx::Context& context,
                                                    const id::UnitIdentifier& unit_id) {
    return GetLastMessageFuture(context, unit_id).get();
}

}  // namespace sdv_comms::dt