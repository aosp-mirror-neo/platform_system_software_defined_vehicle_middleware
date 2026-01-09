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

#include <sdv/sd_common.hpp>

using android::base::unexpected;
using sdv::SdvResult;
using sdv::SdvStatus;
using sdv::SdvStatusCode;

namespace sdv_comms::sd {

// Check that inline-data wrapper types with match size of wrapped types.
static_assert(sizeof(UnitType) == sizeof(ASDV_UnitType));

// UnitType implementation

SdvResult<UnitType> UnitType::FromString(const std::string& unit_type_str) {
    // Declare result variable and get pointer to wrapped internal type.
    SdvResult<UnitType> result = UnitType();
    ASDV_UnitType* result_data = ToFFIMutable(result.value());

    // Call internal function to parse string into result.
    auto status = ASDV_UnitType_fromString((const unsigned char*)unit_type_str.data(), result_data);
    if (status != ASDV_STATUS_OK) {
        return unexpected<SdvStatus>(static_cast<SdvStatusCode>(status));
    }

    return result;
}

SdvResult<UnitType> UnitType::Create(const std::string& sdv_package_name,
                                     const std::string& service_bundle_name,
                                     const std::string& unit_type_name) {
    SdvResult<UnitType> result = UnitType();
    auto result_data = ToFFIMutable(result.value());

    // Copy inputs into result struct, validating length for each.
    if (!PopulateUCharBufFromString(result_data->sdv_package_name, sdv_package_name) ||
        !PopulateUCharBufFromString(result_data->service_bundle_name, service_bundle_name) ||
        !PopulateUCharBufFromString(result_data->unit_type_name, unit_type_name)) {
        return unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument);
    }

    auto status = ASDV_UnitType_validate(result_data);
    if (status != ASDV_STATUS_OK) {
        return unexpected<SdvStatus>(static_cast<SdvStatusCode>(status));
    }

    return result;
}

bool UnitType::IsValid() const {
    const auto* data = ToFFI(*this);
    return (data->sdv_package_name[0] != '\0' && data->unit_type_name[0] != '\0');
}

// The maximum size in bytes (including the NUL (\0) character) for the complete
// formatted unit type.
//
// Sum all Unit Type part sizes with NUL - 3 NULs + 2 delimiters ('.', '#') + 1 NUL.
static const size_t kAsdvUnitTypeMaxLength = ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL +
        ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL +
        ASDV_UNIT_TYPE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL;

std::string UnitType::ToString() const {
    std::string result(kAsdvUnitTypeMaxLength, 0);
    uint32_t buf_size = result.size();

    auto status = ASDV_UnitType_toString(ToFFI(*this), (unsigned char*)result.data(), &buf_size);
    if (status == ASDV_STATUS_OK) {
        result.resize(buf_size - 1);  // Returned buf_size includes NUL
    } else {
        result.resize(0);
    }

    return result;
}

std::string_view UnitType::sdv_package_name() const {
    return (const char*)ToFFI(*this)->sdv_package_name;
}

std::string_view UnitType::service_bundle_name() const {
    return (const char*)ToFFI(*this)->service_bundle_name;
}

std::string_view UnitType::unit_type_name() const {
    return (const char*)ToFFI(*this)->unit_type_name;
}

UnitType::UnitType() {
    memset(data_, 0, sizeof(data_));
}

// ServerUnitMetadata implementation

ServerUnitMetadata::ServerUnitMetadata(const ServerUnitMetadata& o) {
    handle_ = o.handle_ ? ASDV_ServerUnitMetadata_clone(o.handle_) : nullptr;
}

ServerUnitMetadata::ServerUnitMetadata(ServerUnitMetadata&& o) : handle_(o.handle_) {
    o.handle_ = nullptr;
}

ServerUnitMetadata::~ServerUnitMetadata() {
    if (handle_) {
        ASDV_ServerUnitMetadata_destroy(handle_);
        handle_ = nullptr;
    }
}

