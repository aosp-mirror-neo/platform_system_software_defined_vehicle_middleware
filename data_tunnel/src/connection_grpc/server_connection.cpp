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

#define LOG_TAG "sdv_dt_agent:connection_grpc:server_connection"

#include "server_connection.h"

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

GrpcServerConnection::GrpcServerConnection(ConnectionManager::EventHandler& event_handler,
                                           grpc::GenericCallbackServerContext* context) :
      GrpcConnectionBase(event_handler), peer_(context->peer()), context_(context) {
    // Trigger explicit send of initial metadata so that we'll get an OnSendInitialMetadataDone
    // event to tell us when the connection is ready and we can pass it to the event handler.
    StartSendInitialMetadata();
}

// Need to call Disconnect() from here because it needs to call the GrpcServerConnection
// implementations of DoGrpc*; if we called it from ~GrpcConnectionBase it'd run after this
// destructor when those methods were no longer available.
GrpcServerConnection::~GrpcServerConnection() {
    Disconnect();
}

std::string GrpcServerConnection::ToString() const {
    return std::format("GrpcServerConnection({})", peer_);
}

void GrpcServerConnection::DoGrpcTryCancel() {
    context_->TryCancel();
}

void GrpcServerConnection::DoGrpcFinish() {
    Finish(grpc::Status::CANCELLED);
}

std::shared_ptr<const grpc::AuthContext> GrpcServerConnection::GetAuthContext() const {
    return context_->auth_context();
}

void GrpcServerConnection::OnSendInitialMetadataDone(bool success) {
    ReportGrpcConnectResult(success);
}

void GrpcServerConnection::OnDone() {
    ReportGrpcDone();
}

}  // namespace android::sdv::dt::agent
