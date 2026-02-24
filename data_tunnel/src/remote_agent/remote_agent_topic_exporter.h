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
#include "remote_agent/remote_agent_common.h"
#include "topic.h"
#include "topic_manager.h"

#include <shared_mutex>

namespace android::sdv::dt::agent {

// This class is responsible for maintaining all state associated with the export of topic data to a
// single peer VM, and for transmission of that data.
//
// It's meant only for use by RemoteAgentServerSession and is separate mainly to avoid piling more
// state and functionality into that class.
//
// It runs a processing thread to do the actual sending of topic data, and sets up a callback for
// each exported topic (which will run from the connection thread for the local client that
// publishes the topic) that notifies the processing thread whenever data is available for any
// exported publication.
class RemoteAgentTopicExporter final {
public:
    RemoteAgentTopicExporter(const std::string& vm_name, TopicManager&, IConnection&,
                             std::atomic<uint32_t>& parcel_id_counter);
    ~RemoteAgentTopicExporter();

    // Threshold at which we split topic data into multiple TopicRawData payloads.
    // For topics whose message size is larger than this we always send one message per payload
    // Public for testing.
    static constexpr uint32_t kTopicRawDataSplitThreshold = 16384;

    bool HandleSubscribeRequest(const proto::Payload_SubscribeRequest& topic_name,
                                RawBuffer& send_buffer);

private:
    // Holds state that RemoteAgentTopicExporter needs for each topic it exports to a specific peer.
    // Needed because we need a separate Reader (i.e. read pointer into the FMQ) for each peer. Also
    // serves as a "owner" for the callback we register with the Topic, ensuring that it's
    // unregistered when we're done with it.
    struct ExportedTopic final {
        Topic& topic;
        int32_t subscriber_pub_id;
        Topic::Reader reader;
        uint32_t callback_id;
        proto::Payload_TopicRawData data_payload;
        size_t required_send_buffer_size;
        bool subscribe_result_sent = false;
        // Lock to prevent out-of-order data delivery when we have multiple processing threads.
        std::mutex lock;
        bool destroyed = false;

        ExportedTopic(Topic& topic, int32_t subscriber_pub_id, Topic::Reader reader,
                      uint32_t callback_id, size_t data_payload_capacity);
        ExportedTopic(ExportedTopic&& o);
        ~ExportedTopic();
    };

    // Callback function to be registered with Topics that are exported.
    void OnTopicEvent(int32_t event_type, int32_t topic_id);

    // Processing functions for individual events recorded by OnTopicEvent.
    void MaybeSendSubscribeResult(ExportedTopic& exported_topic, RawBuffer& send_buffer);
    void ProcessTopicDataAvailable(ExportedTopic& exported_topic, RawBuffer& send_buffer);
    void ProcessTopicException(ExportedTopic& exported_topic, RawBuffer& send_buffer);

    // Run by processing_thread_.  Wakes when OnTopicEvent notifies it via condition variable that
    // new events are available for exported topics, and sends appropriate messages to the client.
    void ProcessingThread();

    const std::string vm_name_;
    TopicManager& topic_manager_;
    IConnection& connection_;
    std::atomic<uint32_t>& parcel_id_counter_;

    // Record of all currently exported topics.
    std::shared_mutex exported_topics_lock_;
    std::unordered_map<int32_t, ExportedTopic> exported_topics_;

    // This group of fields is used to queue work for ProcessingThread and wake it to do that work.
    //
    // We use use separate structures / lock from exported_topics_ so that the client connection
    // threads calling OnTopicEvent (which populates ready_topics_) don't block waiting for
    // ProcessingThread to send data.
    std::mutex readiness_lock_;
    std::condition_variable readiness_cv_;
    struct TopicReadiness {
        bool has_data_available = false;
        bool has_exception = false;
    };
    std::unordered_map<int32_t, TopicReadiness> ready_topics_;
    bool ready_to_terminate_ = false;  // Used to trigger ProcessingThread exit

    // Executes ProcessingThread; Because we block while sending, we run multiple processing
    // threads so that while one packet is being sent another thread can be working on
    // serializing the next.
    static constexpr size_t kProcessingThreadCount = 2;
    std::array<std::jthread, kProcessingThreadCount> processing_threads_;
};

}  // namespace android::sdv::dt::agent