ServerUnitMetadata ServerUnitMetadata::Create() {
    return ServerUnitMetadata(ASDV_ServerUnitMetadata_create());
}

bool ServerUnitMetadata::IsValid() const {
    return handle_ != nullptr;
}

sdv::SdvStatus ServerUnitMetadata::SetVersion(uint32_t version) {
    if (!IsValid()) {
        return SdvStatus(SdvStatusCode::kInvalidArgument);
    }

    auto status = ASDV_ServerUnitMetadata_setVersion(handle_, &version);

    return SdvStatus(static_cast<SdvStatusCode>(status));
}

uint32_t ServerUnitMetadata::GetVersion() const {
    uint32_t version = 0;
    if (ASDV_ServerUnitMetadata_getVersion(handle_, &version) != ASDV_STATUS_OK) {
        return 0;
    }

    return version;
}

bool ServerUnitMetadata::operator==(const ServerUnitMetadata& o) const {
    // If both are empty, consider equal.
    if (!o.handle_ && !handle_) {
        return true;
    }
    // If only one is empty, consider unequal.
    if (!o.handle_ || !handle_) {
        return false;
    }
    // If neither is empty, call comparison
    bool result = false;
    CHECK(ASDV_ServerUnitMetadata_equals(handle_, o.handle_, &result) == ASDV_STATUS_OK);
    return result;
}

ServerUnitMetadata::ServerUnitMetadata(ASDV_ServerUnitMetadata* h) : handle_(h) {}

ASDV_ServerUnitMetadata* ServerUnitMetadata::ToMutableABIPtr() {
    return handle_;
}

// PublisherUnitMetadata implementation

PublisherUnitMetadata::PublisherUnitMetadata(const PublisherUnitMetadata& o) {
    handle_ = o.handle_ ? ASDV_PublisherUnitMetadata_clone(o.handle_) : nullptr;
}

PublisherUnitMetadata::PublisherUnitMetadata(PublisherUnitMetadata&& o) : handle_(o.handle_) {
    o.handle_ = nullptr;
}

PublisherUnitMetadata::~PublisherUnitMetadata() {
    if (handle_) {
        ASDV_PublisherUnitMetadata_destroy(handle_);
        handle_ = nullptr;
    }
}

PublisherUnitMetadata PublisherUnitMetadata::Create() {
    return PublisherUnitMetadata(ASDV_PublisherUnitMetadata_create());
}

bool PublisherUnitMetadata::IsValid() const {
    return handle_ != nullptr;
}

sdv::SdvStatus PublisherUnitMetadata::SetVersion(uint32_t version) {
    if (!IsValid()) {
        return SdvStatus(SdvStatusCode::kInvalidArgument);
    }

    auto status = ASDV_PublisherUnitMetadata_setVersion(handle_, &version);
    return SdvStatus(static_cast<SdvStatusCode>(status));
}

sdv::SdvStatus PublisherUnitMetadata::SetMessageSize(uint32_t message_size) {
    if (!IsValid()) {
        return SdvStatus(SdvStatusCode::kInvalidArgument);
    }

    auto status = ASDV_PublisherUnitMetadata_setMessageSize(handle_, &message_size);
    return SdvStatus(static_cast<SdvStatusCode>(status));
}

sdv::SdvStatus PublisherUnitMetadata::SetMessageCount(uint32_t message_count) {
    if (!IsValid()) {
        return SdvStatus(SdvStatusCode::kInvalidArgument);
    }

    auto status = ASDV_PublisherUnitMetadata_setMessageCount(handle_, &message_count);
    return SdvStatus(static_cast<SdvStatusCode>(status));
}

uint32_t PublisherUnitMetadata::GetVersion() const {
    uint32_t version = 0;
    if (ASDV_PublisherUnitMetadata_getVersion(handle_, &version) != ASDV_STATUS_OK) {
        return 0;
    }

    return version;
}

