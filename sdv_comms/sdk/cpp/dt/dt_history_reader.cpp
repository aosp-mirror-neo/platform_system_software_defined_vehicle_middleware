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

#include <libsdv_comms_dt.h>

#include <functional>
#include <future>
#include <memory>

#include <callback_helpers.hpp>
#include <ffi_helpers.hpp>
#include <sdv/context.hpp>
#include <sdv/dt_history_reader.hpp>
#include <sdv/identity.hpp>
#include <sdv/sd_common.hpp>
#include <sdv/status.hpp>

using android::base::unexpected;
using sdv::SdvResult;
using sdv::SdvStatus;
using sdv::SdvStatusCode;

namespace sdv_comms::dt {

HistoryReader::HistoryReader(ASDV_HistoryHandle* handle) : handle_(handle) {}

HistoryReader::~HistoryReader() {
    if (handle_) {
        CHECK(ASDV_UnsubscribeFromHistory(handle_) == ASDV_STATUS_OK);
        handle_ = nullptr;
    }
}

HistoryReader::HistoryReader(HistoryReader&& o) : handle_(o.handle_) {
    // Taking ownership
    o.handle_ = nullptr;
}

bool HistoryReader::IsValid() const {
    return (handle_ != nullptr);
}

using CreateResult = HistoryReader::CreateResult;

void HistoryReader::CreateAsync(const ctx::Context& context, const id::UnitIdentifier& unit_id,
                                const uint32_t history_size,
                                std::function<void(CreateResult)> callback) {
    static const auto _ReaderFromHandle = [](ASDV_HistoryHandle* h) { return HistoryReader(h); };
    auto abi_cb = MakeOnceABICallback<_ReaderFromHandle>(std::move(callback));
    auto vtable = MakeOnceVTable<ASDV_SubscribeToHistory, 3>(std::move(abi_cb));
    ASDV_SubscribeToHistory(context.ToABIPtr(), ToFFI(unit_id), history_size, vtable);
}

std::future<CreateResult> HistoryReader::CreateFuture(const ctx::Context& context,
                                                      const id::UnitIdentifier& unit_id,
                                                      const uint32_t history_size) {
    auto [result_future, callback] = MakeFutureSatisfyingCallback<CreateResult>();
    CreateAsync(context, unit_id, history_size, std::move(callback));
    return std::move(result_future);
}

CreateResult HistoryReader::Create(const ctx::Context& context, const id::UnitIdentifier& uid,
                                   const uint32_t history_size) {
    return CreateFuture(context, uid, history_size).get();
}

sd::PublisherUnitMetadata HistoryReader::GetMetadata() const {
    if (!IsValid()) {
        LOG(ERROR) << "GetMetadata() called on invalid HistoryReader.  Returning invalid value.";
        return sd::PublisherUnitMetadata(nullptr);
    }
    auto result = sd::PublisherUnitMetadata::Create();

    auto status = ASDV_GetPublisherUnitMetadataFromHistory(handle_, ToFFIMutable(result));
    if (status != ASDV_STATUS_OK) {
        LOG(ERROR) << "ASDV_GetPublisherUnitMetadataFromHistory failed with status "
                   << SdvStatus(status) << "; returning invalid value.";
        return sd::PublisherUnitMetadata(nullptr);
    }

    return result;
}

SdvResult<uint32_t> HistoryReader::AvailableToRead() const {
    if (!IsValid()) {
        LOG(ERROR) << "AvailableToRead() called on invalid HistoryReader.";
        return unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument);
    }
    uint32_t result;
    auto status = ASDV_AvailableInHistory(handle_, &result);
    if (status != ASDV_STATUS_OK) {
        return unexpected(SdvStatus(status));
    }

    return result;
}

SdvResult<uint32_t> HistoryReader::ReadLastMessages(uint8_t* _Nonnull messages,
                                                    const uint32_t buffer_size) const {
    if (!IsValid() || !messages || !buffer_size) {
        LOG(ERROR) << "ReadLastMessages() called on invalid HistoryReader.";
        return unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument);
    }
    uint32_t read_message_count;
    auto status = ASDV_ReadHistoryMessages(handle_, messages, buffer_size, &read_message_count);
    if (status != ASDV_STATUS_OK) {
        return unexpected(SdvStatus(status));
    }

    return read_message_count;
}

}  // namespace sdv_comms::dt
