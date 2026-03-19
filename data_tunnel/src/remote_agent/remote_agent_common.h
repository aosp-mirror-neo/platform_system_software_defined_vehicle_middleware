/*
 * Copyright (C) 2026 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#pragma once

// Common utilities and interfaces shared among RemoteAgent classes.

#include "config.h"
#include "connection.h"
#include "proto/dt_proto.h"

#include <aidl/google/sdv/identity/ServiceFqin.h>
#include <aidl/google/sdv/service_discovery/common/UnitType.h>
#include <android/binder_manager.h>

#include <atomic>

namespace android::sdv::dt::agent {

// Used for control messages (connection + subscription setup / teardown), which have only VM name
// or topic name as variable length fields and are limited such that 4KB will always be plenty.
constexpr size_t kDefaultBufferSize = 4 * 1024;

extern const ::aidl::google::sdv::service_discovery::common::UnitType kDtAgentUnitType;

std::string MakeFqinString(const ::aidl::google::sdv::identity::ServiceFqin& fqin);

// Send the given payload over the given connection, using data_buffer for scratch space.
bool SendPayload(IConnection& conn, const proto::Payload& payload, RawBuffer& data_buffer);

// Try to Receive a payload from the given parcel, using writer as scratch space.
std::pair<proto::Parcel::ParseResult, std::unique_ptr<proto::Payload>> ReceivePayload(
        IConnection& conn);

}  // namespace android::sdv::dt::agent