uint32_t PublisherUnitMetadata::GetMessageSize() const {
    uint32_t message_size = 0;
    if (ASDV_PublisherUnitMetadata_getMessageSize(handle_, &message_size) != ASDV_STATUS_OK) {
        return 0;
    }

    return message_size;
}

uint32_t PublisherUnitMetadata::GetMessageCount() const {
    uint32_t message_count = 0;
    if (ASDV_PublisherUnitMetadata_getMessageCount(handle_, &message_count) != ASDV_STATUS_OK) {
        return 0;
    }

    return message_count;
}

bool PublisherUnitMetadata::operator==(const PublisherUnitMetadata& o) const {
    // Equal if both are empty.
    if (!o.handle_ && !handle_) {
        return true;
    }
    // Unequal if only one is empty.
    if (!o.handle_ || !handle_) {
        return false;
    }
    // If neither is empty, call comparison function.
    bool result = false;
    CHECK(ASDV_PublisherUnitMetadata_equals(handle_, o.handle_, &result) == ASDV_STATUS_OK);
    return result;
}

PublisherUnitMetadata::PublisherUnitMetadata(ASDV_PublisherUnitMetadata* h) : handle_(h) {}

ASDV_PublisherUnitMetadata* PublisherUnitMetadata::ToMutableABIPtr() {
    return handle_;
}

// UnitNameDiscoveryArgs implementation

// Check that inline-data wrapper types with match size of wrapped types.
static_assert(sizeof(UnitNameDiscoveryArgs) == sizeof(ASDV_UnitNameDiscoveryArgs));

SdvResult<UnitNameDiscoveryArgs> UnitNameDiscoveryArgs::Create(
        const std::string& sdv_package_name, const std::string& service_bundle_name,
        const std::string& unit_name, const std::string& sdv_vm_name) {
    // Declare result variable and get pointer to wrapped internal type.
    SdvResult<UnitNameDiscoveryArgs> result = UnitNameDiscoveryArgs();
    ASDV_UnitNameDiscoveryArgs* result_data = ToFFIMutable(result.value());

    // Validate lengths of component fields (including null terminators) to ensure they'll
    // fit into ASDV_UnitNameDiscoveryArgs fields.
    if (!PopulateUCharBufFromString(result_data->sdv_package_name, sdv_package_name) ||
        !PopulateUCharBufFromString(result_data->service_bundle_name, service_bundle_name) ||
        !PopulateUCharBufFromString(result_data->unit_name, unit_name) ||
        !PopulateUCharBufFromString(result_data->sdv_vm_name, sdv_vm_name)) {
        return unexpected<SdvStatus>(SdvStatusCode::kInvalidArgument);
    }

    return result;
}

// Component accessors
std::string_view UnitNameDiscoveryArgs::sdv_package_name() const {
    return (const char*)ToFFI(*this)->sdv_package_name;
}

std::string_view UnitNameDiscoveryArgs::service_bundle_name() const {
    return (const char*)ToFFI(*this)->service_bundle_name;
}

std::string_view UnitNameDiscoveryArgs::unit_name() const {
    return (const char*)ToFFI(*this)->unit_name;
}

std::string_view UnitNameDiscoveryArgs::sdv_vm_name() const {
    return (const char*)ToFFI(*this)->sdv_vm_name;
}

UnitNameDiscoveryArgs::UnitNameDiscoveryArgs() {
    // Suppresses "private field not used" compiler warning.
    memset(data_, 0, sizeof(data_));
}

// InferredMetadata implementation

InferredMetadata::InferredMetadata(ASDV_InferredMetadata* h) : handle_(h) {}

InferredMetadata::InferredMetadata(InferredMetadata&& o) : handle_(o.handle_) {
    o.handle_ = nullptr;
}

InferredMetadata::~InferredMetadata() {
    if (handle_) {
        ASDV_InferredMetadata_destroy(handle_);
        handle_ = nullptr;
    }
}

InferredMetadata InferredMetadata::Create() {
    return InferredMetadata(ASDV_InferredMetadata_create());
}

