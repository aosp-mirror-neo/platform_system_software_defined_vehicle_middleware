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

#define LOG_TAG "sdv_dt_agent:remote_agent"

#include "remote_agent/remote_agent_server.h"

#include "config.h"
#include "remote_agent_server_session.h"

#include <aidl/google/sdv/service_discovery/common/RegistrationToken.h>
#include <android-base/logging.h>

using ::aidl::google::sdv::service_discovery::common::AccessControl;
using ::aidl::google::sdv::service_discovery::common::RegistrationToken;
using ::aidl::google::sdv::service_discovery::common::metadata::ApplicationMetadata;
using ::aidl::google::sdv::service_discovery::common::metadata::PublisherUnitMetadata;
using ::aidl::google::sdv::service_discovery::common::metadata::TransportMetadata;
using ::aidl::google::sdv::service_discovery::discovery::IServiceRegistrationAgent;
using ::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent;

namespace android::sdv::dt::agent {

RemoteAgentServer::RemoteAgentServer(const std::string& unit_name,
                                     const RemoteAgentHandshaker& handshaker,
                                     TopicManager& topic_manager) :
      unit_name_(unit_name), handshaker_(handshaker), topic_manager_(topic_manager) {}

RemoteAgentServer::~RemoteAgentServer() {
    Shutdown();
}

bool RemoteAgentServer::Startup(
        ConnectionManager& connection_manager,
        std::shared_ptr<IServiceRegistrationAgent> service_registration_agent,
        std::shared_ptr<ITransportSupportAgent> transport_support_agent) {
    {
        std::unique_lock guard(lock_);
        CHECK(!service_registration_agent_);
    }

    RegistrationToken registration_token;
    auto call_status =
            service_registration_agent->registerServiceUnit(unit_name_, kDtAgentUnitType,
                                                            AccessControl{}, ApplicationMetadata{},
                                                            &registration_token);
    if (!call_status.isOk()) {
        ALOGE("IServiceRegistrationAgent::registerServiceUnit() failed! %s",
              call_status.getDescription().c_str());
        return false;
    }

    // Hold on to service_registration_agent_ for shutdown.
    {
        std::unique_lock guard(lock_);
        service_registration_agent_ = std::move(service_registration_agent);
    }

    // Start listening so that we have the port needed to register transport metadata.
    if (!connection_manager.Listen(*this)) {
        ALOGE("Listen failed!  Aborting RemoteAgentServer startup.");
        Shutdown();  // Call shutdown to unregister service unit.
        return false;
    }

    auto listen_socket_address =
            SocketAddressV4(connection_manager.GetLocalIp(), connection_manager.GetListenPort());
    TransportMetadata agent_metadata{};
    listen_socket_address.Serialize(agent_metadata.value_holder);
    call_status = transport_support_agent->setAgentMetadata(registration_token, agent_metadata);
    if (!call_status.isOk()) {
        ALOGE("ITransportSupportAgent::setAgentMetadata() failed! %s",
              call_status.getDescription().c_str());
        Shutdown();  // Call shutdown to unregister service unit.
        return false;
    }

    // Finally garbage-collect any sessions that finished terminating.
    ClearTerminatedSessions(std::unique_lock(lock_));

    return true;
}

void RemoteAgentServer::Shutdown() {
    {
        std::unique_lock guard(lock_);
        if (!service_registration_agent_) {
            return;
        }
    }

    auto call_status = service_registration_agent_->unregisterServiceUnit(unit_name_);
    if (!call_status.isOk()) {
        ALOGE("IServiceRegistrationAgent::unregisterServiceUnit() failed: %s ",
              call_status.getDescription().c_str());
    }

    std::unique_lock guard(lock_);
    service_registration_agent_.reset();
    for (auto& [session, unused] : sessions_) {
        session->RequestTerminate();
    }
    ClearTerminatedSessions(guard);
}

std::unordered_set<std::string> RemoteAgentServer::GetPairedClientVmNames() const {
    std::lock_guard guard(lock_);
    std::unordered_set<std::string> result;
    for (auto& entry : vm_name_to_session_) {
        result.insert(entry.first);
    }
    return result;
}

uint32_t RemoteAgentServer::GetTotalSessionCount() const {
    std::lock_guard guard(lock_);
    return sessions_.size();
}

bool RemoteAgentServer::DumpStats(std::ostream& out_stream) const {
    std::lock_guard guard(lock_);
    if (sessions_.empty()) {
        out_stream << "\nInbound Remote Agent Connections: NONE\n";
        return true;
    }
    out_stream << "\nInbound Remote Agent Connections:\n";
    out_stream << "\tvm_name,connection,bytes_sent,bytes_received\n";
    for (const auto& session_entry : sessions_) {
        session_entry.first->DumpStats(out_stream);
    }

    return true;
}

void RemoteAgentServer::OnConnectionManagerEvent(
        ::sdv::SdvResult<std::unique_ptr<IConnection>> connection) {
    if (!connection.has_value()) {
        ALOGW("RemoteAgentServer: Got inbound connection error: %s",
              connection.error().toString().c_str());
        return;
    }
    std::unique_lock guard(lock_);
    if (!service_registration_agent_) {
        // If service_registration_agent_ is empty then we're currently shut down.
        // connection's destructor will kill connection, but release the lock first.
        guard.unlock();
        return;
    }
    auto session = std::make_unique<RemoteAgentServerSession>(
            std::move(connection.value()), handshaker_,
            [this](RemoteAgentServerSession* session, const std::string& vm_name) {
                OnHandshakeSuccess(session, vm_name);
            },
            topic_manager_);
    sessions_.emplace(session.get(), std::move(session));
    ClearTerminatedSessions(guard);
}

void RemoteAgentServer::OnHandshakeSuccess(RemoteAgentServerSession* session_ptr,
                                           const std::string& peer_vm_name) {
    std::unique_lock guard(lock_);
    if (!session_ptr) {
        ALOGE("RemoteAgentServer::OnHandshakeSuccess with null session_ptr.");
        return;
    }

    if (!sessions_.contains(session_ptr)) {
        ALOGE("RemoteAgentServer::OnHandshakeSuccess for unknown session (Peer VM: %s).  "
              "Terminating session.",
              peer_vm_name.c_str());
        session_ptr->RequestTerminate();
        return;
    }

    if (vm_name_to_session_.contains(peer_vm_name)) {
        // Start termination process for previous session.
        vm_name_to_session_[peer_vm_name]->RequestTerminate();
    }
    vm_name_to_session_[peer_vm_name] = session_ptr;
    ClearTerminatedSessions(guard);
}

// Garbage-collection function.  It would be nice if we could have RemoteAgentServerSession
// fire a callback to tell us when it's terminated, but that callback would run in a thread
// owned by the session object, so deleting the session at that point would lead to a self-join.
//
// Instead we just clean up the session map whenever we get a chance to by calling this method.
void RemoteAgentServer::ClearTerminatedSessions(const std::unique_lock<std::mutex>& guard) {
    CHECK(guard.owns_lock() && guard.mutex() == &lock_);
    std::erase_if(sessions_, [](auto& entry) { return entry.first->IsTerminated(); });
    std::erase_if(vm_name_to_session_,
                  [this](auto& entry) { return !sessions_.contains(entry.second); });
}

}  // namespace android::sdv::dt::agent
