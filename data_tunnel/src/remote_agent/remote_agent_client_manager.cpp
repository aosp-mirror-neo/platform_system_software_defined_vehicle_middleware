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

#include "remote_agent/remote_agent_client_manager.h"

#include <aidl/google/sdv/service_discovery/discovery/BnServiceUnitChangeListener.h>
#include <aidl/google/sdv/service_discovery/discovery/IServiceUnitChangeListener.h>
#include <android-base/logging.h>

#include <future>

using ::aidl::google::sdv::identity::ServiceIdentity;
using ::aidl::google::sdv::service_discovery::common::metadata::TransportMetadata;
using ::aidl::google::sdv::service_discovery::discovery::BnServiceUnitChangeListener;
using ::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent;
using ::aidl::google::sdv::service_discovery::discovery::IServiceUnitChangeListener;
using ::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent;
using ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition;

namespace android::sdv::dt::agent {

// Unit change listener binder object class suitable for registering with ServiceDiscovery;
// redirects events to the owning RemoteAgentClientManager.
//
// It exists to get unit change events to RemoteAgentClientManager without requiring that class to
// be a shared_ptr-managed binder object.
class RemoteAgentClientManager::Listener : public BnServiceUnitChangeListener {
public:
    Listener(RemoteAgentClientManager& client_mgr) : client_mgr_(client_mgr) {}

    ::ndk::ScopedAStatus onServiceUnitRegistered(const ServiceUnitDefinition& unit_def) override {
        return client_mgr_.onServiceUnitRegistered(unit_def);
    }

    ::ndk::ScopedAStatus onServiceUnitUnregistered(const ServiceUnitDefinition& unit_def) override {
        return client_mgr_.onServiceUnitUnregistered(unit_def);
    }

