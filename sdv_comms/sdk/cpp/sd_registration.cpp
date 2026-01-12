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
#include <sdv/sd_callbacks.h>
#include <sdv/unit_discovery.h>
#include <sdv/unit_metadata.h>
#include <sdv/unit_registration.h>
#include <sdv/unit_type.h>

#include <stdint.h>

#include <sdv/sd_registration.hpp>

using sdv::SdvStatus;
using sdv::SdvStatusCode;

namespace sdv_comms::sd {

UnitRegistrationArgs::UnitRegistrationArgs(ASDV_UnitRegistrationArgs* h) : handle_(h) {}

UnitRegistrationArgs::UnitRegistrationArgs(UnitRegistrationArgs&& o) : handle_(o.handle_) {
    o.handle_ = nullptr;
}

UnitRegistrationArgs::~UnitRegistrationArgs() {
    if (handle_) {
        ASDV_UnitRegistrationArgs_destroy(handle_);
        handle_ = nullptr;
    }
}

ASDV_UnitRegistrationArgs* UnitRegistrationArgs::ReleaseABIPtr() {
    ASDV_UnitRegistrationArgs* result = handle_;
    handle_ = nullptr;
    return result;
}

UnitRegistrationArgs UnitRegistrationArgs::Create() {
    return UnitRegistrationArgs(ASDV_UnitRegistrationArgs_create());
}

bool UnitRegistrationArgs::IsValid() const {
    return handle_ != nullptr;
}

SdvStatus UnitRegistrationArgs::SetUnitName(const std::string& unit_name) {
    if (!IsValid()) {
        return SdvStatus(SdvStatusCode::kInvalidArgument);
    }

    auto status = ASDV_UnitRegistrationArgs_setUnitName(handle_, (unsigned char*)unit_name.c_str());
    return SdvStatus(status);
}

SdvStatus UnitRegistrationArgs::SetUnitType(const UnitType& unit_type) {
    if (!IsValid()) {
        return SdvStatus(SdvStatusCode::kInvalidArgument);
    }

    auto status = ASDV_UnitRegistrationArgs_setUnitType(handle_, ToFFI(unit_type));
    return SdvStatus(status);
}

SdvStatus UnitRegistrationArgs::SetUnitMetadata(const UnitMetadata& md) {
    if (!IsValid()) {
        return SdvStatus(SdvStatusCode::kInvalidArgument);
    }

    if (::std::holds_alternative<ServerUnitMetadata>(md)) {
        return SetServerUnitMetadata(::std::get<ServerUnitMetadata>(md));
    } else if (::std::holds_alternative<PublisherUnitMetadata>(md)) {
        return SetPublisherUnitMetadata(::std::get<PublisherUnitMetadata>(md));
    }

    return SdvStatus(SdvStatusCode::kInvalidArgument);
}

SdvStatus UnitRegistrationArgs::SetServerUnitMetadata(const ServerUnitMetadata& md) {
    if (!IsValid()) {
        return SdvStatus(SdvStatusCode::kInvalidArgument);
    }

    auto status = ASDV_UnitRegistrationArgs_setServerUnitMetadata(handle_, ToFFI(md));
    return SdvStatus(status);
}

SdvStatus UnitRegistrationArgs::SetPublisherUnitMetadata(const PublisherUnitMetadata& md) {
    if (!IsValid()) {
        return SdvStatus(SdvStatusCode::kInvalidArgument);
    }

    auto status = ASDV_UnitRegistrationArgs_setPublisherUnitMetadata(handle_, ToFFI(md));
    return SdvStatus(status);
}

SdvStatus UnitRegistrationArgs::SetApplicationMetadata(const uint8_t* data, size_t data_len) {
    if (!IsValid()) {
        return SdvStatus(SdvStatusCode::kInvalidArgument);
    }

    auto status = ASDV_UnitRegistrationArgs_setApplicationMetadata(handle_, data, data_len);
    return SdvStatus(status);
}

std::string UnitRegistrationArgs::GetUnitName() const {
    if (!IsValid()) {
        return "";
    }

    std::string result(UINT8_MAX, 0);  // Sized to max value of uint8_t that stores result.
    uint8_t buf_size = result.size();

    auto status = ASDV_UnitRegistrationArgs_getUnitName(handle_, (unsigned char*)result.data(),
                                                        &buf_size);
    if (status == ASDV_STATUS_OK) {
        // TODO: b/369854916 - Use buf_size once ABI call is updated to set it to the output length.
        result.resize(strlen(result.data()));
    } else {
        LOG(ERROR) << "ASDV_UnitRegistrationArgs_getUnitName failed with status "
                   << SdvStatus(status) << "; returning empty string.";
        result.resize(0);
    }

    return result;
}

UnitType UnitRegistrationArgs::GetUnitType() const {
    if (!IsValid()) {
        return UnitType();
    }

    UnitType result;
    auto status = ASDV_UnitRegistrationArgs_getUnitType(handle_, ToFFIMutable(result));
    if (status != ASDV_STATUS_OK) {
        LOG(ERROR) << "ASDV_UnitRegistrationArgs_getUnitType failed with status "
                   << SdvStatus(status) << "; returning invalid value.";
        // Zero out result on error.
        memset(&result, 0, sizeof(result));
    }

    return result;
}

UnitMetadata UnitRegistrationArgs::GetUnitMetadata() const {
    if (!IsValid()) {
        return NoUnitMetadata();
    }

    ASDV_UnitMetadataType md_type;
    auto status = ASDV_UnitRegistrationArgs_getUnitMetadataType(handle_, &md_type);
    if (status != ASDV_STATUS_OK) {
        LOG(ERROR) << "ASDV_UnitRegistrationArgs_getUnitMetaDataType failed with status "
                   << SdvStatus(status) << "; returning invalid value.";
        return NoUnitMetadata();
    }

    switch (md_type) {
        case ASDV_UNIT_METADATA_TYPE_SERVER:
            return GetServerUnitMetadata();
        case ASDV_UNIT_METADATA_TYPE_PUBLISHER:
            return GetPublisherUnitMetadata();
    }

    LOG(ERROR) << "ASDV_UnitRegistrationArgs_getUnitMetaDataType returned unsupported"
               << "metadata type: " << md_type << "; returning invalid value.";
    return NoUnitMetadata();
}

ServerUnitMetadata UnitRegistrationArgs::GetServerUnitMetadata() const {
    if (!IsValid()) {
        return ServerUnitMetadata(nullptr);
    }

    ServerUnitMetadata result = ServerUnitMetadata::Create();
    auto status = ASDV_UnitRegistrationArgs_getServerUnitMetadata(handle_, ToFFIMutable(result));
    if (status != ASDV_STATUS_OK) {
        LOG(ERROR) << "ASDV_UnitRegistrationArgs_getServerUnitMetadata failed with status "
                   << SdvStatus(status) << "; returning invalid value.";
        return ServerUnitMetadata(nullptr);
    }
    return result;
}

PublisherUnitMetadata UnitRegistrationArgs::GetPublisherUnitMetadata() const {
    if (!IsValid()) {
        return PublisherUnitMetadata(nullptr);
    }

    PublisherUnitMetadata result = PublisherUnitMetadata::Create();
    auto status = ASDV_UnitRegistrationArgs_getPublisherUnitMetadata(handle_, ToFFIMutable(result));
    if (status != ASDV_STATUS_OK) {
        LOG(ERROR) << "ASDV_UnitRegistrationArgs_getPublisherUnitMetadata failed with status "
                   << SdvStatus(status) << "; returning invalid value.";
        return PublisherUnitMetadata(nullptr);
    }
    return result;
}

}  // namespace sdv_comms::sd
