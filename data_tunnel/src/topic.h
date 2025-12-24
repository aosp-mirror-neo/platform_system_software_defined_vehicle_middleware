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

#include <aidl/google/sdv/data_tunnel/PublicationDescriptor.h>
#include <fmq/AidlMessageQueue.h>

#include <libsdv_dt_core.h>

#include <map>
#include <memory>
#include <mutex>
#include <vector>

namespace android::sdv::dt::agent {

using AidlMQ =
        AidlMessageQueue<int8_t, ::aidl::android::hardware::common::fmq::UnsynchronizedWrite>;
using AidlMQDescriptor = ::aidl::android::hardware::common::fmq::MQDescriptor<
        int8_t, ::aidl::android::hardware::common::fmq::UnsynchronizedWrite>;
using PublicationDescriptor = ::aidl::google::sdv::data_tunnel::PublicationDescriptor;

class Topic {
public:
    struct Metadata {
        Metadata(const std::string& name, size_t message_size, size_t message_count) :
              name(name), message_size(message_size), message_count(message_count) {}
        std::string name;
        size_t message_size;
        size_t message_count;

        // message_size can't be 0
        // message_count must at least 2 and must be even
        bool IsValid() const {
            return message_size > 0 && message_count > 1 && (message_count & 1) == 0 && name.size();
        }
    };

    // Thin wrapper around FMQ handle to provide operations common to internal readers.
    class Reader {
    public:
        Reader() = delete;
        Reader(const Reader&) = delete;
        Reader(Reader&&) = default;

        size_t AvailableToRead();
        bool Read(uint8_t* buffer_pointer, size_t bytes_to_read);

    private:
        friend class Topic;

        explicit Reader(std::unique_ptr<AidlMQ>);

        std::unique_ptr<AidlMQ> mq_;
    };

    Topic(const Topic&) = delete;
    Topic& operator=(const Topic&) = delete;

    static std::unique_ptr<Topic> Create(const Metadata& metadata, int32_t id,
                                         core::ClientId client_id, bool is_remote,
                                         std::unique_ptr<AidlMQ> mq);

    int32_t GetId() const;
    const Metadata& GetMetadata() const;

    // ID of the client that publishes this topic; only meaningful for local topics.
    core::ClientId GetClientId() const;

    // Indicates whether the topic's publisher is remote (lives on another VM) or local
    bool GetIsRemote() const;

    bool IsActive() const;

    // Make an AIDL PublicationDescriptor that we can return to a client over binder
    // to enable that client to subscribe to the publication.
    bool MakeDescriptor(PublicationDescriptor* publication_desc) const;

    // TODO(b/406556288):  Remove these methods and build LocalMQWriter for RemoteAgent to use.
    long Write(const uint8_t* buffer_pointer, size_t to_read);
    bool NotifyWritten(size_t /*written*/);

    std::optional<std::vector<uint8_t>> GetLastMessage();

    // Subscriber callbacks run when data is written to a topic or an error occurs and are used
    // to drive notifications to clients or peer VMs that have subscribed to a topic.
    //
    // These callbacks are executed within a dedicated thread in CommChannel that polls a client
    // the client pipe (see ClientManager::ReadCallback) or a dedicated thread in RemoteAgent
    // that polls peer GRPC connections (see RemoteAgent::ConnectionThread).
    //
    // Because these threads need to resume polling ASAP and because a lock is held on the topic
    // while the callbacks run, callbacks should not block or do any long-running computation.
    //
    // Callbacks are installed only for local and remote subscriptions:
    //
    //   - Local subscriber callbacks (see the free function SubscriberCallback) write messages
    //     to the client CommChannel; as long as the pipe buffer is not full this will not block.
    //   - Remote subscriptions callbacks signal another worker thread (see
    //     ExportBroker::BrokerThread and ExportBroker::ExportBrokerCallback) that will read data
    //     from the topic and write it to a peer VM.

    using Callback = std::function<void(core::CommChannel::Message::Type, core::TopicId)>;

    uint32_t RegisterSubscriberCallback(Callback callback);
    void UnregisterSubscriberCallback(uint32_t callback_id);
    size_t GetSubscriberCallbackCount() const;

    // Create a reader for use within DT Agent.
    std::optional<Reader> CreateReader();

private:
    friend class TopicManager;

    Topic(const Metadata& metadata, int32_t id, const core::ClientId& client_id, bool is_remote,
          std::unique_ptr<AidlMQ> mq);

    // Returns an empty unique_ptr on error.
    std::unique_ptr<AidlMQ> CloneMQHandle();

    void Delete();

    const int32_t id_;
    const core::ClientId client_id_;
    const Metadata metadata_;
    const bool is_remote_;

    // Protects state in this class.
    mutable std::mutex lock_;

    // Topic state can be accessed from any thread
    enum class Status { Active, Deleted };
    std::atomic<Status> status_{Status::Active};

    // FMQ is thread safe, but our pointer to it is not.  Code that wants to
    // access mq_ must first acquire lock_;
    std::unique_ptr<AidlMQ> mq_;

    // subscriber_callbacks_ and subscriber_callbacks_id_counter_ can be
    // accessed only with lock_ locked.
    std::map<uint32_t, Callback> subscriber_callbacks_;
    uint32_t subscriber_callbacks_id_counter_ = 1;  // allocating 0 for error
};

}  // namespace android::sdv::dt::agent
