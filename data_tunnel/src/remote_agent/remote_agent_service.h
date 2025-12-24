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

#include "authorities_listener.h"
#include "connection.h"
#include "remote_agent/remote_agent_client_manager.h"
#include "remote_agent/remote_agent_common.h"
#include "remote_agent/remote_agent_server.h"
#include "remote_agent_service_interface.h"
#include "topic_manager.h"

#include <aidl/google/sdv/ca/ICertificateAuthority.h>
#include <aidl/google/sdv/identity/IIdentityAgent.h>
#include <aidl/google/sdv/service_discovery/discovery/IServiceDiscoveryAgent.h>
#include <aidl/google/sdv/service_discovery/discovery/IServiceRegistrationAgent.h>
#include <aidl/google/sdv/service_discovery/discovery/ITransportSupportAgent.h>

#include <ostream>
#include <shared_mutex>

namespace android::sdv::dt::agent {

class RemoteAgentService final : public IRemoteAgentService {
public:
    // References to this objects' fields are given to each other move is unsafe.
    RemoteAgentService(RemoteAgentService&&) = delete;
    ~RemoteAgentService();

    static std::unique_ptr<RemoteAgentService> CreateWithDefaults(TopicManager& topic_manager,
                                                                  ndk::SpAIBinder link_to_death,
                                                                  IPAddressV4 listen_address);

    static std::unique_ptr<RemoteAgentService> Create(
            TopicManager& topic_manager, ndk::SpAIBinder link_to_death, IPAddressV4 listen_address,
            ConnectionManagerFactory connection_manager_factory,
            std::shared_ptr<::aidl::google::sdv::identity::IIdentityAgent> identity_agent,
            std::shared_ptr<::aidl::google::sdv::ca::ICertificateAuthority> certificate_authority,
            std::shared_ptr<
                    ::aidl::google::sdv::service_discovery::discovery::IServiceRegistrationAgent>
                    service_registration_agent,
            std::shared_ptr<
                    ::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent>
                    transport_support_agent,
            std::shared_ptr<
                    ::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent>
                    service_discovery_agent);

    bool Startup() override;
    void Shutdown() override;

    const std::string& GetLocalVmName() const override;

    ::sdv::SdvStatus RequestSubscribeToPublication(const std::string& vm_name,
                                                   const std::string& publication_name) override;
    ::sdv::SdvResult<std::vector<uint8_t>> FetchLastPublicationMessage(
            const std::string& vm_name, const std::string& publication_name) override;

    bool DumpStats(int fd) const override;

    // Public for testing.
    bool DumpStats(std::ostream& out_stream) const;

private:
    RemoteAgentService(
            TopicManager& topic_manager, IPAddressV4 listen_address,
            const ::aidl::google::sdv::identity::ServiceIdentity& service_identity,
            std::unique_ptr<AuthoritiesListener> authorities_listener,
            std::optional<ConnectionManager::SslConfig> ssl_config,
            ConnectionManagerFactory connection_manager_factory,
            std::shared_ptr<
                    ::aidl::google::sdv::service_discovery::discovery::IServiceRegistrationAgent>
                    service_registration_agent,
            std::shared_ptr<
                    ::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent>
                    transport_support_agent,
            std::shared_ptr<
                    ::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent>
                    service_discovery_agent);

    TopicManager& topic_manager_;
    const ::aidl::google::sdv::identity::ServiceIdentity identity_;
    std::unique_ptr<AuthoritiesListener> authorities_listener_;
    IPAddressV4 listen_address_;
    std::optional<ConnectionManager::SslConfig> ssl_config_;
    RemoteAgentHandshaker handshaker_;

    ConnectionManagerFactory connection_manager_factory_;
    std::shared_ptr<::aidl::google::sdv::service_discovery::discovery::IServiceRegistrationAgent>
            service_registration_agent_;
    std::shared_ptr<::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent>
            transport_support_agent_;
    std::shared_ptr<::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent>
            service_discovery_agent_;

    mutable std::shared_mutex lock_;
    std::unique_ptr<ConnectionManager> connection_manager_;
    RemoteAgentClientManager client_manager_;
    RemoteAgentServer server_;
};

}  // namespace android::sdv::dt::agent
