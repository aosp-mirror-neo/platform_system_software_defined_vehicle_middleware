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

#include "ffi_helpers.hpp"

#include <android-base/logging.h>
#include <sdv/ctx.h>

#include <sdv/context.hpp>

using android::base::unexpected;
using sdv::SdvResult;
using sdv::SdvStatus;
using sdv::SdvStatusCode;

namespace sdv_comms::ctx {

Context::Context(const ASDV_Context* handle) {
    CHECK(handle) << "Null handle passed to Context constructor!";
    handle_ = handle;
}

SdvResult<Context> Context::Create(const ASDV_Context* handle) {
    if (!handle) {
        return unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument);
    }
    return Context(handle);
}

Context Context::CreateForTest(const id::ServiceFqin& fqin) {
    const ASDV_Context* ctx_ptr = ASDV_Context_temporaryCreate(ToFFI(fqin));
    CHECK(ctx_ptr);
    return Context(ctx_ptr);
}

const ASDV_Context* Context::ToABIPtr() const {
    return handle_;
}

bool Context::IsValid() const {
    return handle_ != nullptr;
}

SdvResult<id::ServiceIdentity> Context::GetSelfIdentity() const {
    if (!handle_) {
        return unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument);
    }
    ASDV_ServiceIdentity result;
    auto status = ASDV_Context_getSelfIdentity(handle_, &result);
    if (status != ASDV_STATUS_OK) {
        return unexpected<SdvStatus>((SdvStatusCode)status);
    }
    return reinterpret_cast<id::ServiceIdentity&>(result);
}

SdvResult<id::ServiceFqin> Context::GetSelfFqin() const {
    if (!handle_) {
        return unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument);
    }
    ASDV_ServiceFqin result;
    auto status = ASDV_Context_getSelfFqin(handle_, &result);
    if (status != ASDV_STATUS_OK) {
        return unexpected<SdvStatus>((SdvStatusCode)status);
    }
    return reinterpret_cast<id::ServiceFqin&>(result);
}

SdvResult<id::ServicePublicKey> Context::GetPublicKey() const {
    if (!handle_) {
        return unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument);
    }
    ASDV_ServiceIdentity result;
    auto status =
            ASDV_Context_getSelfIdentity(reinterpret_cast<const ASDV_Context*>(handle_), &result);
    if (status != ASDV_STATUS_OK) {
        return unexpected<SdvStatus>((SdvStatusCode)status);
    }
    return reinterpret_cast<id::ServicePublicKey&>(result.public_key);
}

}  // namespace sdv_comms::ctx