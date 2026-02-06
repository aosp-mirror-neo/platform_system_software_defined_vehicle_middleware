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

#include "remote_agent/remote_agent_client.h"

#include <android-base/scopeguard.h>
#include <log/log.h>

using ::aidl::google::sdv::identity::ServiceIdentity;
namespace android::sdv::dt::agent {

using proto::Parcel;
using proto::Payload;
using proto::Payload_LastMessageRequest;
using proto::Payload_LastMessageResult;
using proto::Payload_SubscribeRequest;
using proto::Payload_SubscribeResult;
using proto::Payload_TopicDestroyed;
using proto::Payload_TopicRawData;

static constexpr auto kReconnectDelay = std::chrono::milliseconds(50);

RemoteAgentClient::RemoteAgentClient(const ServiceIdentity& peer_identity,
                                     const SocketAddressV4& peer_address,
                                     ConnectionManager& connection_manager,
                                     const RemoteAgentHandshaker& handshaker,
                                     TopicManager& topic_manager) :
      peer_vm_name_(peer_identity.name.sdvVmName),
      peer_identity_(peer_identity),
      peer_address_(peer_address),
      topic_manager_(topic_manager),
      connection_thread_(
              [this](auto connection_handler, auto handshaker) {
                  ConnectionThread(connection_handler, handshaker);
              },
              std::ref(connection_manager), std::ref(handshaker)) {
    ALOGD("RemoteAgentClient(%s): Created for peer address=%s", peer_vm_name_.c_str(),
          peer_address_.ToString().c_str());
}

RemoteAgentClient::~RemoteAgentClient() {
    RequestTerminate();
    connection_thread_.join();
}

void RemoteAgentClient::RequestTerminate() {
    std::unique_lock guard(lock_);
    if (client_state_ == ClientState::TERMINATING || client_state_ == ClientState::TERMINATED) {
        return;
    }

    // Immediately unregister any publications imported from this peer to prevent would-be
    // subscribers from connecting to them.
    for (const auto& subscription : subscriptions_) {
        topic_manager_.UnregisterRemoteTopic(subscription.first);
    }
    subscriptions_.clear();

    client_state_ = ClientState::TERMINATING;
    if (connection_) {
        // Make sure connection thread exits any receive wait.
        connection_->RequestDisconnect();
    }
}

bool RemoteAgentClient::IsTerminated() {
    return client_state_ == ClientState::TERMINATED;
}

RemoteAgentClient::LastMessageResultFuture RemoteAgentClient::FetchLastMessage(
        const std::string& topic_name) {
    std::unique_lock guard(lock_);
    if (client_state_ != ClientState::ACTIVE) {
        std::promise<LastMessageResult> result_promise;
        result_promise.set_value(base::unexpected(
                ::sdv::SdvStatus(::sdv::SdvStatusCode::kInternal, "Not connected to peer")));
        return result_promise.get_future();
    }

    auto [entry, inserted] = last_message_requests_.try_emplace(topic_name, LastMessageRequest());
    // Taking the reference here is important:  Another modification to last_message_requests_ after
    // we release the lock below can invalidate the iterator we got back from try_emplace, but won't
    // invalidate a pointer or reference to an entry.
    auto& pending_request = entry->second;
    auto result_future = pending_request.shared_future_;

    // Already had an entry, return its future.
    if (!inserted) {
        return result_future;
    }

    // Release lock before sending.
    guard.unlock();

    RawBuffer data_buffer(kDefaultBufferSize);
    if (!SendPayload(*connection_, Payload_LastMessageRequest(topic_name), parcel_id_counter_++,
                     data_buffer)) {
        ALOGE("RemoteAgentClient(%s): Error sending last message request for %s",
              peer_vm_name_.c_str(), topic_name.c_str());

        // Resolve promise before deleting the entry that contains it.
        pending_request.promise_.set_value(
                base::unexpected(::sdv::SdvStatus(::sdv::SdvStatusCode::kInternal,
                                                  "Error sending request to peer")));

        guard.lock();
        if (last_message_requests_.erase(topic_name) == 0) {
            ALOGE("RemoteAgentClient(%s): LastMessageRequest for %s unexpectedly missing!",
                  peer_vm_name_.c_str(), topic_name.c_str());
        }
    }

    return result_future;
}

void RemoteAgentClient::ActivateConnection() {
    ALOGD("RemoteAgentClient(%s): ActivateConnection running", peer_vm_name_.c_str());
    // Process all the pending subscription requests
    std::lock_guard guard(lock_);
    if (client_state_ != ClientState::CONNECT_WAIT) {
        return;
    }
    client_state_ = ClientState::ACTIVE;
    auto it = subscriptions_.begin();
    while (it != subscriptions_.end()) {
        if (!SendSubscribeRequest(it->first).isOk()) {
            topic_manager_.UnregisterRemoteTopic(it->first);
            it = subscriptions_.erase(it);
        } else {
            ++it;
        }
    }
}

::sdv::SdvStatus RemoteAgentClient::RequestSubscribe(const std::string& topic_name) {
    ALOGD("RemoteAgentClient(%s)::RequestSubscribe(%s)", peer_vm_name_.c_str(), topic_name.c_str());

    auto* topic = topic_manager_.FindTopic(topic_name, /* local_only= */ false);
    if (!topic) {
        return ::sdv::SdvStatus(::sdv::SdvStatusCode::kNotFound);
    }

    if (!topic->GetIsRemote()) {
        return ::sdv::SdvStatus(::sdv::SdvStatusCode::kFailedPrecondition,
                                "Cannot create remote subscription to local topic");
    }

    std::lock_guard guard(lock_);

    if (client_state_ == ClientState::TERMINATING || client_state_ == ClientState::TERMINATED) {
        return ::sdv::SdvStatus(::sdv::SdvStatusCode::kInternal,
                                "Peer connection is terminating or terminated");
    }

    auto [entry_it, inserted] = subscriptions_.try_emplace(topic_name, *topic);

    if (!inserted) {
        // It's unexpected that we'd already have an entry; DT agent should only call this
        // method when it first creates a `Topic` record for a given remote topic.
        if (&entry_it->second.topic == topic) {
            ALOGW("RemoteAgentClient(%s): Got duplicate RequestSubscribe for %s.  Ignoring.",
                  peer_vm_name_.c_str(), topic_name.c_str());
            return ::sdv::SdvStatus(::sdv::SdvStatusCode::kOk);
        } else {
            ALOGE("RemoteAgentClient(%s): Got RequestSubscribe for already-subscribed %s, with "
                  "different topic instance.",
                  peer_vm_name_.c_str(), topic_name.c_str());
            return ::sdv::SdvStatus(::sdv::SdvStatusCode::kAlreadyExists);
        }
    }

    // Normal success case -- no collision
    if (client_state_ == ClientState::CONNECT_WAIT) {
        ALOGD("RemoteAgentClient(%s): Postponing subscription to %s.", peer_vm_name_.c_str(),
              topic_name.c_str());
        return ::sdv::SdvStatus(::sdv::SdvStatusCode::kOk);
    }

    return SendSubscribeRequest(topic_name);
}

::sdv::SdvStatus RemoteAgentClient::SendSubscribeRequest(const std::string& topic_name) {
    RawBuffer data_buffer(kDefaultBufferSize);
    if (!SendPayload(*connection_, Payload_SubscribeRequest(topic_name), parcel_id_counter_++,
                     data_buffer)) {
        return ::sdv::SdvStatus(::sdv::SdvStatusCode::kInternal,
                                std::format("SendPayload(Payload_SubscribeRequest, {}) failed!",
                                            topic_name));
    }

    return ::sdv::SdvStatus(::sdv::SdvStatusCode::kOk);
}

void RemoteAgentClient::DumpStats(std::ostream& out_stream) const {
    std::lock_guard guard(lock_);
    if (!connection_) {
        out_stream << std::format("\t{},<NONE>,0,0\n", peer_vm_name_);
    } else {
        out_stream << std::format("\t{},{},{},{}\n", peer_vm_name_, connection_->ToString(),
                                  connection_->BytesSent(), connection_->BytesReceived());
    }
}

void RemoteAgentClient::HandleLastMessageResult(
        const Payload_LastMessageResult& last_message_result) {
    const auto& topic_name = last_message_result.topic_name_;
    ALOGD("RemoteAgentClient(%s): LastMessage result for %s", peer_vm_name_.c_str(),
          topic_name.c_str());
    std::unique_lock<std::mutex> guard(lock_);
    auto waiting_entry = last_message_requests_.extract(topic_name);
    guard.unlock();

    if (waiting_entry.empty()) {
        ALOGE("RemoteAgentClient(%s): Got last message result for %s with no pending request",
              peer_vm_name_.c_str(), topic_name.c_str());
        return;
    }

    if (last_message_result.result_ != Payload_LastMessageResult::Result_Ok) {
        waiting_entry.mapped().promise_.set_value(base::unexpected(
                ::sdv::SdvStatus(::sdv::SdvStatusCode::kInternal, "Peer returned failure")));
        return;
    }

    auto& buffer = last_message_result.buffer_;
    std::vector<uint8_t> message_data(buffer.Ptr(), buffer.Ptr() + buffer.Size());
    waiting_entry.mapped().promise_.set_value(LastMessageResult(std::move(message_data)));
}

void RemoteAgentClient::HandleSubscribeResult(const Payload_SubscribeResult& subscribe_result) {
    ALOGD("RemoteAgentClient(%s): Subscribe result for %s, Result: %u, message size: %u, message "
          "count %u, has_last_message: %u",
          peer_vm_name_.c_str(), subscribe_result.topic_name_.c_str(), subscribe_result.result_,
          subscribe_result.message_size_, subscribe_result.message_count_,
          subscribe_result.has_last_message_);

    const auto& topic_name = subscribe_result.topic_name_;

    std::unique_lock<std::mutex> guard(lock_);
    // Ensure the topic is one we've actually imported.
    auto subscription_it = subscriptions_.find(topic_name);
    if (subscription_it == subscriptions_.end()) {
        ALOGE("RemoteAgentClient(%s): Subscribe result for unrequested topic %s. Ignoring.",
              topic_name.c_str(), peer_vm_name_.c_str());
        // TODO(b/432280220): Send unsubscribe request
        return;
    }

    auto& subscription = subscription_it->second;
    auto& topic = subscription.topic;

    // This cleanup will run any time we return without disabling it, which happens upon success.
    // This lets us early return from any failure without having to duplicate cleanup steps at each.
    android::base::ScopeGuard cleanup_on_failure_guard([&]() {
        subscriptions_.erase(subscription_it);
        guard.unlock();
        // TODO(b/432280220): Send unsubscribe request
        if (topic.IsActive()) {
            topic_manager_.UnregisterRemoteTopic(subscribe_result.topic_name_);
        }
    });

    if (!topic.IsActive()) {
        ALOGW("RemoteAgentClient(%s): Subscribe result for inactive topic %s. Ignoring.",
              peer_vm_name_.c_str(), topic_name.c_str());
        return;
    }

    if (subscribe_result.result_ != Payload_SubscribeResult::Result_Ok) {
        ALOGE("RemoteAgentClient(%s): Subscribe to %s failed.  Destroying local topic.",
              peer_vm_name_.c_str(), topic_name.c_str());
        return;
    }

    auto& metadata = topic.GetMetadata();
    if (subscribe_result.message_size_ != metadata.message_size ||
        subscribe_result.message_count_ != metadata.message_count) {
        ALOGE("RemoteAgentClient(%s): %s metadata mismatch: Local: %zu bytes x %zu msgs; "
              "Remote: %u bytes x %u msgs.  Destroying local topic.",
              peer_vm_name_.c_str(), topic_name.c_str(), metadata.message_size,
              metadata.message_count, subscribe_result.message_size_,
              subscribe_result.message_count_);
        return;
    }

    if (subscribe_result.has_last_message_) {
        if (subscribe_result.last_message_content_.Size() != metadata.message_size) {
            ALOGE("RemoteAgentClient(%s): Deserialization assertion failure!  Received "
                  "SubscribeResult with has_last_message_ true but wrong "
                  "last_message_content_.Size()!",
                  peer_vm_name_.c_str());
            return;
        }
        if (!topic.Write(subscribe_result.last_message_content_.Ptr(),
                         subscribe_result.message_size_)) {
            ALOGE("RemoteAgentClient(%s): Unable to write data to %s!", peer_vm_name_.c_str(),
                  topic_name.c_str());
            return;
        }
        topic.NotifyWritten(1);
    }

    subscription.is_active = true;
    guard.unlock();

    // We're succeeding, so we don't want the failure cleanup to run.
    cleanup_on_failure_guard.Disable();

    topic.ReportReady();
}

void RemoteAgentClient::HandleTopicRawData(const Payload_TopicRawData& topic_raw_data) {
    const auto& topic_name = topic_raw_data.topic_name_;

    std::unique_lock guard(lock_);
    auto subscription_it = subscriptions_.find(topic_name);
    if (subscription_it == subscriptions_.end()) {
        ALOGE("RemoteAgentClient(%s): Got topic data for unsubscribed topic %s.",
              peer_vm_name_.c_str(), topic_name.c_str());
        // TODO(b/432280220): Send unsubscribe request
        return;
    }
    auto& subscription = subscription_it->second;
    guard.unlock();

    if (!subscription.is_active) {
        ALOGW("RemoteAgentClient(%s): Got data for %s before Subscribe result.  Ignoring.",
              peer_vm_name_.c_str(), topic_name.c_str());
        return;
    }

    auto& topic = subscription.topic;
    if (!topic.IsActive()) {
        ALOGW("RemoteAgentClient(%s): Got data for inactive topic %s.  Destroying subscription.",
              peer_vm_name_.c_str(), topic_name.c_str());
        // TODO(b/432280220): Send unsubscribe request
        guard.lock();
        subscriptions_.erase(topic_name);
        return;
    }

    if (!topic.Write(topic_raw_data.buffer_.Ptr(), topic_raw_data.data_size_)) {
        ALOGW("RemoteAgentClient(%s): Unable to write data to %s.", peer_vm_name_.c_str(),
              topic_name.c_str());
        return;
    }
    topic.NotifyWritten(topic_raw_data.data_size_ / topic.GetMetadata().message_size);
}

void RemoteAgentClient::HandleTopicDestroyed(const Payload_TopicDestroyed& topic_destroyed) {
    const auto& topic_name = topic_destroyed.topic_name_;
    std::unique_lock guard(lock_);
    auto extracted = subscriptions_.extract(topic_name);
    if (extracted.empty()) {
        ALOGW("RemoteAgentClient(%s): TopicDestroyed for unsubscribed topic %s",
              peer_vm_name_.c_str(), topic_name.c_str());
        return;
    }
    if (!extracted.mapped().is_active) {
        ALOGW("RemoteAgentClient(%s): TopicDestroyed for inactive subscription %s",
              peer_vm_name_.c_str(), topic_name.c_str());
        // This shouldn't happen since server should send SubscribeResult before anything else,  but
        // the peer DT agent for this client is the "authority" for the given topic given that we
        // tried to import it from them, so we'll fall through and unregister the topic.
    }
    guard.unlock();
    topic_manager_.UnregisterRemoteTopic(topic_name);
}

void RemoteAgentClient::HandleConnectionMessages() {
    ALOGD("RemoteAgentClient(%s)::HandleConnectionMessages starting", peer_vm_name_.c_str());
    RawBuffer send_buffer(kDefaultBufferSize);
    // ReceivePayload() may later increase the buffer size if needed
    RawBuffer receive_buffer(kDefaultBufferSize);
    BufferWriter receive_writer(receive_buffer);

    while (true) {
        auto [parse_result, payload] = ReceivePayload(*connection_, nullptr, receive_writer);
        if (parse_result == proto::Parcel::ParseResult::UnknownPayload) {
            ALOGW("RemoteAgentClient(%s): Received unknown payload", peer_vm_name_.c_str());
            continue;
        }

        if (parse_result != proto::Parcel::ParseResult::Success) {
            ALOGE("RemoteAgentClient(%s): ReceivePayload failed", peer_vm_name_.c_str());
            break;
        }

        if (const auto* last_message_result = payload->As<Payload_LastMessageResult>()) {
            HandleLastMessageResult(std::move(*last_message_result));
        } else if (const auto* subscribe_result = payload->As<Payload_SubscribeResult>()) {
            HandleSubscribeResult(*subscribe_result);
        } else if (const auto* topic_raw_data = payload->As<Payload_TopicRawData>()) {
            HandleTopicRawData(*topic_raw_data);
        } else if (const auto* topic_destroyed = payload->As<Payload_TopicDestroyed>()) {
            HandleTopicDestroyed(*topic_destroyed);
        } else {
            ALOGE("RemoteAgentClient(%s): Received unexpected payload type %d",
                  peer_vm_name_.c_str(), payload->type_);
        }
    }

    ALOGD("RemoteAgentClient(%s): HandleConnection exiting", peer_vm_name_.c_str());
}

void RemoteAgentClient::DeactivateConnection() {
    std::lock_guard<std::mutex> guard(lock_);

    // Serve all the last message requests and delete them
    for (auto& it : last_message_requests_) {
        it.second.promise_.set_value(
                base::unexpected(::sdv::SdvStatus(::sdv::SdvStatusCode::kInternal,
                                                  std::format("Lost peer connection"))));
    }
    last_message_requests_.clear();

    // All subscriptions become inactive.
    for (auto& subscription : subscriptions_) {
        subscription.second.is_active = false;
    }

    // Return to CONNECT_WAIT if this wasn't a requested disconnect.
    if (client_state_ != ClientState::TERMINATING) {
        client_state_ = ClientState::CONNECT_WAIT;
    }
}

void RemoteAgentClient::OnConnectionManagerEvent(
        ::sdv::SdvResult<std::unique_ptr<IConnection>> connection_result) {
    std::unique_lock guard(lock_);
    pending_connection_result_.emplace(std::move(connection_result));
    guard.unlock();
    pending_connection_ready_.notify_one();
}

void RemoteAgentClient::ConnectionThread(ConnectionManager& connection_manager,
                                         const RemoteAgentHandshaker& handshaker) {
    ALOGD("RemoteAgentClient(%s): ConnectionThread starting", peer_vm_name_.c_str());

    // Loop to keep retrying connections until termination is requested.
    bool first_iteration = true;
    while (client_state_ != ClientState::TERMINATING) {
        if (!first_iteration) {
            // If this isn't our first time through the loop, sleep briefly before trying again so
            // that we don't spam the logs as badly with failure messages.
            std::this_thread::sleep_for(kReconnectDelay);
        }
        first_iteration = false;

        // Initiate outbound connect
        ALOGD("Connecting to %s @ %s", peer_vm_name_.c_str(), peer_address_.ToString().c_str());
        connection_manager.Connect(peer_address_, MakeFqinString(peer_identity_.name), *this);

        // Wait for connect result
        std::unique_lock guard(lock_);
        pending_connection_ready_.wait(guard, [&]() -> bool {
            // We don't check client_state_ because we need to hang around long enough for the
            // Connect call above to fire OnConnectionManagerEvent, or we'll get a segfault.
            return pending_connection_result_.has_value();
        });

        // Move pending_connection_result_ contents into local storage.
        auto connection_result = std::exchange(pending_connection_result_, std::nullopt);
        guard.unlock();

        if (client_state_ == ClientState::TERMINATING) {
            break;
        }
        if (!connection_result->has_value()) {
            ALOGE("RemoteAgentClient(%s): Connect() error: %s", peer_vm_name_.c_str(),
                  connection_result->error().toString().c_str());
            continue;
        }

        if (!handshaker.PerformHandshake(*connection_result->value(), peer_vm_name_)) {
            ALOGE("RemoteAgentClient(%s): Failed handshake", peer_vm_name_.c_str());
            continue;
        }

        connection_ = std::move(connection_result->value());
        ActivateConnection();
        HandleConnectionMessages();
        DeactivateConnection();

        // Do possibly blocking delete in this thread to avoid having it happen in binder.
        connection_.reset();
    }

    std::lock_guard guard(lock_);

    client_state_ = ClientState::TERMINATED;
    ALOGD("RemoteAgentClient(%s): ConnectionThread exiting", peer_vm_name_.c_str());
}

}  // namespace android::sdv::dt::agent
