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
#pragma once

#include "sd_common.hpp"

#include <string_view>

// Classes for registering services for discovery.

struct ASDV_UnitRegistrationArgs;

namespace sdv_comms {

// Forward declaration for friendship.
namespace dt {
class Publisher;
}

namespace sd {

class UnitRegistrationArgs {
public:
    // Movable, not copyable.
    UnitRegistrationArgs(UnitRegistrationArgs&&);
    UnitRegistrationArgs(const UnitRegistrationArgs&) = delete;

    // Destructor cleans up underlying record
    ~UnitRegistrationArgs();

    // Create and return a new instance.
    // Returned object could be !IsValid() in the case of an internal error.
    static UnitRegistrationArgs Create();

    // Test whether an instance contains a valid reference.
    // An instance will become invalid when moved-from, or some interfaces
    // may return an invalid instances to indicate error.
    bool IsValid() const;

    // Attribute setters.
    sdv::SdvStatus SetUnitName(const std::string& unit_name);
    sdv::SdvStatus SetUnitType(const UnitType& unit_type);
    sdv::SdvStatus SetUnitMetadata(const UnitMetadata& metadata);
    sdv::SdvStatus SetServerUnitMetadata(const ServerUnitMetadata& metadata);
    sdv::SdvStatus SetPublisherUnitMetadata(const PublisherUnitMetadata& metadata);
    sdv::SdvStatus SetApplicationMetadata(const uint8_t* data, size_t data_len);

    // Attribute getters
    std::string GetUnitName() const;
    UnitType GetUnitType() const;
    UnitMetadata GetUnitMetadata() const;
    ServerUnitMetadata GetServerUnitMetadata() const;
    PublisherUnitMetadata GetPublisherUnitMetadata() const;

private:
    friend dt::Publisher;

    UnitRegistrationArgs() = delete;
    UnitRegistrationArgs(ASDV_UnitRegistrationArgs*);

    // Releases ownership of handle and returns it without destroying it.
    ASDV_UnitRegistrationArgs* ReleaseABIPtr();

    // Pointer to owned underlying data.
    ASDV_UnitRegistrationArgs* handle_ = nullptr;
};

}  // namespace sd
}  // namespace sdv_comms