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

GrpcClientConnection::GrpcClientConnection(ConnectionManager::EventHandler& event_handler,
                                           const SocketAddressV4& addr, const std::string& target,
                                           std::shared_ptr<grpc::Channel> channel) :
      GrpcConnectionBase(event_handler),
      peer_(addr),
      target_(target),
      channel_(channel),
      stub_(std::make_unique<grpc::GenericStub>(channel_)),
      context_(std::make_unique<grpc::ClientContext>()) {
    stub_->PrepareBidiStreamingCall(context_.get(), kMethodName, grpc::StubOptions(), this);

    // GRPC requires "holds" if calling StartRead / StartWrite from outside of one of the
    // On* event handlers on the client reactor.  See:
    //
    // https://grpc.github.io/grpc/cpp/classgrpc_1_1_client_bidi_reactor.html#a88c169b6b8d9ea5579d152a86b74582c
    //
    // We do call StartRead / StartWrite from outside those handlers, so we add a hold here
    // that will then be removed in DoGrpcFinish() during Disconnect or failed connection
    // cleanup.
    ClientGenericBidiReactor::AddHold();

    StartCall();
}

// Need to call Disconnect() from here because it needs to call the GrpcClientConnection
// implementations of DoGrpc*; if we called it from ~GrpcConnectionBase it'd run after this
// destructor, when those methods are no longer available.
GrpcClientConnection::~GrpcClientConnection() {
    Disconnect();
}

std::string GrpcClientConnection::ToString() const {
    return std::format("GrpcClientConnection({})", peer_.ToString());
}

void GrpcClientConnection::DoGrpcTryCancel() {
    context_->TryCancel();
}
void GrpcClientConnection::DoGrpcFinish() {
    ClientGenericBidiReactor::RemoveHold();
}
std::shared_ptr<const grpc::AuthContext> GrpcClientConnection::GetAuthContext() const {
    return context_->auth_context();
}

void GrpcClientConnection::OnReadInitialMetadataDone(bool success) {
    ReportGrpcConnectResult(success);
}

void GrpcClientConnection::OnDone(const grpc::Status& /*status*/) {
    ReportGrpcDone();
}

}  // namespace android::sdv::dt::agent
