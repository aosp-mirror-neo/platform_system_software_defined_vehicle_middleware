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

#include "remote_agent/remote_agent_common.h"
#include "remote_agent/remote_agent_handshaker.h"
#include "topic.h"
#include "topic_manager.h"

#include <aidl/google/sdv/identity/ServiceIdentity.h>

#include <future>
#include <unordered_map>
#include <unordered_set>

#include <sdv/status.hpp>

namespace android::sdv::dt::agent {

// Represents the client side of a single DT Agent peer-peer connection.
class RemoteAgentClient final : public ConnectionManager::EventHandler {
public:
    RemoteAgentClient(const ::aidl::google::sdv::identity::ServiceIdentity& peer_identity,
                      const SocketAddressV4& peer_address, ConnectionManager& connection_manager,
                      const RemoteAgentHandshaker& handshaker, TopicManager& topic_manager);
    ~RemoteAgentClient();

    RemoteAgentClient(const RemoteAgentClient&) = delete;
    RemoteAgentClient(RemoteAgentClient&&) = delete;

    // Initiate termination process for client.
    void RequestTerminate();

    // True when the client is fully terminated and can be deleted without blocking.
    bool IsTerminated();

    using LastMessageResult = ::sdv::SdvResult<std::vector<uint8_t>>;
    using LastMessageResultFuture = std::shared_future<LastMessageResult>;
    LastMessageResultFuture FetchLastMessage(const std::string& topic_name);

    ::sdv::SdvStatus RequestSubscribe(const std::string& topic_name);

    // Public for testing.
    struct Subscription {
        Topic& topic;
        bool is_active = false;  // Peer is sending topic data.
    };

    // Exists to support testing.
    std::unordered_map<int32_t, Subscription> Subscriptions() { return subscriptions_; }

    void DumpStats(std::ostream& out_stream) const;

private:
    ::sdv::SdvStatus SendSubscribeRequest(const std::string& topic_name, int32_t publication_id);

    // Moves the connection from CONNECT_WAIT to ACTIVE, executing any pending operations that were
    // waiting for this transition.
    void ActivateConnection();

    void HandleLastMessageResult(const proto::Payload_LastMessageResult& last_message_result);
    void HandleSubscribeResult(const proto::Payload_SubscribeResult& subscribe_result);
    void HandleTopicRawData(const proto::Payload_TopicRawData& topic_raw_data);
    void HandleTopicDestroyed(const proto::Payload_TopicDestroyed& topic_destroyed);
    void HandleConnectionMessages();

    // Handles transition out of ACTIVE, cleaning up data that is tied to a particular connection
    // and updating client_state_ if needed.
    void DeactivateConnection();

    void OnConnectionManagerEvent(::sdv::SdvResult<IConnectionHolder>);

    void ConnectionThread(ConnectionManager&, const RemoteAgentHandshaker&);

    const std::string peer_vm_name_;
    const ::aidl::google::sdv::identity::ServiceIdentity peer_identity_;
    const SocketAddressV4 peer_address_;
    TopicManager& topic_manager_;

    // This lock is used to prevent concurrent read/modify/write operations on state for imported
    // topics and pending last message requests, and also to ensure we don't drop any pending
    // subscriptions when transitioning into ACTIVE, and to safely access connection_ outside of the
    // connection thread.
    mutable std::mutex lock_;

    // Pending last message requests.  Guarded by lock_.
    struct LastMessageRequest {
        LastMessageRequest() { shared_future_ = promise_.get_future(); }
        std::promise<LastMessageResult> promise_;
        LastMessageResultFuture shared_future_;
    };
    std::unordered_map<std::string, LastMessageRequest> last_message_requests_;

    // List of remote topics imported from this peer.  Key is internal pub_id.  Guarded by lock_.
    std::unordered_map<int32_t, Subscription> subscriptions_;

    // State of the Client.
    enum class ClientState {
        // Waiting to establish connection or complete handshake.
        // LastMessage requests will fail; Subscribe requests will be pending.
        CONNECT_WAIT,

        // Connection is fully active and functional.
        ACTIVE,

        // Client is terminating; any new last message or subscribe requests will fail.
        TERMINATING,

        // Client has terminated; deletion will not block.
        TERMINATED,
    };
    std::atomic<ClientState> client_state_ = ClientState::CONNECT_WAIT;

    // Used to pass new connection to ConnectionThread; can't just assign
    // connection_ because it may be occupied in a receive wait.
    std::condition_variable pending_connection_ready_;
    std::optional<::sdv::SdvResult<IConnectionHolder>> pending_connection_result_;

    // Outbound connection to "Server" DT Agent.
    // Only modified by connection_thread; can only be used by code outside of connection_thread_
    // when connection_state_ is ACTIVE and lock_ is held.
    IConnectionHolder connection_;

    // Thread executing ConnectionThread
    std::jthread connection_thread_;

    std::atomic<uint32_t> parcel_id_counter_;
};

}  // namespace android::sdv::dt::agent