    ::ndk::ScopedAStatus onServiceUnitChanged(const ServiceUnitDefinition& unit_def) override {
        return client_mgr_.onServiceUnitChanged(unit_def);
    }

private:
    RemoteAgentClientManager& client_mgr_;
};

RemoteAgentClientManager::RemoteAgentClientManager(const ServiceIdentity& self_identity,
                                                   const RemoteAgentHandshaker& handshaker,
                                                   TopicManager& topic_manager) :
      self_identity_(self_identity),
      handshaker_(handshaker),
      topic_manager_(topic_manager),
      listener_(ndk::SharedRefBase::make<Listener>(*this)) {
    CHECK(listener_);
}

RemoteAgentClientManager::~RemoteAgentClientManager() {
    Shutdown();
}

bool RemoteAgentClientManager::Startup(
        ConnectionManager& connection_manager,
        std::shared_ptr< ::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent>
                service_discovery_agent,
        std::shared_ptr< ::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent>
                transport_support_agent) {
    std::unique_lock guard(lock_);
    CHECK(!connection_manager_);

    guard.unlock();

    auto call_status =
            service_discovery_agent->registerServiceUnitChangeListenerByType(kDtAgentUnitType,
                                                                             listener_);
    if (!call_status.isOk()) {
        ALOGE("IServiceDiscoveryAgent::registerServiceUnitChangeListenerByName() failed! %s",
              call_status.getDescription().c_str());
        return false;
    }

    guard.lock();

    // Hold on to agent binder interfaces and connection manager we'll need later.
    service_discovery_agent_ = std::move(service_discovery_agent);
    transport_support_agent_ = std::move(transport_support_agent);
    connection_manager_ = &connection_manager;

    // Finally garbage-collect any clients that finished terminating.
    ClearTerminatedClients(guard);

    return true;
}

void RemoteAgentClientManager::Shutdown() {
    std::unique_lock guard(lock_);
    // Ok to call Shutdown twice.
    if (!connection_manager_) {
        return;
    }
    guard.unlock();

    auto call_status =
            service_discovery_agent_->unregisterServiceUnitChangeListenerByType(kDtAgentUnitType,
                                                                                listener_);
    if (!call_status.isOk()) {
        ALOGE("IServiceDiscoveryAgent::unregisterServiceUnitChangeListenerByName() failed! %s",
              call_status.getDescription().c_str());
    }

    guard.lock();
    service_discovery_agent_.reset();
    transport_support_agent_.reset();
    connection_manager_ = nullptr;
    RequestTerminateAllClients(guard);
    auto pending_subscriptions = std::exchange(pending_subscription_requests_, {});
    guard.unlock();

    // Unregister topics for any pending subs that were never handed off to a
    // RemoteAgentClient
    for (const auto& [vm_name, pending_topics] : pending_subscriptions) {
        for (const auto& pending_topic : pending_topics) {
            topic_manager_.UnregisterRemoteTopic(pending_topic);
        }
    }
}

RemoteAgentClient::LastMessageResultFuture RemoteAgentClientManager::FetchLastMessage(
        const std::string& vm_name, const std::string& topic_name) {
    std::unique_lock guard(lock_);

    if (!connection_manager_) {
        // If connection_manager_ isn't populated then we haven't started up.
        std::promise<RemoteAgentClient::LastMessageResult> result_promise;
        result_promise.set_value(
                base::unexpected(::sdv::SdvStatus(::sdv::SdvStatusCode::kFailedPrecondition)));
        return result_promise.get_future();
    }

    auto client_it = clients_.find(vm_name);
    if (client_it == clients_.end()) {
        std::promise<RemoteAgentClient::LastMessageResult> result_promise;
        result_promise.set_value(base::unexpected(
                ::sdv::SdvStatus(::sdv::SdvStatusCode::kNotFound, "Unknown peer VM name")));
        return result_promise.get_future();
    }

    return client_it->second->FetchLastMessage(topic_name);
}

::sdv::SdvStatus RemoteAgentClientManager::RequestSubscribe(const std::string& vm_name,
                                                            const std::string& topic_name) {
    std::unique_lock guard(lock_);

    if (!connection_manager_) {
        // If connection_manager_ isn't populated then we haven't started up.
        return ::sdv::SdvStatus(::sdv::SdvStatusCode::kFailedPrecondition);
    }

    auto client_it = clients_.find(vm_name);
    if (client_it == clients_.end()) {
        // Unknown peer, put this in the pending set.
        pending_subscription_requests_[vm_name].insert(topic_name);
        return ::sdv::SdvStatus(::sdv::SdvStatusCode::kOk);
    }

    return client_it->second->RequestSubscribe(topic_name);
}

std::unordered_set<std::string> RemoteAgentClientManager::CurrentClientNames() {
    std::unordered_set<std::string> result;
    std::unique_lock guard(lock_);
    for (auto& entry : clients_) {
        result.insert(entry.first);
    }
    return result;
}

size_t RemoteAgentClientManager::TerminatingClientCount() {
    return terminating_clients_.size();
}

bool RemoteAgentClientManager::DumpStats(std::ostream& out_stream) const {
    std::lock_guard guard(lock_);
    if (clients_.empty()) {
        out_stream << "\nOutbound Remote Agent Connections: NONE\n";
        return true;
    }

    out_stream << "\nOutbound Remote Agent Connections:\n";
    out_stream << "\tvm_name,connection,bytes_sent,bytes_received\n";
    for (const auto& client_entry : clients_) {
        client_entry.second->DumpStats(out_stream);
    }

    return true;
}

std::optional<SocketAddressV4> RemoteAgentClientManager::GetPeerDtAgentSocketAddress(
        const std::unique_lock<std::mutex>& guard, const ServiceUnitDefinition& unit_def) {
    CHECK(guard.owns_lock() && guard.mutex() == &lock_);

    if (!transport_support_agent_) {
        ALOGE("RemoteAgentClientManager::GetPeerDtAgentSocketAddress called while shut down.");
        return std::nullopt;
    }

    TransportMetadata transport_metadata;
    ServiceUnitDefinition service_unit_definition;
    auto call_status = transport_support_agent_->getAgentMetadata(unit_def.identity.sid,
                                                                  unit_def.serviceUnitName,
                                                                  &service_unit_definition,
                                                                  &transport_metadata);
    if (!call_status.isOk()) {
        ALOGE("Unable to retrieve metadata for %s! "
              "ITransportSupportAgent::getAgentMetadata() "
              "failed! %s",
              call_status.getDescription().c_str(), unit_def.identity.name.sdvVmName.c_str());
        return std::nullopt;
    }

    SocketAddressV4 socket_address;
    if (socket_address.Deserialize(transport_metadata.value_holder.data(),
                                   transport_metadata.value_holder.size()) == 0) {
        return std::nullopt;
    }
    return socket_address;
}

bool RemoteAgentClientManager::MaybeTerminateClientForPeer(
        const std::unique_lock<std::mutex>& guard, const std::string& peer_name) {
    CHECK(guard.owns_lock() && guard.mutex() == &lock_);
    auto extracted = clients_.extract(peer_name);
    if (!extracted.empty()) {
        extracted.mapped()->RequestTerminate();
        terminating_clients_.push_back(std::move(extracted.mapped()));
    }
    // Finally  garbage-collect any terminated clients.
    ClearTerminatedClients(guard);
    return !extracted.empty();
}

void RemoteAgentClientManager::RequestTerminateAllClients(
        const std::unique_lock<std::mutex>& guard) {
    CHECK(guard.owns_lock() && guard.mutex() == &lock_);
    for (auto& entry : clients_) {
        entry.second->RequestTerminate();
        terminating_clients_.push_back(std::move(entry.second));
    }
    clients_.clear();
    ClearTerminatedClients(guard);
}

void RemoteAgentClientManager::ClearTerminatedClients(const std::unique_lock<std::mutex>& guard) {
    CHECK(guard.owns_lock() && guard.mutex() == &lock_);
    std::erase_if(terminating_clients_, [](auto& client) { return client->IsTerminated(); });
}

::ndk::ScopedAStatus RemoteAgentClientManager::onServiceUnitRegistered(
        const ServiceUnitDefinition& unit_def) {
    const auto& identity = unit_def.identity;
    ALOGD("RemoteAgentClientManager: onServiceUnitRegistered(%s)",
          unit_def.identity.toString().c_str());

    if (identity == self_identity_) {
        return ::ndk::ScopedAStatus::ok();
    }

    const auto& vm_name = identity.name.sdvVmName;

    std::unique_lock guard(lock_);
    if (!connection_manager_) {
        ALOGW("RemoteAgentClientManager got onServiceUnitRegistered while shut down.  Ignoring.");
        return ::ndk::ScopedAStatus::ok();
    }

    auto socket_address = GetPeerDtAgentSocketAddress(guard, unit_def);
    if (!socket_address) {
        ALOGE("RemoteAgentClientManager: Unable to parse Agent metadata for VM %s",
              vm_name.c_str());
        return ::ndk::ScopedAStatus::ok();
    }

    MaybeTerminateClientForPeer(guard, vm_name);
    auto [new_entry_it, inserted] =
            clients_.emplace(vm_name,
                             std::make_unique<RemoteAgentClient>(identity, *socket_address,
                                                                 *connection_manager_, handshaker_,
                                                                 topic_manager_));
    CHECK(inserted);

    auto pending_subs = pending_subscription_requests_.extract(vm_name);
    if (pending_subs.empty()) {
        return ::ndk::ScopedAStatus::ok();
    }

    auto& client = *new_entry_it->second;
    for (const auto& topic_name : pending_subs.mapped()) {
        if (!client.RequestSubscribe(topic_name).isOk()) {
            ALOGE("RemoteAgentClientManager: Pending subscription request to %s for %s failed!",
                  vm_name.c_str(), topic_name.c_str());
        }
    }

    return ::ndk::ScopedAStatus::ok();
}

::ndk::ScopedAStatus RemoteAgentClientManager::onServiceUnitUnregistered(
        const ServiceUnitDefinition& unit_def) {
    const auto& identity = unit_def.identity;
    ALOGD("RemoteAgentClientManager: onServiceUnitUnregistered(%s)",
          unit_def.identity.toString().c_str());

    if (identity == self_identity_) {
        return ::ndk::ScopedAStatus::ok();
    }
    const auto& vm_name = identity.name.sdvVmName;
    std::unique_lock guard(lock_);
    if (!MaybeTerminateClientForPeer(guard, vm_name)) {
        ALOGW("RemoteAgentClientManager: onServiceUnitUnregistered for unknown VM %s.",
              vm_name.c_str());
    }
    return ::ndk::ScopedAStatus::ok();
}

::ndk::ScopedAStatus RemoteAgentClientManager::onServiceUnitChanged(
        const ServiceUnitDefinition& unit_def) {
    const auto& identity = unit_def.identity.name.sdvVmName;
    ALOGW("RemoteAgentClientManager: onServiceUnitChanged VM %s is unexpected",
          unit_def.identity.name.sdvVmName.c_str());
    return ::ndk::ScopedAStatus::ok();
}

}  // namespace android::sdv::dt::agent
