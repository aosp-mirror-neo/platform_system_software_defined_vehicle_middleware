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

#define LOG_TAG "sdv_dt_agent:connection_grpc:connection_manager"

#include "connection_manager.h"

#include "../certs.h"
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

constexpr std::chrono::seconds kGrpcShutdownTimeout = std::chrono::seconds(1);

GrpcConnectionManager::GrpcConnectionManager(
        IPAddressV4& server_ip, std::optional<ConnectionManager::SslConfig> ssl_config) :
      ConnectionManager(server_ip), ssl_config_(std::move(ssl_config)) {}

GrpcConnectionManager::~GrpcConnectionManager() {
    Shutdown();
}

void GrpcConnectionManager::Shutdown() {
    if (server_) {
        // Shutdown with a deadline to avoid blocking forever if there are active RPCs.
        // 1 second should be enough for graceful shutdown.
        // When the timeout expires, all remaining connections are forcibly cancelled.
        server_->Shutdown(std::chrono::system_clock::now() + kGrpcShutdownTimeout);
    }
}

bool GrpcConnectionManager::Listen(EventHandler& event_handler) {
    if (server_) {
        ALOGE("Unsupported second call to Listen() on GrpcConnectionManager!");
        return false;
    }

    // Setup server.
    listen_event_handler_ = &event_handler;
    grpc::ServerBuilder builder;

    builder.RegisterCallbackGenericService(this);
    // Allow for unlimited message size.
    builder.SetMaxSendMessageSize(-1);
    builder.SetMaxReceiveMessageSize(-1);

    // Tell Grpc to pick a listen port for us.
    auto server_address = GetLocalIp().ToString() + ":0";
    int listen_port = 0;
    builder.AddListeningPort(server_address, MakeServerCredentials(ssl_config_), &listen_port);
    auto server = builder.BuildAndStart();

    if (!server) {
        ALOGE("Failed to start GRPC Server!");
        return false;
    }

    if (listen_port < 0 || listen_port > 65536) {
        ALOGE("GRPC Server selected invalid port!");
        return false;
    }

    server_ = std::move(server);
    listen_port_ = listen_port;
    ALOGI("GrpcConnectionManager listening on %s:%d", GetLocalIp().ToString().c_str(), listen_port);
    return true;
}

void GrpcConnectionManager::Connect(const SocketAddressV4& addr, const std::string& target,
                                    EventHandler& event_handler) {
    ALOGI("GrpcConnectionManager starting connection to %s at %s", target.c_str(),
          addr.ToString().c_str());

    auto client_creds = MakeClientCredentials(ssl_config_);

    grpc::ChannelArguments channel_args;
    channel_args.SetSslTargetNameOverride(EncodeNameForCert(target));
    // Allow for unlimited message size.
    channel_args.SetMaxSendMessageSize(-1);
    channel_args.SetMaxReceiveMessageSize(-1);
    auto channel =
            grpc::CreateCustomChannel(addr.ToString(), std::move(client_creds), channel_args);

    // GrpcClientConnection constructor sets up the call and manages its own
    // lifetime until it's sends itself to the event handler on successful setup.
    new GrpcClientConnection(event_handler, addr, target, std::move(channel));
}

grpc::ServerGenericBidiReactor* GrpcConnectionManager::CreateReactor(
        grpc::GenericCallbackServerContext* context) {
    CHECK(listen_event_handler_) << "Got request for reactor with no event handler present!";
    // GrpcServerConnection constructor sets up the call and manages its own
    // lifetime until it's sent to the event handler on successful setup.
    return new GrpcServerConnection(*listen_event_handler_, context);
}

std::shared_ptr<grpc::ServerCredentials> GrpcConnectionManager::MakeServerCredentials(
        const std::optional<ConnectionManager::SslConfig>& ssl_config) {
    if (!ssl_config) {
        return grpc::InsecureServerCredentials();
    }

    // We use GRPC_SSL_REQUEST_AND_REQUIRE_CLIENT_CERTIFICATE_BUT_DONT_VERIFY because we want
    // GRPC to require a valid certificate / key pair from the client, but can't depend on it to
    // verify that certificate against the trusted root set, because GRPC provides no means to
    // update the set of trusted roots it will use for verification after server start up.
    //
    // Instead we do the verification against trusted roots by installing an authentication metadata
    // processor, the ClientAuthenticator class defined above.
    //
    // See https://grpc.github.io/grpc/core/grpc__security__constants_8h.html
    auto ssl_cred_opts = grpc::SslServerCredentialsOptions(
            GRPC_SSL_REQUEST_AND_REQUIRE_CLIENT_CERTIFICATE_BUT_DONT_VERIFY);
    ssl_cred_opts.pem_key_cert_pairs = {
            {.private_key = ssl_config->pem_private_key, .cert_chain = ssl_config->pem_cert_chain}};
    auto result = grpc::SslServerCredentials(ssl_cred_opts);
    auto auth_metadata_processor =
            std::make_shared<ClientAuthenticator>(ssl_config->trusted_roots_accessor);
    result->SetAuthMetadataProcessor(std::move(auth_metadata_processor));
    return result;
}

std::shared_ptr<grpc::ChannelCredentials> GrpcConnectionManager::MakeClientCredentials(
        const std::optional<ConnectionManager::SslConfig>& ssl_config) {
    if (!ssl_config) {
        return grpc::InsecureChannelCredentials();
    }

    return grpc::SslCredentials(grpc::SslCredentialsOptions{
            .pem_root_certs = ssl_config->trusted_roots_accessor.GetAllTrustedRootsAsPEM(),
            .pem_private_key = ssl_config->pem_private_key,
            .pem_cert_chain = ssl_config->pem_cert_chain,
    });
}

std::unique_ptr<ConnectionManager> CreateGrpcConnectionManager(
        IPAddressV4 listen_address, std::optional<ConnectionManager::SslConfig> ssl_config) {
    return std::make_unique<GrpcConnectionManager>(listen_address, ssl_config);
}

}  // namespace android::sdv::dt::agent
