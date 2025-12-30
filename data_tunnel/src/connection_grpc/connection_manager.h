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
#include "client_authenticator.h"
#include "client_connection.h"
#include "server_connection.h"

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

#include <algorithm>
#include <memory>

namespace android::sdv::dt::agent {

class GrpcConnectionManager : public ConnectionManager, public grpc::CallbackGenericService {
public:
    // Constructor for GrpcConnectionManager.
    GrpcConnectionManager(IPAddressV4& server_ip,
                          std::optional<ConnectionManager::SslConfig> ssl_config);

    // Shuts down the gRPC server.
    virtual void Shutdown() override;

    // Starts listening for incoming connections.
    virtual bool Listen(EventHandler& event_handler) override;

    // Initiates a connection to a remote endpoint.
    virtual void Connect(const SocketAddressV4& addr, const std::string& target,
                         EventHandler& event_handler) override;

    virtual ~GrpcConnectionManager();

private:
    // Called by Grpc framework for each incoming connection.
    // Called by Grpc framework for each incoming connection.
    grpc::ServerGenericBidiReactor* CreateReactor(
            grpc::GenericCallbackServerContext* context) override;
    // Creates server credentials based on the provided SSL configuration.
    std::shared_ptr<grpc::ServerCredentials> MakeServerCredentials(
            const std::optional<ConnectionManager::SslConfig>& ssl_config);

    std::shared_ptr<grpc::ChannelCredentials> MakeClientCredentials(
            const std::optional<ConnectionManager::SslConfig>& ssl_config);

    EventHandler* listen_event_handler_ = nullptr;
    std::unique_ptr<grpc::Server> server_ = nullptr;
    std::optional<SslConfig> ssl_config_ = std::nullopt;
};

// Creates a GrpcConnectionManager instance.
std::unique_ptr<ConnectionManager> CreateGrpcConnectionManager(
        IPAddressV4 listen_address, std::optional<ConnectionManager::SslConfig> ssl_config);

}  // namespace android::sdv::dt::agent
