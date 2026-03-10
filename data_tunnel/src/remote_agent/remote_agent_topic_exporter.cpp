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

#include "remote_agent/remote_agent_topic_exporter.h"

namespace android::sdv::dt::agent {

using proto::Parcel;
using proto::Payload_SubscribeRequest;
using proto::Payload_SubscribeResult;
using proto::Payload_TopicDestroyed;
using proto::Payload_TopicRawData;

RemoteAgentTopicExporter::RemoteAgentTopicExporter(const std::string& vm_name,
                                                   TopicManager& topic_manager,
                                                   IConnection& connection) :
      vm_name_(vm_name), topic_manager_(topic_manager), connection_(connection) {
    for (size_t i = 0; i < kProcessingThreadCount; i++) {
        processing_threads_[i] = std::jthread([this]() { ProcessingThread(); });
    }
}

RemoteAgentTopicExporter::~RemoteAgentTopicExporter() {
    // Notify processing_threads_ that we're terminating and that they should exit.
    {
        std::unique_lock readiness_guard(readiness_lock_);
        ready_to_terminate_ = true;
    }
    readiness_cv_.notify_all();

    // Explicitly join all processing threads before any other member destruction so that they don't
    // try to access members that have been destroyed.
    for (auto& t : processing_threads_) {
        t.join();
    }

    // We need to explicitly clear exported_topics_ before the destructor exits and member
    // destruction starts, to ensure that topic notification callbacks are unregistered and we
    // won't get events that try to access data that has been destroyed.
    {
        std::unique_lock topics_guard(exported_topics_lock_);
        exported_topics_.clear();
    }
}

RemoteAgentTopicExporter::ExportedTopic::ExportedTopic(Topic& topic, int32_t sub_pub_id,
                                                       Topic::Reader reader, uint32_t callback_id,
                                                       size_t data_payload_capacity) :
      topic(topic),
      subscriber_pub_id(sub_pub_id),
      reader(std::move(reader)),
      callback_id(callback_id),
      data_payload(data_payload_capacity) {
    const auto& metadata = topic.GetMetadata();
    data_payload.subscriber_pub_id = subscriber_pub_id;
    required_send_buffer_size =
            Parcel::ParcelSize(Payload_TopicRawData::PayloadSize(data_payload_capacity));
}

RemoteAgentTopicExporter::ExportedTopic::ExportedTopic(ExportedTopic&& other) :
      topic(other.topic),
      subscriber_pub_id(other.subscriber_pub_id),
      reader(std::move(other.reader)),
      callback_id(other.callback_id),
      data_payload(std::move(other.data_payload)),
      required_send_buffer_size(other.required_send_buffer_size) {
    other.callback_id = 0;  // Take ownership of registered callback.
}

RemoteAgentTopicExporter::ExportedTopic::~ExportedTopic() {
    if (callback_id) {
        topic.UnregisterSubscriberCallback(callback_id);
    }
}

bool RemoteAgentTopicExporter::HandleSubscribeRequest(const Payload_SubscribeRequest& sub_request,
                                                      RawBuffer& send_buffer) {
    const auto& topic_name = sub_request.topic_name;
    const auto subscriber_pub_id = sub_request.subscriber_pub_id;
    ALOGD("RemoteAgentTopicExporter(%s): Request to subscribe to %s", vm_name_.c_str(),
          topic_name.c_str());

    // Failure payload, if needed.
    Payload_SubscribeResult result_payload;
    result_payload.subscriber_pub_id = subscriber_pub_id;
    result_payload.result = Payload_SubscribeResult::Result_Failure;

    auto topic = topic_manager_.FindTopic(topic_name, /*local_only*/ true);
    if (!topic) {
        return SendPayload(connection_, result_payload, send_buffer);
    }

    auto reader = topic->CreateReader();
    if (!reader) {
        ALOGE("RemoteAgentTopicExporter(%s): Unable to create Topic::Reader to export %s",
              vm_name_.c_str(), topic_name.c_str());
        return SendPayload(connection_, result_payload, send_buffer);
    }

    auto topic_id = topic->GetId();
    const auto& metadata = topic->GetMetadata();
    auto message_size = metadata.message_size;

    //  Compute data capacity for TopicRawData payloads for this topic.
    //    - For topics with message_size <= kTopicRawDataSplitThreshold we try to pack multiple
    //      messages per payload, and set the capacity to the largest multiple of message_size
    //      that fits within kTopicRawDataSplitThreshold.
    //    - For topics where message_size > kTopicRawDataSplitThreshold payload capcity is always
    //      exactly message_size.
    auto data_payload_capacity =
            std::max(message_size, (kTopicRawDataSplitThreshold / message_size) * message_size);

    // Get lock before registering callback to avoid having ProcessingThread try to process a
    // callback event before the entry is in the map.
    std::unique_lock guard(exported_topics_lock_);

    auto callback_id = topic->RegisterSubscriberCallback(
            [this](auto event_type, auto topic_id) { OnTopicEvent(event_type, topic_id); });
    if (!callback_id) {
        guard.unlock();
        ALOGE("RemoteAgentTopicExporter(%s): Unable to register callback for %s", vm_name_.c_str(),
              topic_name.c_str());
        return SendPayload(connection_, result_payload, send_buffer);
    }

    auto [entry, inserted] =
            exported_topics_.try_emplace(topic_id,
                                         ExportedTopic(*topic, subscriber_pub_id,
                                                       std::move(*reader), callback_id,
                                                       data_payload_capacity));

    guard.unlock();

    if (!inserted) {
        ALOGW("RemoteAgentTopicExporter(%s): Request for duplicate subscription to %s.",
              vm_name_.c_str(), topic_name.c_str());
        return true;
    }

    // In the success case we don't send the SubscribeResult; ProcessingThread handles it to
    // ensure that we don't send topic data before sending the result.

    // Fake a data available event to wake ProcessingThread to send SubscribeResult, as well as any
    // data already available.  (It's ok if there's no data; we just won't send a data parcel in
    // that case.)
    OnTopicEvent(core::CommChannel::Message::DataAvailable, topic_id);

    return true;  // No write done so no write failure to report.
}

void RemoteAgentTopicExporter::OnTopicEvent(int32_t event_type, int32_t topic_id) {
    std::unique_lock guard(readiness_lock_);
    if (event_type == core::CommChannel::Message::DataAvailable) {
        ready_topics_[topic_id].has_data_available = true;
    } else if (event_type == core::CommChannel::Message::Exception) {
        ready_topics_[topic_id].has_exception = true;
    } else {
        ALOGW("RemoteAgentTopicExporter::OnTopicEvent got unexpected event type %d for topic %d",
              event_type, topic_id);
        return;
    }
    guard.unlock();
    readiness_cv_.notify_one();
}

void RemoteAgentTopicExporter::MaybeSendSubscribeResult(ExportedTopic& exported_topic,
                                                        RawBuffer& send_buffer) {
    if (exported_topic.subscribe_result_sent) {
        return;
    }

    // Never try more than once.
    exported_topic.subscribe_result_sent = true;

    auto& metadata = exported_topic.topic.GetMetadata();
    Payload_SubscribeResult payload;
    payload.subscriber_pub_id = exported_topic.subscriber_pub_id;
    payload.result = Payload_SubscribeResult::Result_Ok;
    payload.message_size = metadata.message_size;
    payload.message_count = metadata.message_count;
    payload.has_last_message = 0;

    // Try to include an initial message if there is one available.
    if (exported_topic.reader.AvailableToRead() >= metadata.message_size) {
        payload.last_message_content.Resize(metadata.message_size, false);

        // Since reader would have just been created and its read pointer set one message behind the
        // write pointer, risk of an overflow is minimal.  But retry a few times in a loop anyways.
        static constexpr int kMaxAttempts = 3;
        for (int attempt_count = kMaxAttempts; attempt_count > 0; attempt_count--) {
            if (exported_topic.reader.Read(payload.last_message_content.Ptr(),
                                           metadata.message_size)) {
                payload.has_last_message = 1;
                break;
            } else {
                ALOGW("RemoteAgentTopicExporter(%s): Overflow while reading last message for "
                      "SubscribeResult for topic %s!  %d attempts remaining.  Consider increasing "
                      "message_count.",
                      vm_name_.c_str(), exported_topic.topic.GetMetadata().name.c_str(),
                      attempt_count - 1);
            }
        }
        if (!payload.has_last_message) {
            ALOGE("RemoteAgentTopicExporter(%s): Complete failure to read last message for "
                  "SubscribeResult for topic %s due to repeated overflows!  Consider "
                  "increasing message_count.",
                  vm_name_.c_str(), exported_topic.topic.GetMetadata().name.c_str());
        }
    }

    // Parcel size can be bigger than what we computed based on TopicRawData, so make sure the
    // buffer is big enough.
    auto parcel_size = Parcel::ParcelSize(payload);
    if (parcel_size > send_buffer.Size()) {
        send_buffer.Resize(parcel_size, /*copy_data=*/false);
    }
    if (!SendPayload(connection_, payload, send_buffer)) {
        ALOGE("RemoteAgentTopicExporter(%s): Unable to send SubscribeResult for %s",
              vm_name_.c_str(), exported_topic.topic.GetMetadata().name.c_str());
    }
}

void RemoteAgentTopicExporter::ProcessTopicDataAvailable(ExportedTopic& exported_topic,
                                                         RawBuffer& send_buffer) {
    auto& topic = exported_topic.topic;
    auto& reader = exported_topic.reader;
    auto& payload = exported_topic.data_payload;

    if (send_buffer.Size() < exported_topic.required_send_buffer_size) {
        send_buffer = RawBuffer(exported_topic.required_send_buffer_size);
    }

    size_t available = reader.AvailableToRead();
    bool has_overflowed = false;
    while (available > 0) {
        size_t read_size = std::min(available, payload.buffer.Size());
        if (!reader.Read(payload.buffer.Ptr(), read_size)) {
            if (has_overflowed) {
                ALOGE("RemoteAgentTopicExporter(%s): %s repeated overflow.", vm_name_.c_str(),
                      topic.GetMetadata().name.c_str());
                break;
            }
            has_overflowed = true;
            // Re-query available count after overflow read pointer repositioning
            available = reader.AvailableToRead();
            ALOGE("RemoteAgentTopicExporter(%s): Overflow on %s.  New available: %zu.",
                  vm_name_.c_str(), topic.GetMetadata().name.c_str(), available);
            continue;
        }

        payload.data_size = read_size;
        if (!SendPayload(connection_, payload, send_buffer)) {
            ALOGE("RemoteAgentTopicExporter(%s): Unable to send topic data for %s",
                  vm_name_.c_str(), topic.GetMetadata().name.c_str());
            break;
        }
        available -= read_size;
    }
}

void RemoteAgentTopicExporter::ProcessTopicException(ExportedTopic& exported_topic,
                                                     RawBuffer& send_buffer) {
    auto payload = Payload_TopicDestroyed(exported_topic.subscriber_pub_id);
    if (!SendPayload(connection_, payload, send_buffer)) {
        ALOGE("RemoteAgentTopicExporter(%s): Unable to send TopicDestroyed for %s",
              vm_name_.c_str(), exported_topic.topic.GetMetadata().name.c_str());
    }
}

void RemoteAgentTopicExporter::ProcessingThread() {
    std::unordered_map<int32_t, TopicReadiness> ready_topics;
    RawBuffer send_buffer(kDefaultBufferSize);

    while (true) {
        // Wait for something to become ready, and then swap ready sets into
        // locals for processing so we can release the readiness lock.
        std::unique_lock readiness_guard(readiness_lock_);
        readiness_cv_.wait(readiness_guard,
                           [this]() { return ready_to_terminate_ || !ready_topics_.empty(); });
        if (ready_to_terminate_) {
            break;
        }
        // Swap ready map into local instance so we can release readiness lock immediately.
        ready_topics.swap(ready_topics_);
        readiness_guard.unlock();

        bool any_topic_destroyed = false;
        for (auto& [topic_id, readiness] : ready_topics) {
            // Only need read lock to look up topic.
            std::shared_lock map_guard(exported_topics_lock_);
            auto exported_topic_it = exported_topics_.find(topic_id);
            if (exported_topic_it == exported_topics_.end()) {
                ALOGW("RemoteAgentTopicExporter(%s):Event reported for non-subscribed topic %d",
                      vm_name_.c_str(), topic_id);
                continue;
            }

            auto& exported_topic = exported_topic_it->second;

            // Lock topic so that another thread can't work on it and potentially send data out-
            // of-order.  Contention on any given topic's lock should be minimal.
            std::unique_lock topic_guard(exported_topic.lock);

            // Some other thread already marked the topic destroyed.
            if (exported_topic.destroyed) {
                continue;
            }

            // Can now release map lock because exported_topic reference is only invalidated on
            // delete that would require first getting the topic lock that we hold.
            map_guard.unlock();

            MaybeSendSubscribeResult(exported_topic, send_buffer);

            // We process data available first so that all available data is sent before
            // destroying the topic.
            if (readiness.has_data_available) {
                ProcessTopicDataAvailable(exported_topic, send_buffer);
            }
            if (readiness.has_exception) {
                ProcessTopicException(exported_topic, send_buffer);
                any_topic_destroyed = exported_topic.destroyed = true;
            }
        }
        ready_topics.clear();

        // Do a cleanup pass on the map if any topic was marked destroyed.
        // We can't directly delete entries above because we don't hold the map lock, and we can't
        // get the map lock at that point without risking a deadlock, since we hold the topic lock
        // and some other thread could be holding the map lock and trying to get the topic lock.
        if (any_topic_destroyed) {
            std::unique_lock map_guard(exported_topics_lock_);
            std::erase_if(exported_topics_, [](auto& entry) { return entry.second.destroyed; });
        }
    }
}

}  // namespace android::sdv::dt::agent
