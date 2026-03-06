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

#define LOG_TAG "sdv_dt_agent:connection_grpc:client_connection"

#include "client_connection.h"

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

#include <chrono>

namespace android::sdv::dt::agent {

void GrpcClientConnection::Start(ConnectionManager::EventHandler& event_handler,
                                 const SocketAddressV4& addr, const std::string& target,
                                 std::shared_ptr<grpc::ChannelCredentials> credentials) {
    grpc::ChannelArguments channel_args;
    channel_args.SetSslTargetNameOverride(EncodeNameForCert(target));
    // Allow for unlimited message size.
    channel_args.SetMaxSendMessageSize(-1);
    channel_args.SetMaxReceiveMessageSize(-1);
    auto channel = grpc::CreateCustomChannel(addr.ToString(), std::move(credentials), channel_args);

    auto conn = std::unique_ptr<GrpcClientConnection>(
            new GrpcClientConnection(event_handler, addr, target, std::move(channel)));

    conn->stub_->PrepareBidiStreamingCall(conn->context_.get(), kMethodName, grpc::StubOptions(),
                                          conn.get());

    // GRPC requires "holds" if calling StartRead / StartWrite from outside of one of the
    // On* event handlers on the client reactor.  See:
    //
    // https://grpc.github.io/grpc/cpp/classgrpc_1_1_client_bidi_reactor.html#a88c169b6b8d9ea5579d152a86b74582c
    //
    // We do call StartRead / StartWrite from outside those handlers, so we add a hold here
    // that will then be removed in DoGrpcFinish() during Disconnect or failed connection
    // cleanup.
    conn->AddHold();

    conn->StartCall();

    // Not a leak: After StartCall GRPC has ownership of the GrpcClientConnection object and
    // guarantees that OnDone will be called on it.
    conn.release();
}

GrpcClientConnection::GrpcClientConnection(ConnectionManager::EventHandler& event_handler,
                                           const SocketAddressV4& addr, const std::string& target,
                                           std::shared_ptr<grpc::Channel> channel) :
      GrpcConnectionBase(),
      event_handler_(event_handler),
      peer_(addr),
      target_(target),
      stub_(std::make_unique<grpc::GenericStub>(std::move(channel))),
      context_(std::make_unique<grpc::ClientContext>()) {}

std::string GrpcClientConnection::ToString() const {
    return std::format("GrpcClientConnection({})", peer_.ToString());
}

void GrpcClientConnection::RequestDisconnect() {
    context_->TryCancel();
}

std::shared_ptr<const grpc::AuthContext> GrpcClientConnection::GetAuthContext() const {
    return context_->auth_context();
}

void GrpcClientConnection::OnReadInitialMetadataDone(bool success) {
    if (!success) {
        ALOGW("%s: Failure establishing connection", ToString().c_str());
        event_handler_.OnConnectionManagerEvent(
                base::unexpected(::sdv::SdvStatus(::sdv::SdvStatusCode::kUnavailable)));
        RemoveHold();  // Release hold to allow finish.
        return;
    }

    ALOGI("%s: Connection established", ToString().c_str());

    // Since connect succeeded, send the connection to the client application.
    event_handler_.OnConnectionManagerEvent(IConnectionHolder(this, [](IConnection* iconn) {
        // This releaser function must not block, and we need to keep the connection around until
        // GRPC is finished with it.  So instead of deleting it right away we tell GRPC that we're
        // done with it, so that GRPC will finalize it and call OnDone, which can then delete.

        // Get typed connection and verify that all read/write operations are terminated.
        auto* gconn = static_cast<GrpcClientConnection*>(iconn);
        CHECK(gconn->TryTerminateAllOperations());

        // Release hold to allow GRPC to finish connection; this wll lead to OnDone.
        gconn->RemoveHold();
    }));
}

void GrpcClientConnection::OnDone(const grpc::Status& /*status*/) {
    // GRPC is done with the reactor so we can delete it.
    delete this;
}

}  // namespace android::sdv::dt::agent
