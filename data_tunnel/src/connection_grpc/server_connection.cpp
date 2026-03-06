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

GrpcServerConnection* GrpcServerConnection::Start(ConnectionManager::EventHandler& event_handler,
                                                  grpc::GenericCallbackServerContext* context) {
    auto conn =
            std::unique_ptr<GrpcServerConnection>(new GrpcServerConnection(event_handler, context));

    // Trigger explicit send of initial metadata so that we'll get an OnSendInitialMetadataDone
    // event to tell us when the connection is ready and we can pass it to the event handler.
    conn->StartSendInitialMetadata();

    // Not a leak:  GRPC will receive this pointer and guarantee that OnDone is called.
    return conn.release();
}

GrpcServerConnection::GrpcServerConnection(ConnectionManager::EventHandler& event_handler,
                                           grpc::GenericCallbackServerContext* context) :
      GrpcConnectionBase(),
      event_handler_(event_handler),
      peer_(context->peer()),
      context_(context) {}

std::string GrpcServerConnection::ToString() const {
    return std::format("GrpcServerConnection({})", peer_);
}

void GrpcServerConnection::RequestDisconnect() {
    context_->TryCancel();
}

std::shared_ptr<const grpc::AuthContext> GrpcServerConnection::GetAuthContext() const {
    return context_->auth_context();
}

void GrpcServerConnection::OnSendInitialMetadataDone(bool success) {
    if (!success) {
        ALOGW("%s: Failure establishing connection", ToString().c_str());
        // Release hold to allow connection to finish.
        Finish(grpc::Status::CANCELLED);
        return;
    }

    ALOGI("%s: Connection established", ToString().c_str());

    // Since connect succeeded, send the connection to the client application.
    event_handler_.OnConnectionManagerEvent(IConnectionHolder(this, [](IConnection* iconn) {
        // This releaser function must not block, and we need to keep the connection around until
        // GRPC is finished with it.  So instead of deleting it right away we tell GRPC to finish it
        // done with it, so that GRPC will finalize it and call OnDone, which can then delete.

        // Get typed pointer and verify that all read/write operations are terminated.
        auto* gconn = static_cast<GrpcServerConnection*>(iconn);
        CHECK(gconn->TryTerminateAllOperations());

        // This will lead to GRPC calling OnDone.
        gconn->Finish(grpc::Status::CANCELLED);
    }));
}

void GrpcServerConnection::OnDone() {
    // GRPC is done with the reactor so we can delete it.
    delete this;
}

}  // namespace android::sdv::dt::agent
