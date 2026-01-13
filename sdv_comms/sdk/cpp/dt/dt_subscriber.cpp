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

#include <callback_helpers.hpp>
#include <ffi_helpers.hpp>
#include <sdv/dt_subscriber.hpp>
#include <sdv/identity.hpp>
#include <sdv/status.hpp>

using android::base::unexpected;
using sdv::SdvResult;
using sdv::SdvStatus;
using sdv::SdvStatusCode;

namespace sdv_comms::dt {

Subscriber::Subscriber(ASDV_SubscriptionHandle* handle) : handle_(handle) {}

Subscriber::~Subscriber() {
    if (handle_) {
        ASDV_UnsubscribeFromPublication(handle_);
        handle_ = nullptr;
    }
}

Subscriber::Subscriber(Subscriber&& o) : handle_(o.handle_) {
    // Taking ownership
    o.handle_ = nullptr;
}

bool Subscriber::IsValid() const {
    return (handle_ != nullptr);
}

namespace {
uint64_t OptionsToFlags(const Subscriber::CreateOptions& opts) {
    uint64_t result = 0;
    if (opts.fetch_last_message) {
        result |= ASDV_SubscriptionFlags_FetchLastMessage;
    }
    return result;
}
}  // namespace

using CreateResult = Subscriber::CreateResult;

void Subscriber::CreateAsync(const ctx::Context& context, const id::UnitIdentifier& unit_id,
                             Subscriber::CreateOptions options,
                             std::function<void(CreateResult)> callback) {
    if (!context.ToABIPtr()) {
        callback(unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument));
        return;
    }
    static const auto _SubFromHandle = [](ASDV_SubscriptionHandle* h) { return Subscriber(h); };
    auto abi_cb = MakeOnceABICallback<_SubFromHandle>(std::move(callback));
    auto vtable = MakeOnceVTable<ASDV_SubscribeToPublication, 3>(std::move(abi_cb));
    ASDV_SubscribeToPublication(context.ToABIPtr(), ToFFI(unit_id), OptionsToFlags(options),
                                vtable);
}

std::future<CreateResult> Subscriber::CreateFuture(const ctx::Context& context,
                                                   const id::UnitIdentifier& unit_id,
                                                   Subscriber::CreateOptions options) {
    auto [result_future, callback] = MakeFutureSatisfyingCallback<CreateResult>();
    CreateAsync(context, unit_id, options, std::move(callback));
    return std::move(result_future);
}

SdvResult<Subscriber> Subscriber::Create(const ctx::Context& context,
                                         const id::UnitIdentifier& unit_id,
                                         Subscriber::CreateOptions options) {
    return CreateFuture(context, unit_id, options).get();
}

sd::PublisherUnitMetadata Subscriber::GetMetadata() const {
    if (!IsValid()) {
        LOG(ERROR) << "GetMetadata() called on invalid Publisher; returning invalid result.";
        return sd::PublisherUnitMetadata(nullptr);
    }
    auto result = sd::PublisherUnitMetadata::Create();

    auto status = ASDV_GetPublisherUnitMetadataFromSubscriber(handle_, ToFFIMutable(result));
    if (status != ASDV_STATUS_OK) {
        LOG(ERROR) << "ASDV_GetPublisherUnitMetadataFromSubscriber failed with status "
                   << SdvStatus(status) << "; returning invalid value.";
        return sd::PublisherUnitMetadata(nullptr);
    }

    return result;
}

sdv::SdvResult<uint32_t> Subscriber::AvailableToRead() const {
    if (!IsValid()) {
        return unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument);
    }
    uint32_t message_count;
    auto status = ASDV_AvailableToRead(handle_, &message_count);
    if (status != ASDV_STATUS_OK) {
        return unexpected<SdvStatus>(SdvStatus(status));
    }
    return message_count;
}

sdv::SdvResult<uint32_t> Subscriber::ReadMessages(void* messages, uint32_t buffer_size) {
    if (!IsValid() || !messages || !buffer_size) {
        return unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument);
    }
    uint32_t message_count = 0;
    auto status = ASDV_ReadMessages(handle_, messages, buffer_size, &message_count);
    if (status != ASDV_STATUS_OK) {
        return unexpected<SdvStatus>(SdvStatus(status));
    }
    return message_count;
}

SdvStatus Subscriber::SetNotificationHandler(Subscriber::NotificationCallback callback,
                                             const uint32_t min_interval_ms) {
    if (!IsValid()) {
        return SdvStatus(SdvStatusCode::kInvalidArgument);
    }
    static const auto _ConvertNotification = [](ASDV_NotificationType n) {
        switch (n) {
            case ASDV_NotificationType::DataAvailable:
                return NotificationType::DataAvailable;
            case ASDV_NotificationType::Exception:
                return NotificationType::Exception;
        }
        return NotificationType::Unknown;
    };
    auto abi_cb = MakeRepeatedABICallback<_ConvertNotification>(std::move(callback));
    auto vtable = MakeRepeatedVTable<ASDV_SetNotificationHandler, 1>(std::move(abi_cb));
    return SdvStatus(ASDV_SetNotificationHandler(handle_, vtable, min_interval_ms));
}

SdvStatus Subscriber::ClearNotificationHandler() {
    if (!IsValid()) {
        return SdvStatus(SdvStatusCode::kInvalidArgument);
    }
    return SdvStatus(ASDV_ClearNotificationHandler(handle_));
}

}  // namespace sdv_comms::dt
