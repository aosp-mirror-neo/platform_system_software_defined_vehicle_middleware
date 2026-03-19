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

#define LOG_TAG "sdv_dt_agent:remote_agent"

#include "remote_agent/remote_agent_common.h"

#include "certs.h"
#include "config.h"

#include <log/log.h>

#include <iostream>

namespace android::sdv::dt::agent {

using ::aidl::google::sdv::identity::ServiceFqin;
using ::aidl::google::sdv::service_discovery::common::UnitType;

const UnitType kDtAgentUnitType = UnitType{/*sdvPackageName*/ config::kPackageName,
                                           /*serviceBundleName*/ config::kServiceBundleName,
                                           /*typeName*/ "DataTunnelAgentUnitType"};

// The FQIN string form is used as the name of the service in any associated certificates.
std::string MakeFqinString(const ServiceFqin& fqin) {
    return fqin.sdvVmName + ":" + fqin.sdvPackageName + "." + fqin.serviceBundleName + "/" +
            fqin.serviceInstanceName;
}

bool SendPayload(IConnection& conn, const proto::Payload& payload, RawBuffer& data_buffer) {
    proto::Parcel parcel;
    BufferWriter writer(data_buffer);
    if (!parcel.Serialize(writer, payload)) {
        ALOGE("Unable to Serialize payload %u bytes", proto::Parcel::ParcelSize(payload));
        return false;
    }
    uint32_t parcel_size = parcel.ParcelSize(payload);
    return conn.SendData(data_buffer.Ptr(), parcel_size);
}

std::pair<proto::Parcel::ParseResult, std::unique_ptr<proto::Payload>> ReceivePayload(
        IConnection& conn) {
    auto received = conn.ReceiveData();
    if (!received) {
        ALOGE("IConnection::ReceiveData() returned empty result");
        return {proto::Parcel::ParseResult::Failure, nullptr};
    }

    BufferReader reader(received.span());

    auto payload_result = proto::Parcel::Deserialize(reader);

    if (payload_result.first == proto::Parcel::ParseResult::Failure) {
        ALOGE("ReceivePayload(): Got malformed parcel from connection!");
        return {proto::Parcel::ParseResult::Failure, nullptr};
    }

    if (payload_result.first == proto::Parcel::ParseResult::UnknownPayload) {
        ALOGE("ReceivePayload(): Got unknown payload type from connection!");
        return {proto::Parcel::ParseResult::UnknownPayload, nullptr};
    }

    // Result was success

    if (reader.AvailableToRead()) {
        ALOGE("ReceivePayload(): Got payload with extra data (%zu bytes) from connection!",
              reader.AvailableToRead());
    }

    return payload_result;
}

}  // namespace android::sdv::dt::agent