// Test whether the wrapper owns a reference to an internal object.
bool InferredMetadata::IsValid() const {
    return handle_ != nullptr;
}

ASDV_InferredMetadata* InferredMetadata::ToMutableABIPtr() {
    return handle_;
}

// Get the location of the service unit relative to the caller.
InferredMetadata::RelativeLocation InferredMetadata::GetLocation() const {
    if (!IsValid()) {
        return RelativeLocation::Unknown;
    }

    ASDV_RelativeLocation location;
    auto status = ASDV_InferredMetadata_getLocation(handle_, &location);
    if (status != ASDV_STATUS_OK) {
        LOG(ERROR) << "ASDV_InferredMetadata_getLocation failed with status " << status
                   << " returning kUnknown.";
        return RelativeLocation::Unknown;
    }

    switch (location) {
        case ASDV_LOCATION_UNKNOWN:
            return RelativeLocation::Unknown;
        case ASDV_LOCATION_LOCAL_SDV_INSTANCE:
            return RelativeLocation::LocalSdvInstance;
        case ASDV_LOCATION_LOCAL_HARDWARE:
            return RelativeLocation::LocalHardware;
        case ASDV_LOCATION_REMOTE:
            return RelativeLocation::Remote;
    }

    return RelativeLocation::Unknown;
}

// ServiceUnitDefinition implementation

ServiceUnitDefinition::ServiceUnitDefinition(ASDV_ServiceUnitDefinition* h) : handle_(h) {}

ServiceUnitDefinition::ServiceUnitDefinition(const ServiceUnitDefinition& o) {
    handle_ = o.handle_ ? ASDV_ServiceUnitDefinition_clone(o.handle_) : nullptr;
}

ServiceUnitDefinition ServiceUnitDefinition::FromABIPtr(ASDV_ServiceUnitDefinition* h) {
    return ServiceUnitDefinition(h);
}

ServiceUnitDefinition::ServiceUnitDefinition(ServiceUnitDefinition&& o) : handle_(o.handle_) {
    // Clear other instance handle to reflect transfer of ownership.
    o.handle_ = nullptr;
}

// Destructor to clean up underlying record.
ServiceUnitDefinition::~ServiceUnitDefinition() {
    if (handle_) {
        ASDV_ServiceUnitDefinition_destroy(handle_);
        handle_ = nullptr;
    }
}

// Test whether the wrapper owns a reference to an internal object.
bool ServiceUnitDefinition::IsValid() const {
    return handle_ != nullptr;
}

// Returns the UnitIdentifier for the Service Unit.
id::UnitIdentifier ServiceUnitDefinition::GetUnitIdentifier() const {
    id::UnitIdentifier result;
    auto status = ASDV_ServiceUnitDefinition_getUnitIdentifier(handle_, ToFFIMutable(result));
    CHECK(status == ASDV_STATUS_OK);
    return result;
}

// Returns the UnitIdentifier for the Service Unit.
UnitType ServiceUnitDefinition::GetUnitType() const {
    UnitType result;
    auto status = ASDV_ServiceUnitDefinition_getUnitType(handle_, ToFFIMutable(result));
    CHECK(status == ASDV_STATUS_OK);
    return result;
}

// Returns the identity for the bundle that registered the Service Unit.
id::ServiceIdentity ServiceUnitDefinition::GetServiceBundleIdentity() const {
    id::ServiceIdentity result;
    auto status = ASDV_ServiceUnitDefinition_getServiceIdentity(handle_, ToFFIMutable(result));
    CHECK(status == ASDV_STATUS_OK);
    return result;
}

// Returns the InferredMetadata for the Service Unit.
InferredMetadata ServiceUnitDefinition::GetInferredMetadata() const {
    auto metadata = InferredMetadata::Create();
    if (!metadata.IsValid()) {
        return metadata;
    }

    auto status =
            ASDV_ServiceUnitDefinition_getInferredMetadata(handle_, metadata.ToMutableABIPtr());

    if (status != ASDV_STATUS_OK) {
        LOG(ERROR) << "ASDV_ServiceUnitDefinition_getInferredMetadata failed with status "
                   << status;
        // No copy constructor, cast to rvalue reference for "move"
        return InferredMetadata(nullptr);
    }

    return metadata;
}

