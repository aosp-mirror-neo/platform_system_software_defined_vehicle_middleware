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

// Manages data from a single Grpc read operation.
//
// Grpc's read operations are message-oriented, returning a whole block of data from
// a single SendData() call by a peer. If the buffer provided to ReceiveData() is
// not large enough to hold the entire message, this class holds onto the
// leftover data to be returned on subsequent calls.
class ReadData {
public:
    // Extracts data from a Grpc ByteBuffer and creates a ReadData object.
    // This is a factory method.
    static std::optional<ReadData> Extract(grpc::ByteBuffer& buffer);

    // Copies data from the internal buffer to the user-provided buffer.
    size_t Consume(void* buffer, size_t buffer_size);

    // Checks if all data from the internal buffer has been consumed.
    bool Exhausted();

private:
    grpc::Slice data;
    size_t consumed = 0;
};

}  // namespace android::sdv::dt::agent
