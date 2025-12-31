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

bool SendPayload(IConnection& conn, const proto::Payload& payload, uint32_t parcel_id,
                 RawBuffer& data_buffer) {
    proto::Parcel parcel(parcel_id);
    BufferWriter writer(data_buffer);
    if (!parcel.Serialize(writer, payload)) {
        ALOGE("Unable to Serialize payload %u bytes", payload.Size());
        return false;
    }
    uint32_t parcel_size = parcel.ParcelSize(payload);
    return conn.SendData(data_buffer.Ptr(), parcel_size);
}

// TODO(b/429464380): Make IConnection::ReceiveData message-oriented and simplify this
std::pair<proto::Parcel::ParseResult, std::unique_ptr<proto::Payload>> ReceivePayload(
        IConnection& conn, uint32_t* parcel_id, BufferWriter& writer) {
    BufferReader reader(writer);
    while (true) {
        reader.SetAvailableToRead(writer.GetPos());
        if (reader.AvailableToRead()) {
            auto payload_result = proto::Parcel::Deserialize(reader, parcel_id);
            if (payload_result.first == proto::Parcel::ParseResult::Success) {
                if (reader.AvailableToRead()) {
                    // There is still some data left
                    writer.CutAndMove(reader.GetPos());
                } else {
                    // Nothing left to parse. Reset the writer's pointer
                    writer.Reset();
                }
                return payload_result;
            } else if (payload_result.first == proto::Parcel::ParseResult::NeedMoreData) {
                if (!writer.Available()) {
                    // Writer buffer isn't big enough to fit the message
                    size_t current_size = writer.Buffer().Size();
                    writer.Inflate(current_size);
                    size_t new_size = writer.Buffer().Size();
                    ALOGD("Payload buffer is too big to fit into the local %zu bytes receive "
                          "buffer. Resizing to %zu.",
                          current_size, new_size);
                }
            } else if (payload_result.first == proto::Parcel::ParseResult::UnknownPayload) {
                ALOGW("Received an unknown payload type parcel. Ignoring...");
                return payload_result;
            } else {
                ALOGE("Unable to deserialize the received parcel!");
                return payload_result;
            }
        }

        size_t bytes_read = conn.ReceiveData(writer.CurPtr(), writer.Available());
        if (bytes_read > 0) {
            writer.MovePos(static_cast<size_t>(bytes_read));
        } else {
            ALOGE("IConnection::ReceiveData() returned 0");
            if (parcel_id) *parcel_id = 0;
            return {proto::Parcel::ParseResult::Failure, nullptr};
        }
    }
}

}  // namespace android::sdv::dt::agent
