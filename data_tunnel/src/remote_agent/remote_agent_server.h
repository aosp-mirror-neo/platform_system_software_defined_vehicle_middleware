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

#include "connection.h"
#include "remote_agent/remote_agent_server_session.h"

#include <aidl/google/sdv/service_discovery/discovery/IServiceRegistrationAgent.h>
#include <aidl/google/sdv/service_discovery/discovery/ITransportSupportAgent.h>

#include <unordered_map>
#include <unordered_set>

namespace android::sdv::dt::agent {

class RemoteAgentServer final : public ConnectionManager::EventHandler {
public:
    RemoteAgentServer(const std::string& unit_name, const RemoteAgentHandshaker& handshaker,
                      TopicManager& topic_manager);
    ~RemoteAgentServer();

    bool Startup(
            ConnectionManager&,
            std::shared_ptr<
                    ::aidl::google::sdv::service_discovery::discovery::IServiceRegistrationAgent>,
            std::shared_ptr<
                    ::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent>);

    void Shutdown();

    // Inspectors for testing
    std::unordered_set<std::string> GetPairedClientVmNames() const;
    uint32_t GetTotalSessionCount() const;

    bool DumpStats(std::ostream& out_stream) const;

    // Public only for test purposes.
    void OnHandshakeSuccess(RemoteAgentServerSession* session, const std::string& peer_vm_name);

private:
    // Called by connection_manager when we get a new inbound connection.
    // TODO(b/432436114): Update this to take just the connection & pass lambda to Connect()
    void OnConnectionManagerEvent(::sdv::SdvResult<IConnectionHolder> connection);

    // Requires lock_ to be held; callers must provide guard.
    void ClearTerminatedSessions(const std::unique_lock<std::mutex>& guard);

    const std::string unit_name_;
    const RemoteAgentHandshaker& handshaker_;
    TopicManager& topic_manager_;
    std::shared_ptr<::aidl::google::sdv::service_discovery::discovery::IServiceRegistrationAgent>
            service_registration_agent_;

    using SessionOwnershipMap = std::unordered_map<RemoteAgentServerSession*,
                                                   std::unique_ptr<RemoteAgentServerSession>>;

    mutable std::mutex lock_;
    SessionOwnershipMap sessions_;
    std::unordered_map<std::string, RemoteAgentServerSession*> vm_name_to_session_;
};

}  // namespace android::sdv::dt::agent
