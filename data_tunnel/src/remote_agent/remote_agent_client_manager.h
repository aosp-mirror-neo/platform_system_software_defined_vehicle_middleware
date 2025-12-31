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
#include "remote_agent/remote_agent_client.h"

#include <aidl/google/sdv/identity/ServiceIdentity.h>
#include <aidl/google/sdv/service_discovery/discovery/IServiceDiscoveryAgent.h>
#include <aidl/google/sdv/service_discovery/discovery/IServiceUnitChangeListener.h>
#include <aidl/google/sdv/service_discovery/discovery/ITransportSupportAgent.h>

#include <ostream>
#include <unordered_map>

namespace android::sdv::dt::agent {

class RemoteAgentClientManager final {
public:
    RemoteAgentClientManager(const ::aidl::google::sdv::identity::ServiceIdentity& self_identity,
                             const RemoteAgentHandshaker&, TopicManager&);

    ~RemoteAgentClientManager();

    bool Startup(ConnectionManager& connection_manager,
                 std::shared_ptr<
                         ::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent>
                         service_discovery_agent,
                 std::shared_ptr<
                         ::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent>
                         transport_support_agent);
    void Shutdown();

    RemoteAgentClient::LastMessageResultFuture FetchLastMessage(const std::string& vm_name,
                                                                const std::string& topic_name);
    ::sdv::SdvStatus RequestSubscribe(const std::string& vm_name, const std::string& topic_name);

    // Inspectors for testing.
    std::unordered_set<std::string> CurrentClientNames();
    size_t TerminatingClientCount();

    bool DumpStats(std::ostream& out_stream) const;

private:
    class Listener;

    // Returns nullopt if socket address can't be retrieved or parsed.
    std::optional<SocketAddressV4> GetPeerDtAgentSocketAddress(
            const std::unique_lock<std::mutex>& guard,
            const ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition&);

    bool MaybeTerminateClientForPeer(const std::unique_lock<std::mutex>& guard,
                                     const std::string& peer_name);
    void RequestTerminateAllClients(const std::unique_lock<std::mutex>& guard);
    void ClearTerminatedClients(const std::unique_lock<std::mutex>& guard);

    // IServiceUnitChangeListener methods; we don't formally implement that interface but need
    // to consume events from an implementation of it.
    ::ndk::ScopedAStatus onServiceUnitRegistered(
            const ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition&);
    ::ndk::ScopedAStatus onServiceUnitUnregistered(
            const ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition&);
    ::ndk::ScopedAStatus onServiceUnitChanged(
            const ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition&);

    const ::aidl::google::sdv::identity::ServiceIdentity self_identity_;
    const RemoteAgentHandshaker& handshaker_;
    TopicManager& topic_manager_;

    // Listener binder object used to forward SD events to RemoteAgentClientManager
    std::shared_ptr<::aidl::google::sdv::service_discovery::discovery::IServiceUnitChangeListener>
            listener_;

    ConnectionManager* connection_manager_ = nullptr;

    // Binder services the client manager depends on; shared_ptr management dictated by Binder.
    std::shared_ptr<::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent>
            service_discovery_agent_;
    std::shared_ptr<::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent>
            transport_support_agent_;

    mutable std::mutex lock_;
    std::unordered_map<std::string, std::unique_ptr<RemoteAgentClient>> clients_;
    std::vector<std::unique_ptr<RemoteAgentClient>> terminating_clients_;

    // Pending subscription requests
    //
    // A client on our VM can learn of a remote topic and ask to subscribe to it before we get
    // the registration event for the DT agent on the publishing VM.
    //
    // In that case the current expected behavior is for DT agent to queue the request and send it
    // once the peer VM's DT agent has been discovered and connected to.
    std::unordered_map<std::string, std::unordered_set<std::string>> pending_subscription_requests_;
};

}  // namespace android::sdv::dt::agent