UnitMetadata ServiceUnitDefinition::GetUnitMetadata() const {
    ASDV_UnitMetadataType md_type;

    auto status = ASDV_ServiceUnitDefinition_getUnitMetadataType(handle_, &md_type);
    if (status != ASDV_STATUS_OK) {
        LOG(ERROR) << "ASDV_ServiceUnitDefinition_getUnitMetadataType failed with status "
                   << status;
        return std::monostate();
    }

    switch (md_type) {
        case ASDV_UNIT_METADATA_TYPE_SERVER:
            return GetServerUnitMetadata();
        case ASDV_UNIT_METADATA_TYPE_PUBLISHER:
            return GetPublisherUnitMetadata();
    }

    return std::monostate();
}

// Returns the Application Metadata for the Service Unit.
std::vector<uint8_t> ServiceUnitDefinition::GetApplicationMetadata() const {
    // First get the metadata size.
    size_t md_size;
    auto status = ASDV_ServiceUnitDefinition_getApplicationMetadataSize(handle_, &md_size);
    if (status != ASDV_STATUS_OK) {
        LOG(ERROR) << "ASDV_ServiceUnitDefinition_getApplicationMetadataSize failed with status "
                   << status;
        return std::vector<uint8_t>();
    }

    // No data to retrieve, just return empty vector.
    if (md_size == 0) {
        return std::vector<uint8_t>();
    }

    // Allocate appropriately-sized vector and fetch metadata.
    std::vector<uint8_t> metadata(md_size, 0);
    status = ASDV_ServiceUnitDefinition_getApplicationMetadata(handle_, metadata.data(),
                                                               metadata.size());
    if (status != ASDV_STATUS_OK) {
        LOG(ERROR) << "ASDV_ServiceUnitDefinition_getApplicationMetadata failed with status "
                   << status;
        return std::vector<uint8_t>();
    }

    return metadata;
}

// Get the ServerUnitMetadata for the Service Unit.
// Returned object could be unpopulated; use IsValid() to check.
ServerUnitMetadata ServiceUnitDefinition::GetServerUnitMetadata() const {
    ServerUnitMetadata metadata = ServerUnitMetadata::Create();

    if (!metadata.IsValid()) {
        LOG(ERROR) << "Error creating ASDV_ServerUnitMetadata.  Returning empty instance.";
        return metadata;
    }

    auto status =
            ASDV_ServiceUnitDefinition_getServerUnitMetadata(handle_, metadata.ToMutableABIPtr());
    if (status != ASDV_STATUS_OK) {
        LOG(ERROR) << "ASDV_ServiceUnitDefinition_getServerUnitMetadata failed with status "
                   << status;
        return ServerUnitMetadata(nullptr);
    }

    return metadata;
}

// Get the PublisherUnitMetadata for the Service Unit.
// Returned object could be unpopulated; use IsValid() to check.
PublisherUnitMetadata ServiceUnitDefinition::GetPublisherUnitMetadata() const {
    PublisherUnitMetadata metadata = PublisherUnitMetadata::Create();

    if (!metadata.IsValid()) {
        LOG(ERROR) << "Error creating ASDV_PublisherUnitMetadata.  Returning empty instance.";
        return metadata;
    }

    auto status = ASDV_ServiceUnitDefinition_getPublisherUnitMetadata(handle_,
                                                                      metadata.ToMutableABIPtr());
    if (status != ASDV_STATUS_OK) {
        LOG(ERROR) << "ASDV_ServiceUnitDefinition_getPublisherUnitMetadata failed with status "
                   << status;
        return PublisherUnitMetadata(nullptr);
    }

    return metadata;
}

}  // namespace sdv_comms::sd
