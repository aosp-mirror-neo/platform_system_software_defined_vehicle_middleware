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

#include "remote_agent/remote_agent_server_session.h"

#include "remote_agent/remote_agent_topic_exporter.h"

#include <android-base/scopeguard.h>
#include <log/log.h>

#include <algorithm>
#include <unordered_map>
#include <unordered_set>

namespace android::sdv::dt::agent {

// Used for VM name before we get the actual name from handshake.
static const char* kUnknownVmName = "<UNKNOWN>";

using proto::Parcel;
using proto::Payload;
using proto::Payload_LastMessageRequest;
using proto::Payload_LastMessageResult;
using proto::Payload_SubscribeRequest;

RemoteAgentServerSession::RemoteAgentServerSession(IConnectionHolder connection,
                                                   const RemoteAgentHandshaker& handshaker,
                                                   HandshakeCallback handshake_cb,
                                                   TopicManager& topic_manager) :
      connection_(std::move(connection)),
      topic_manager_(topic_manager),
      peer_vm_name_(kUnknownVmName) {
    ALOGD("RemoteAgentServerSession: Starting for connection %s", connection_->ToString().c_str());
    connection_thread_ =
            std::jthread([this, &handshaker, handshake_cb = std::move(handshake_cb)]() {
                ConnectionThread(handshaker, std::move(handshake_cb));
            });
}

RemoteAgentServerSession::~RemoteAgentServerSession() {
    RequestTerminate();
    // Implicit wait for connection thread join.
}

void RemoteAgentServerSession::RequestTerminate() {
    connection_->RequestDisconnect();
}

void RemoteAgentServerSession::DumpStats(std::ostream& out_stream) const {
    out_stream << std::format("\t{},{},{},{}\n", peer_vm_name_, connection_->ToString(),
                              connection_->BytesSent(), connection_->BytesReceived());
}

bool RemoteAgentServerSession::HandleLastMessageRequest(const Payload_LastMessageRequest& request,
                                                        RawBuffer& send_buffer) {
    auto& topic_name = request.topic_name;
    ALOGD("RemoteAgentServerSession(%s): Last message request for %s", peer_vm_name_.c_str(),
          topic_name.c_str());

    Payload_LastMessageResult response;
    response.topic_name = topic_name;
    response.result = Payload_LastMessageResult::Result_Failure;

    // Try to fetch the last message and populate the payload
    auto topic = topic_manager_.FindTopic(topic_name, /* local_only= */ true);
    if (!topic) {
        ALOGW("RemoteAgentServerSession(%s): Unable to find topic %s to get last message.",
              peer_vm_name_.c_str(), topic_name.c_str());
    } else {
        auto message = topic->GetLastMessage();
        if (!message) {
            ALOGW("RemoteAgentServerSession(%s): Unable to get last message from %s.",
                  peer_vm_name_.c_str(), topic_name.c_str());
        } else {
            response.result = Payload_LastMessageResult::Result_Ok;
            response.buffer = RawBuffer(*message);
        }
    }

    // Make sure send buffer is large enough for the parcel.
    auto parcel_size = Parcel::ParcelSize(response);
    if (parcel_size > send_buffer.Size()) {
        send_buffer.Resize(parcel_size, /*copy_data=*/false);
    }

    if (!SendPayload(*connection_, response, parcel_id_counter_++, send_buffer)) {
        ALOGE("RemoteAgentServerSession(%s): Unable to send last message result for %s",
              peer_vm_name_.c_str(), topic_name.c_str());
        return false;
    }
    return true;
}

void RemoteAgentServerSession::ConnectionThread(
        const RemoteAgentHandshaker& handshaker,
        RemoteAgentServerSession::HandshakeCallback handshake_cb) {
    ALOGD("RemoteAgentServerSession(%s)::ConnectionThread started", peer_vm_name_.c_str());

    android::base::ScopeGuard exit_guard([this]() {
        // Do potentially-blocking Disconnect from ConnectionThread so that destruction
        // doesn't have to wait on it and so that we don't depend on garbage collection to
        // shut down server connection and allow ConnectionManager destruction to complete.
        connection_->Disconnect();

        // Ensure marked inactive on exit.
        connection_thread_active_ = false;
    });

    RawBuffer send_buffer(kDefaultBufferSize);
    // ReceivePayload() may later increase the buffer size if needed
    RawBuffer receive_buffer(kDefaultBufferSize);
    BufferWriter receive_writer(receive_buffer);

    auto opt_vm_name = handshaker.PerformHandshake(*connection_, std::nullopt);
    if (!opt_vm_name) {
        return;
    }
    handshake_cb(this, *opt_vm_name);
    peer_vm_name_ = *opt_vm_name;

    // Construction of exporter delayed until we have VM name.
    RemoteAgentTopicExporter exporter(peer_vm_name_, topic_manager_, *connection_,
                                      parcel_id_counter_);

    bool should_continue = true;
    while (should_continue) {
        auto [parse_result, payload] = ReceivePayload(*connection_, nullptr, receive_writer);

        if (parse_result != Parcel::ParseResult::Success) {
            ALOGE("RemoteAgentServerSession(%s): ReceivePayload failed", peer_vm_name_.c_str());
            break;
        }

        if (parse_result == Parcel::ParseResult::UnknownPayload) {
            ALOGW("RemoteAgentServerSession(%s): Received unknown payload. Ignoring.",
                  peer_vm_name_.c_str());
            continue;
        }

        if (auto lmr_request = payload->As<Payload_LastMessageRequest>()) {
            should_continue = HandleLastMessageRequest(*lmr_request, send_buffer);
        } else if (auto sub_request = payload->As<Payload_SubscribeRequest>()) {
            should_continue = exporter.HandleSubscribeRequest(*sub_request, send_buffer);
        } else {
            ALOGE("RemoteAgentServerSession(%s): Received unsupported payload type: %d",
                  peer_vm_name_.c_str(), payload->type);
        }
    }
}
}  // namespace android::sdv::dt::agent
