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
#include "connection_base.h"

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

// Server-side specialization of GrpcConnectionBase.
class GrpcServerConnection : public GrpcConnectionBase<grpc::ServerGenericBidiReactor> {
public:
    // Constructor for GrpcServerConnection.
    GrpcServerConnection(ConnectionManager::EventHandler& event_handler,
                         grpc::GenericCallbackServerContext* context);

    // Destructor for GrpcServerConnection.
    virtual ~GrpcServerConnection();

    // Returns a string representation of the connection.
    std::string ToString() const;

private:
    // Server-specific implementations of Grpc lifecycle operations required by GrpcConnectionBase.
    void DoGrpcTryCancel();
    void DoGrpcFinish();
    std::shared_ptr<const grpc::AuthContext> GetAuthContext() const;

    // Forward Server-specific Grpc lifecycle events to GrpcConnectionBase.
    virtual void OnSendInitialMetadataDone(bool success) override;
    virtual void OnDone() override;

    std::string peer_;
    grpc::GenericCallbackServerContext* context_;
};

}  // namespace android::sdv::dt::agent
