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

#define LOG_TAG "sdv_dt_agent:client_authenticator"

#include "client_authenticator.h"

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

grpc::Status ClientAuthenticator::Process(const grpc::AuthMetadataProcessor::InputMetadata&,
                                          grpc::AuthContext* context,
                                          grpc::AuthMetadataProcessor::OutputMetadata*,
                                          grpc::AuthMetadataProcessor::OutputMetadata*) {
    auto certs = context->FindPropertyValues(GRPC_X509_PEM_CERT_PROPERTY_NAME);
    ALOGD("Authenticating incoming connection; checking %zu client certs", certs.size());
    for (auto cert : certs) {
        if (roots_accessor_.IsSignedByTrustedRoot(std::string_view(cert.data(), cert.length()))) {
            ALOGD("Cert passed auth; returning OK");
            return grpc::Status::OK;
        }
    }
    ALOGD("No client certs signed by any trusted root.  Returning UNAUTHENTICATED.");

    return grpc::Status(grpc::StatusCode::UNAUTHENTICATED,
                        "Certificate provided by peer not signed by a known trusted root.");
}

}  // namespace android::sdv::dt::agent
