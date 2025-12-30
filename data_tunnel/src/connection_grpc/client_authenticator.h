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

#include "../connection.h"

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

class ClientAuthenticator : public grpc::AuthMetadataProcessor {
public:
    // Constructor for the ClientAuthenticator.
    explicit ClientAuthenticator(const TrustedRootsAccessor& roots_accessor) :
          roots_accessor_(roots_accessor) {}

    // Deleted default constructor to prevent construction without a TrustedRootsAccessor.
    ClientAuthenticator() = delete;

    // Processes authentication metadata from an incoming connection to determine if it should be
    // accepted.
    //
    // This method inspects the provided certificate chain to determine if it is signed by a
    // trusted root.
    grpc::Status Process(const grpc::AuthMetadataProcessor::InputMetadata&,
                         grpc::AuthContext* context, grpc::AuthMetadataProcessor::OutputMetadata*,
                         grpc::AuthMetadataProcessor::OutputMetadata*) override;

private:
    const TrustedRootsAccessor& roots_accessor_;
};

}  // namespace android::sdv::dt::agent
