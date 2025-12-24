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

#define LOG_TAG "sdv_dt_agent:read_data"

#include "read_data.h"

#include <android-base/logging.h>
#include <grpc/grpc.h>
#include <grpc/grpc_security_constants.h>
#include <grpcpp/channel.h>
#include <grpcpp/client_context.h>
#include <grpcpp/create_channel.h>
#include <grpcpp/generic/async_generic_service.h>
#include <grpcpp/generic/generic_stub.h>
#include <grpcpp/server_builder.h>
#include <grpcpp/support/client_callback.h>
#include <log/log.h>

namespace android::sdv::dt::agent {

std::optional<ReadData> ReadData::Extract(grpc::ByteBuffer& buffer) {
    ReadData result;

    // Try to extract the data as a single slice without copying if possible.
    // Otherwise fall back to DumpToSingleSlice which copies to produce a single slice.
    if (!buffer.TrySingleSlice(&result.data).ok() && !buffer.DumpToSingleSlice(&result.data).ok()) {
        ALOGE("OnReadDone() unable to dump buffer to single slice.  Forcing terminate.");
        return std::nullopt;
    } else if (result.data.size() == 0) {
        ALOGE("OnReadDone() got success but zero bytes.  Forcing terminate.");
        return std::nullopt;
    }

    return std::move(result);
}

size_t ReadData::Consume(void* buffer, size_t buffer_size) {
    // Consume remainder of data or buffer_size bytes, whichever is smaller.
    size_t read_len = std::min(buffer_size, data.size() - consumed);
    memcpy(buffer, data.begin() + consumed, read_len);
    consumed += read_len;
    return read_len;
}

bool ReadData::Exhausted() {
    return consumed >= data.size();
}

}  // namespace android::sdv::dt::agent
