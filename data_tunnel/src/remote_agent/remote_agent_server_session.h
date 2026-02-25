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

#include <functional>

namespace android::sdv::dt::agent {

// Represents the server side of a single DT Agent peer-peer connection.
class RemoteAgentServerSession final {
public:
    // Type for callback fired when handshake succeeds
    using HandshakeCallback =
            std::function<void(RemoteAgentServerSession*, const std::string& vm_name)>;

    // Constructor
    //   connection: New inbound connection from client
    //   handshaker: Used to complete handshake in connection_thread_
    //   handshake_cb: Callback to be fired on successful handshake to report peer VM name
    //   topic_manager: Used to retrieve local topic data to send to peer
    RemoteAgentServerSession(IConnectionHolder connection, const RemoteAgentHandshaker& handshaker,
                             HandshakeCallback handshake_cb, TopicManager& topic_manager);

    RemoteAgentServerSession(const RemoteAgentServerSession&) = delete;
    RemoteAgentServerSession(RemoteAgentServerSession&&) = delete;

    // Destructor can block if executed while !IsTerminated().
    ~RemoteAgentServerSession();

    // Request termination of the session.
    void RequestTerminate();

    // Session is completely terminated and destruction should not block.
    bool IsTerminated() { return !connection_thread_active_; }

    void DumpStats(std::ostream& out_stream) const;

private:
    bool HandleLastMessageRequest(const proto::Payload_LastMessageRequest& request,
                                  RawBuffer& send_buffer);

    // Executed by connection_thread_; all reads from connection_ happen here.
    void ConnectionThread(const RemoteAgentHandshaker&, HandshakeCallback);

    IConnectionHolder connection_;
    TopicManager& topic_manager_;
    std::string peer_vm_name_;
    std::atomic<uint32_t> parcel_id_counter_;

    // Must be initialized before connection_thread_ to avoid race.
    std::atomic<bool> connection_thread_active_ = true;

    // Dedicated thread since IConnection ReceiveData / SendData are blocking.
    std::jthread connection_thread_;
};

}  // namespace android::sdv::dt::agent
