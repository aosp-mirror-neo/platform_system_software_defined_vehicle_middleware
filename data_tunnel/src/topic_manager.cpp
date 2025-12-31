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

#define LOG_TAG "sdv_dt_agent:topic_mgr"

#include "topic_manager.h"

#include "dumpsys_helper.h"

#include <android-base/logging.h>

namespace android::sdv::dt::agent {

Topic* TopicManager::RegisterTopic(const Topic::Metadata& metadata, const core::ClientId& client_id,
                                   bool is_remote, std::unique_ptr<AidlMQ> mq) {
    CHECK(mq) << "RegisterTopic: Got null message queue!";
    std::lock_guard<std::mutex> active_topics_guard(active_topics_lock_);
    auto topic_id = static_cast<core::TopicId>(topic_id_counter_++);
    ALOGD("Registering topic %s (is_remote=%d, id=%d)", metadata.name.c_str(), is_remote, topic_id);

    auto it = active_topics_.find(metadata.name);
    if (it == active_topics_.end()) {
        std::unique_ptr<Topic> new_topic =
                Topic::Create(metadata, topic_id, client_id, is_remote, std::move(mq));

        active_topics_.emplace(metadata.name, std::move(new_topic));
        ALOGD("Active topics size is now %zu", active_topics_.size());

        topic_id_to_name_map_[topic_id] = metadata.name;
        return active_topics_[metadata.name].get();

    } else {
        ALOGE("Publication %s already registered with id %08X!", metadata.name.c_str(),
              it->second->GetId());
        return nullptr;
    }
}

bool TopicManager::UnregisterTopic(core::TopicId topic_id) {
    std::lock_guard<std::mutex> active_topics_guard(active_topics_lock_);
    auto key_it = topic_id_to_name_map_.find(topic_id);
    if (key_it != topic_id_to_name_map_.end()) {
        return UnregisterTopicUnsafe(key_it->second, active_topics_guard);
    }
    ALOGE("%s: Unable to find topic with id %08X in the active topic list!", __FUNCTION__,
          topic_id);
    return false;
}

// Notifying all subscribers that new data is available for reading
bool TopicManager::NotifyWritten(core::TopicId topic_id, size_t written) {
    std::lock_guard<std::mutex> active_topics_guard(active_topics_lock_);
    if (topic_id_to_name_map_.count(topic_id) == 0) return false;
    return active_topics_[topic_id_to_name_map_[topic_id]]->NotifyWritten(written);
}

Topic* TopicManager::FindTopic(const std::string& topic_name, bool local_only) const {
    std::lock_guard<std::mutex> active_topics_guard(active_topics_lock_);
    return FindTopicUnsafe(topic_name, local_only, active_topics_guard);
}

Topic* TopicManager::FindTopicById(core::TopicId topic_id) const {
    std::lock_guard<std::mutex> active_topics_guard(active_topics_lock_);
    auto key_it = topic_id_to_name_map_.find(topic_id);
    if (key_it == topic_id_to_name_map_.end()) {
        return nullptr;
    }
    auto topic_it = active_topics_.find(key_it->second);
    return (topic_it != active_topics_.end()) ? topic_it->second.get() : nullptr;
}

bool TopicManager::UnregisterRemoteTopic(const std::string& topic_name) {
    std::lock_guard<std::mutex> active_topics_guard(active_topics_lock_);
    return UnregisterTopicUnsafe(topic_name, active_topics_guard);
}

void TopicManager::ClientDisconnected(const core::ClientId& client_id) {
    // Unregister all the client's publications
    std::lock_guard<std::mutex> active_topics_guard(active_topics_lock_);
    std::vector<std::string> publications_to_unregister;
    for (auto& [k, v] : active_topics_) {
        if (v->GetClientId() == client_id) {
            // the topic can't be remote
            if (v->GetIsRemote()) {
                ALOGE("Remote topic %s is associated with the local client %08X", k.c_str(),
                      client_id);
                continue;
            }
            publications_to_unregister.push_back(k);
        }
    }
    for (auto& v : publications_to_unregister) {
        if (!UnregisterTopicUnsafe(v, active_topics_guard)) {
            ALOGE("Unable to unregister %s", v.c_str());
        }
    }
}
void TopicManager::DumpTopicEntryHeader(int fd) {
    dumpsys::Dump(fd,
                  "\tpublication_id,client_id,name,message_size,message_count,buffer_size_bytes,"
                  "subscriber_callback_count,is_remote\n");
}

void TopicManager::DumpTopicEntry(int fd, const Topic& topic) {
    // TODO: [Performance] Fetch the topic stats with a single lock
    const auto& metadata = topic.GetMetadata();
    dumpsys::Dump(fd, "\t%08X,%08X,%s,%zu,%zu,%zu,%zu,%d\n", topic.GetId(), topic.GetClientId(),
                  metadata.name.c_str(), metadata.message_size, metadata.message_count,
                  metadata.message_size * metadata.message_count,
                  topic.GetSubscriberCallbackCount(), topic.GetIsRemote());
}

bool TopicManager::DumpStats(int fd, int32_t topic_id) {
    {
        std::lock_guard<std::mutex> active_topics_guard(active_topics_lock_);
        if (topic_id == -1) {
            dumpsys::Dump(fd, "\nPublication Manager. Active publications: %zu\n",
                          active_topics_.size());
        } else {
            dumpsys::Dump(fd, "\nPublication Manager. Active publication with id: %d\n", topic_id);
        }
        DumpTopicEntryHeader(fd);
        for (auto& v : active_topics_) {
            if (topic_id == -1 || topic_id == v.second->GetId()) {
                DumpTopicEntry(fd, *v.second);
                if (topic_id != -1) break;
            }
        }
    }

    if (topic_id == -1) {
        std::lock_guard<std::mutex> deleted_topics_guard(deleted_topics_lock_);
        dumpsys::Dump(fd, "\nPublication Manager. Deleted publications: %zu\n",
                      deleted_topics_.size());
        DumpTopicEntryHeader(fd);
        for (auto& v : deleted_topics_) {
            DumpTopicEntry(fd, *v);
        }
    }
    return true;
}

// This method must be called only with active_topics_lock_ locked
bool TopicManager::UnregisterTopicUnsafe(const std::string& topic_name,
                                         const std::lock_guard<std::mutex>&) {
    auto topic_it = active_topics_.find(topic_name);
    if (topic_it != active_topics_.end()) {
        std::lock_guard<std::mutex> deleted_topics_guard(deleted_topics_lock_);
        auto topic_id = topic_it->second->GetId();
        ALOGD("Unregistering topic %s with id %08X", topic_name.c_str(), topic_id);
        topic_it->second->Delete();
        deleted_topics_.push_back(std::move(topic_it->second));
        active_topics_.erase(topic_it);
        topic_id_to_name_map_.erase(topic_id);
        return true;
    }
    ALOGE("%s: Unable to find topic with name %s", __FUNCTION__, topic_name.c_str());
    return false;
}

// This method must be called only with active_topics_lock_ locked
Topic* TopicManager::FindTopicUnsafe(const std::string& topic_name, bool local_only,
                                     const std::lock_guard<std::mutex>&) const {
    for (auto& [k, v] : active_topics_) {
        if (local_only && v->GetIsRemote()) continue;
        // returns only the exact match, i.e. no substring search
        // b/355446632
        if (k.compare(topic_name) == 0) {
            return v.get();
        }
    }
    return nullptr;
}

}  // namespace android::sdv::dt::agent
