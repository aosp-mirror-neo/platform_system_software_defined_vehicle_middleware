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

#include "../certs.h"
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

#include <chrono>

namespace android::sdv::dt::agent {

using ClientGenericBidiReactor = grpc::ClientBidiReactor<grpc::ByteBuffer, grpc::ByteBuffer>;

// Client-side specialization of GrpcConnectionBase.
class GrpcClientConnection : public GrpcConnectionBase<ClientGenericBidiReactor> {
public:
    static void Start(ConnectionManager::EventHandler& event_handler, const SocketAddressV4& addr,
                      const std::string& target,
                      std::shared_ptr<grpc::ChannelCredentials> credentials);

    // Returns a string representation of the connection.
    virtual std::string ToString() const;

    // Lifecycle operation required by IConnection.
    virtual void RequestDisconnect() override;

private:
    GrpcClientConnection(ConnectionManager::EventHandler& event_handler,
                         const SocketAddressV4& addr, const std::string& target,
                         std::shared_ptr<grpc::Channel> channel);

    std::shared_ptr<const grpc::AuthContext> GetAuthContext() const;

    // Called by GRPC when the initial metadata has been read and connection may be ready for use.
    void OnReadInitialMetadataDone(bool success) override;

    // Called when GRPC is done with the connection.
    virtual void OnDone(const grpc::Status& /*status*/) override;

    // We don't really use the method name for anything.  Grpc requires it when creating generic
    // connections as a client, since there's no service definition providing method names.
    static constexpr const char kMethodName[] = "SdvDtAgent";

    // How long to wait after a connection failure before trying again.
    static constexpr auto kConnectRetryDelay = std::chrono::seconds(1);

    ConnectionManager::EventHandler& event_handler_;
    SocketAddressV4 peer_;
    std::string target_;
    std::unique_ptr<grpc::GenericStub> stub_;
    std::unique_ptr<grpc::ClientContext> context_;
};

}  // namespace android::sdv::dt::agent
