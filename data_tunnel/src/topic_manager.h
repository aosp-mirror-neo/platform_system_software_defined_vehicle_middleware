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

#include "topic.h"

#include <libsdv_dt_core.h>

#include <map>
#include <memory>
#include <mutex>
#include <vector>

namespace android::sdv::dt::agent {

class TopicManager {
public:
    Topic* RegisterTopic(const Topic::Metadata& metadata, const core::ClientId& client_id,
                         bool is_remote, std::unique_ptr<AidlMQ> mq);

    bool UnregisterTopic(core::TopicId topic_id);

    // Notifying all subscribers that new data is available for reading
    bool NotifyWritten(core::TopicId topic_id, size_t written);

    Topic* FindTopic(const std::string& topic_name, bool local_only) const;

    Topic* FindTopicById(core::TopicId topic_id) const;

    bool UnregisterRemoteTopic(const std::string& topic_name);

    void ClientDisconnected(const core::ClientId& client_id);

    bool DumpStats(int fd, int32_t topic_id);

private:
    static void DumpTopicEntryHeader(int fd);
    static void DumpTopicEntry(int fd, const Topic& topic);

    // This method must be called only with active_topics_lock_ locked
    bool UnregisterTopicUnsafe(const std::string& topic_name, const std::lock_guard<std::mutex>&);

    // This method must be called only with active_topics_lock_ locked
    Topic* FindTopicUnsafe(const std::string& topic_name, bool local_only,
                           const std::lock_guard<std::mutex>&) const;

    // Locally available topics.
    // topic_id_to_name_map_, active_topics_ and topic_id_counter_ can be accessed only with
    // active_topics_lock_ locked
    int32_t topic_id_counter_ = 0;
    std::map<core::TopicId, std::string> topic_id_to_name_map_;
    std::map<std::string, std::unique_ptr<Topic>> active_topics_;
    mutable std::mutex active_topics_lock_;

    // Deleted/inactive topics.
    // deleted_topics_ can be accessed only with deleted_topics_lock_ locked
    std::vector<std::unique_ptr<Topic>> deleted_topics_;
    std::mutex deleted_topics_lock_;
};

}  // namespace android::sdv::dt::agent
