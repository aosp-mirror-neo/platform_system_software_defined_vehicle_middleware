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

#include <functional>
#include <future>
#include <memory>

#include <callback_helpers.hpp>
#include <ffi_helpers.hpp>
#include <sdv/dt_publisher.hpp>
#include <sdv/sd_common.hpp>
#include <sdv/sd_registration.hpp>
#include <sdv/status.hpp>

using android::base::unexpected;
using sdv::SdvResult;
using sdv::SdvStatus;
using sdv::SdvStatusCode;

namespace sdv_comms::dt {

Publisher::Publisher(ASDV_PublicationHandle* handle) : handle_(handle) {}

Publisher::~Publisher() {
    if (handle_) {
        ASDV_DestroyPublication(handle_);
        handle_ = nullptr;
    }
}

Publisher::Publisher(Publisher&& o) : handle_(o.handle_) {
    // Taking ownership
    o.handle_ = nullptr;
}

bool Publisher::IsValid() const {
    return (handle_ != nullptr);
}

using CreateResult = Publisher::CreateResult;

void Publisher::CreateAsync(const ctx::Context& context, sd::UnitRegistrationArgs unit_reg_args,
                            std::function<void(CreateResult)> callback) {
    if (!context.ToABIPtr()) {
        callback(unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument));
        return;
    }
    static const auto _PubFromHandle = [](ASDV_PublicationHandle* h) { return Publisher(h); };
    auto abi_cb = MakeOnceABICallback<_PubFromHandle>(std::move(callback));
    auto vtable = MakeOnceVTable<ASDV_CreatePublication, 2>(std::move(abi_cb));
    ASDV_CreatePublication(context.ToABIPtr(), unit_reg_args.ReleaseABIPtr(), vtable);
}

std::future<CreateResult> Publisher::CreateFuture(const ctx::Context& context,
                                                  sd::UnitRegistrationArgs unit_reg_args) {
    auto [result_future, callback] = MakeFutureSatisfyingCallback<CreateResult>();
    CreateAsync(context, std::move(unit_reg_args), std::move(callback));
    return std::move(result_future);
}

CreateResult Publisher::Create(const ctx::Context& context,
                               sd::UnitRegistrationArgs unit_reg_args) {
    return CreateFuture(context, std::move(unit_reg_args)).get();
}

sd::PublisherUnitMetadata Publisher::GetMetadata() const {
    if (!IsValid()) {
        LOG(ERROR) << "GetMetadata() called on invalid Publisher; returning invalid result.";
        return sd::PublisherUnitMetadata(nullptr);
    }
    auto result = sd::PublisherUnitMetadata::Create();

    auto status = ASDV_GetPublisherUnitMetadata(handle_, ToFFIMutable(result));
    if (status != ASDV_STATUS_OK) {
        LOG(ERROR) << "ASDV_GetPublisherUnitMetadata failed with status " << SdvStatus(status)
                   << "; returning invalid value.";
        return sd::PublisherUnitMetadata(nullptr);
    }

    return result;
}

SdvResult<uint32_t> Publisher::WriteMessages(const void* messages, uint32_t buffer_size) {
    if (!IsValid() || !messages || !buffer_size) {
        return unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument);
    }
    uint32_t written = 0;

    auto status = ASDV_WriteMessages(handle_, messages, buffer_size, &written);
    if (status != ASDV_STATUS_OK) {
        return unexpected<SdvStatus>(SdvStatus(status));
    }

    return written;
}

}  // namespace sdv_comms::dt
