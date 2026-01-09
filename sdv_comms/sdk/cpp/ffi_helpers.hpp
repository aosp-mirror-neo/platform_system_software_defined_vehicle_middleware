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

#include <sdv/service_fqin.h>
#include <sdv/service_identity.h>
#include <sdv/unit_discovery.h>
#include <sdv/unit_metadata.h>
#include <sdv/unit_registration.h>
#include <sdv/unit_type.h>

#include <sdv/identity.hpp>
#include <sdv/sd_common.hpp>
#include <sdv/sd_lookup.hpp>

// Conversion functions to extract FFI-typed record ptrs
// from C++ SDK wrappers.

namespace sdv_comms {

template <size_t BUF_SIZE>
inline bool PopulateUCharBufFromString(unsigned char (&dest)[BUF_SIZE], const std::string& src) {
    if (src.length() + 1 > sizeof(dest)) {
        return false;
    }
    memcpy(dest, src.data(), src.length() + 1);
    return true;
}

inline ASDV_ServiceFqin* ToFFIMutable(id::ServiceFqin& fqin) {
    return reinterpret_cast<ASDV_ServiceFqin*>(&fqin);
}

inline const ASDV_ServiceFqin* ToFFI(const id::ServiceFqin& fqin) {
    return reinterpret_cast<const ASDV_ServiceFqin*>(&fqin);
}

inline ASDV_ServicePublicKey* ToFFIMutable(id::ServicePublicKey& si) {
    return reinterpret_cast<ASDV_ServicePublicKey*>(&si);
}

inline const ASDV_ServicePublicKey* ToFFI(const id::ServicePublicKey& pk) {
    return reinterpret_cast<const ASDV_ServicePublicKey*>(&pk);
}

inline ASDV_ServiceIdentity* ToFFIMutable(id::ServiceIdentity& si) {
    return reinterpret_cast<ASDV_ServiceIdentity*>(&si);
}

inline const ASDV_ServiceIdentity* ToFFI(const id::ServiceIdentity& si) {
    return reinterpret_cast<const ASDV_ServiceIdentity*>(&si);
}

inline ASDV_UnitIdentifier* ToFFIMutable(id::UnitIdentifier& ui) {
    return reinterpret_cast<ASDV_UnitIdentifier*>(&ui);
}

inline const ASDV_UnitIdentifier* ToFFI(const id::UnitIdentifier& ui) {
    return reinterpret_cast<const ASDV_UnitIdentifier*>(&ui);
}

inline ASDV_UnitType* ToFFIMutable(sd::UnitType& ut) {
    return reinterpret_cast<ASDV_UnitType*>(&ut);
}

inline const ASDV_UnitType* ToFFI(const sd::UnitType& ut) {
    return reinterpret_cast<const ASDV_UnitType*>(&ut);
}

inline ASDV_ServerUnitMetadata* ToFFIMutable(sd::ServerUnitMetadata& sum) {
    return *reinterpret_cast<ASDV_ServerUnitMetadata**>(&sum);
}

inline const ASDV_ServerUnitMetadata* ToFFI(const sd::ServerUnitMetadata& sum) {
    return *reinterpret_cast<ASDV_ServerUnitMetadata* const*>(&sum);
}

inline ASDV_PublisherUnitMetadata* ToFFIMutable(sd::PublisherUnitMetadata& pum) {
    return *reinterpret_cast<ASDV_PublisherUnitMetadata**>(&pum);
}

inline const ASDV_PublisherUnitMetadata* ToFFI(const sd::PublisherUnitMetadata& pum) {
    return *reinterpret_cast<ASDV_PublisherUnitMetadata* const*>(&pum);
}

inline ASDV_UnitNameDiscoveryArgs* ToFFIMutable(sd::UnitNameDiscoveryArgs& unda) {
    return reinterpret_cast<ASDV_UnitNameDiscoveryArgs*>(&unda);
}

inline const ASDV_UnitNameDiscoveryArgs* ToFFI(const sd::UnitNameDiscoveryArgs& unda) {
    return reinterpret_cast<const ASDV_UnitNameDiscoveryArgs*>(&unda);
}

}  // namespace sdv_comms
