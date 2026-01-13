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
#include <sdv/service_fqin.h>
#include <sdv/service_identity.h>
#include <sdv/status.h>

#include <sdv/identity.hpp>
#include <sdv/status.hpp>

using sdv::SdvResult;
using sdv::SdvStatus;
using sdv::SdvStatusCode;

namespace sdv_comms::id {

// Ensure wrapper types are same size as the types they wrap.
static_assert(sizeof(ServiceFqin) == sizeof(ASDV_ServiceFqin));
static_assert(sizeof(ServicePublicKey) == sizeof(ASDV_ServicePublicKey));
static_assert(sizeof(ServiceIdentity) == sizeof(ASDV_ServiceIdentity));
static_assert(sizeof(UnitIdentifier) == sizeof(ASDV_UnitIdentifier));

// ServiceFqin Implementation

ServiceFqin::ServiceFqin() {
    // Not really required since the default constructor the compiler emits
    // would do what we need.  But this prevents the compiler from erroring
    // out due to an unused data member.
    memset(data_, 0, sizeof(data_));
}

SdvResult<ServiceFqin> ServiceFqin::FromString(const std::string& fqin_str) {
    // Declare result variable and get pointer to wrapped internal type.
    SdvResult<ServiceFqin> result = ServiceFqin();
    ASDV_ServiceFqin* result_data = ToFFIMutable(result.value());

    // Call internal function to parse string into result.
    auto status = ASDV_ServiceFqin_fromString((const unsigned char*)fqin_str.c_str(), result_data);
    if (status != ASDV_STATUS_OK) {
        return android::base::unexpected<SdvStatus>((SdvStatusCode)status);
    }

    return result;
}

SdvResult<ServiceFqin> ServiceFqin::Create(const std::string& sdv_vm_name,
                                           const std::string& sdv_package_name,
                                           const std::string& service_bundle_name,
                                           const std::string& service_instance_name) {
    // Declare result variable and get pointer to wrapped internal type.
    SdvResult<ServiceFqin> result = ServiceFqin();
    ASDV_ServiceFqin* result_data = ToFFIMutable(result.value());

    // Populate component fields, validating lengthsas we go
    if (!PopulateUCharBufFromString(result_data->sdv_vm_name, sdv_vm_name) ||
        !PopulateUCharBufFromString(result_data->sdv_package_name, sdv_package_name) ||
        !PopulateUCharBufFromString(result_data->service_bundle_name, service_bundle_name) ||
        !PopulateUCharBufFromString(result_data->service_instance_name, service_instance_name)) {
        return android::base::unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument);
    }

    // Do final validation using internal function.
    auto status = ASDV_ServiceFqin_validate(result_data);
    if (status != ASDV_STATUS_OK) {
        return android::base::unexpected<SdvStatus>((SdvStatusCode)status);
    }

    return result;
}

bool ServiceFqin::operator==(const ServiceFqin& o) const {
    bool are_equal = false;
    auto status = ASDV_ServiceFqin_equals(ToFFI(*this), ToFFI(o), &are_equal);
    CHECK_EQ(status, ASDV_STATUS_OK)
            << "Non-OK status from ASDV_ServiceFqin_equals(): " << SdvStatus((SdvStatusCode)status);
    return are_equal;
}

std::string ServiceFqin::ToString() const {
    unsigned char buf[ASDV_SERVICE_FQIN_MAX_SIZE_BYTES_INCLUDING_NUL];
    uint32_t buf_size = sizeof(buf);

    auto status = ASDV_ServiceFqin_toString(ToFFI(*this), buf, &buf_size);
    if (status != ASDV_STATUS_OK) {
        return "";
    }

    return std::string((char*)buf);
}

std::string_view ServiceFqin::sdv_vm_name() const {
    return (const char*)ToFFI(*this)->sdv_vm_name;
}

std::string_view ServiceFqin::sdv_package_name() const {
    return (const char*)ToFFI(*this)->sdv_package_name;
}

std::string_view ServiceFqin::service_bundle_name() const {
    return (const char*)ToFFI(*this)->service_bundle_name;
}

std::string_view ServiceFqin::service_instance_name() const {
    return (const char*)ToFFI(*this)->service_instance_name;
}

// ServicePublicKey implementation

bool ServicePublicKey::operator==(const ServicePublicKey& o) const {
    return memcmp(this->data, o.data, sizeof(this->data)) == 0;
}

// ServiceIdentity Implementation

ServiceIdentity::ServiceIdentity() {
    // Not really needed but prevents unused data member error.
    memset(&data_, 0, sizeof(data_));
}

SdvResult<ServiceIdentity> ServiceIdentity::Create(unsigned long sid, const ServiceFqin& fqin,
                                                   const ServicePublicKey& public_key) {
    // Declare result and get pointer to internal data.
    SdvResult<ServiceIdentity> result = ServiceIdentity();
    ASDV_ServiceIdentity* result_data = ToFFIMutable(result.value());

    // Populate fields.
    result_data->sid = sid;
    result_data->fqin = *ToFFI(fqin);  // Assume FQIN already validated.
    memcpy(result_data->public_key, *ToFFI(public_key), sizeof(ASDV_ServicePublicKey));

    return result;
}

bool ServiceIdentity::operator==(const ServiceIdentity& o) const {
    bool are_equal = false;
    auto status = ASDV_ServiceIdentity_equals(ToFFI(*this), ToFFI(o), &are_equal);
    CHECK_EQ(status, ASDV_STATUS_OK) << "Non-OK status from ASDV_ServiceIdentity_equals(): "
                                     << SdvStatus((SdvStatusCode)status);
    return are_equal;
}

unsigned long ServiceIdentity::sid() const {
    return ToFFI(*this)->sid;
}

const ServiceFqin& ServiceIdentity::fqin() const {
    return reinterpret_cast<const ServiceFqin&>(ToFFI(*this)->fqin);
}

const ServicePublicKey& ServiceIdentity::public_key() const {
    return reinterpret_cast<const ServicePublicKey&>(ToFFI(*this)->public_key);
}

// UnitIdentifier implementation

UnitIdentifier::UnitIdentifier() {
    // Not really needed but prevents unused data member error.
    memset(&data_, 0, sizeof(data_));
}

SdvResult<UnitIdentifier> UnitIdentifier::Create(const ServiceIdentity& service_identity,
                                                 const std::string& unit_name) {
    // Declare result and get pointer to internal data.
    SdvResult<UnitIdentifier> result = UnitIdentifier();
    ASDV_UnitIdentifier* result_data = ToFFIMutable(result.value());

    // Populate fields.
    result_data->service_identity = *ToFFI(service_identity);
    if (!PopulateUCharBufFromString(result_data->unit_name, unit_name)) {
        return android::base::unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument);
    }

    return result;
}

bool UnitIdentifier::operator==(const UnitIdentifier& o) const {
    if (service_identity() != o.service_identity()) {
        return false;
    }

    return (unit_name() == o.unit_name());
}

const ServiceIdentity& UnitIdentifier::service_identity() const {
    return reinterpret_cast<const ServiceIdentity&>(ToFFI(*this)->service_identity);
}

std::string_view UnitIdentifier::unit_name() const {
    return (const char*)ToFFI(*this)->unit_name;
}

}  // namespace sdv_comms::id