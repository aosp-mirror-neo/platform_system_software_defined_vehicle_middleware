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

#include "topic_manager.h"

#include <gtest/gtest.h>

#include <memory>
#include <string>
#include <vector>

namespace android::sdv::dt::agent::test {

// Helper function to create Topic::Metadata
Topic::Metadata CreateTestTopicMetadata(const std::string& name, size_t message_size = 100,
                                        size_t message_count = 2) {
    return Topic::Metadata(name, message_size, message_count);
}

// Helper function to create a valid AidlMQ unique_ptr
std::unique_ptr<AidlMQ> CreateTestAidlMQ(size_t message_size, size_t message_count) {
    auto mq = std::make_unique<AidlMQ>(message_size * message_count);
    EXPECT_TRUE(mq->isValid());  // Ensure the created MQ is valid
    return mq;
}

class TopicManagerTest : public testing::Test {
protected:
    TopicManager tm_;
    core::ClientId client_id_1_ = 1001;
    core::ClientId client_id_2_ = 1002;
};

TEST_F(TopicManagerTest, RegisterTopic_Success) {
    const std::string topic_name = "test_topic_1";
    auto metadata = CreateTestTopicMetadata(topic_name);
    auto mq = CreateTestAidlMQ(metadata.message_size, metadata.message_count);

    Topic* topic = tm_.RegisterTopic(metadata, client_id_1_, false, std::move(mq));

    ASSERT_NE(topic, nullptr);
    EXPECT_EQ(topic->GetMetadata().name, topic_name);
    EXPECT_EQ(topic->GetClientId(), client_id_1_);
    EXPECT_FALSE(topic->GetIsRemote());

    // Verify FindTopic and FindTopicById
    EXPECT_EQ(tm_.FindTopic(topic_name, false), topic);
    EXPECT_EQ(tm_.FindTopicById(topic->GetId()), topic);
}

TEST_F(TopicManagerTest, RegisterTopic_DuplicateName_ReturnsNullptr) {
    const std::string topic_name = "test_topic_duplicate";
    auto metadata = CreateTestTopicMetadata(topic_name);
    auto mq1 = CreateTestAidlMQ(metadata.message_size, metadata.message_count);

    Topic* topic1 = tm_.RegisterTopic(metadata, client_id_1_, false, std::move(mq1));
    ASSERT_NE(topic1, nullptr);

    auto mq2 = CreateTestAidlMQ(metadata.message_size, metadata.message_count);
    Topic* topic2 = tm_.RegisterTopic(metadata, client_id_2_, false, std::move(mq2));

    EXPECT_EQ(topic2, nullptr);  // Should fail to register duplicate
}

TEST_F(TopicManagerTest, UnregisterTopic_Success) {
    const std::string topic_name = "test_topic_to_unregister";
    auto metadata = CreateTestTopicMetadata(topic_name);
    auto mq = CreateTestAidlMQ(metadata.message_size, metadata.message_count);

    Topic* topic = tm_.RegisterTopic(metadata, client_id_1_, false, std::move(mq));
    ASSERT_NE(topic, nullptr);
    core::TopicId topic_id = topic->GetId();

    bool unregistered = tm_.UnregisterTopic(topic_id);
    EXPECT_TRUE(unregistered);

    // Verify it's no longer found
    EXPECT_EQ(tm_.FindTopic(topic_name, false), nullptr);
    EXPECT_EQ(tm_.FindTopicById(topic_id), nullptr);
}

TEST_F(TopicManagerTest, UnregisterTopic_NonExistentId_ReturnsFalse) {
    bool unregistered = tm_.UnregisterTopic(9999);  // Non-existent ID
    EXPECT_FALSE(unregistered);
}

TEST_F(TopicManagerTest, FindTopic_LocalOnly) {
    const std::string local_topic_name = "local_topic";
    const std::string remote_topic_name = "remote_topic";

    tm_.RegisterTopic(CreateTestTopicMetadata(local_topic_name), client_id_1_, false,
                      CreateTestAidlMQ(100, 2));
    tm_.RegisterTopic(CreateTestTopicMetadata(remote_topic_name), client_id_1_, true,
                      CreateTestAidlMQ(100, 2));

    // Find local topic (local_only = true)
    EXPECT_NE(tm_.FindTopic(local_topic_name, true), nullptr);
    // Find remote topic (local_only = true) - should fail
    EXPECT_EQ(tm_.FindTopic(remote_topic_name, true), nullptr);

    // Find local topic (local_only = false)
    EXPECT_NE(tm_.FindTopic(local_topic_name, false), nullptr);
    // Find remote topic (local_only = false)
    EXPECT_NE(tm_.FindTopic(remote_topic_name, false), nullptr);
}

TEST_F(TopicManagerTest, UnregisterRemoteTopic_Success) {
    const std::string remote_topic_name = "remote_topic_to_unregister";
    auto metadata = CreateTestTopicMetadata(remote_topic_name);
    auto mq = CreateTestAidlMQ(metadata.message_size, metadata.message_count);

    Topic* topic = tm_.RegisterTopic(metadata, client_id_1_, true, std::move(mq));
    ASSERT_NE(topic, nullptr);

    bool unregistered = tm_.UnregisterRemoteTopic(remote_topic_name);
    EXPECT_TRUE(unregistered);

    // Verify it's no longer found
    EXPECT_EQ(tm_.FindTopic(remote_topic_name, false), nullptr);
    EXPECT_EQ(tm_.FindTopicById(topic->GetId()), nullptr);
}

TEST_F(TopicManagerTest, UnregisterRemoteTopic_NonExistentName_ReturnsFalse) {
    bool unregistered = tm_.UnregisterRemoteTopic("non_existent_remote_topic");
    EXPECT_FALSE(unregistered);
}

TEST_F(TopicManagerTest, ClientDisconnected_UnregistersAssociatedTopics) {
    // Client 1 topics
    tm_.RegisterTopic(CreateTestTopicMetadata("client1_topic_a"), client_id_1_, false,
                      CreateTestAidlMQ(100, 2));
    tm_.RegisterTopic(CreateTestTopicMetadata("client1_topic_b"), client_id_1_, false,
                      CreateTestAidlMQ(100, 2));
    // Client 2 topics
    tm_.RegisterTopic(CreateTestTopicMetadata("client2_topic_x"), client_id_2_, false,
                      CreateTestAidlMQ(100, 2));
    tm_.RegisterTopic(CreateTestTopicMetadata("client2_topic_y"), client_id_2_, false,
                      CreateTestAidlMQ(100, 2));
    // Remote topic (should not be unregistered by ClientDisconnected)
    tm_.RegisterTopic(CreateTestTopicMetadata("remote_topic_c1"), client_id_1_, true,
                      CreateTestAidlMQ(100, 2));

    // Disconnect client 1
    tm_.ClientDisconnected(client_id_1_);

    // Verify client 1 topics are gone
    EXPECT_EQ(tm_.FindTopic("client1_topic_a", false), nullptr);
    EXPECT_EQ(tm_.FindTopic("client1_topic_b", false), nullptr);

    // Verify client 2 topics remain
    EXPECT_NE(tm_.FindTopic("client2_topic_x", false), nullptr);
    EXPECT_NE(tm_.FindTopic("client2_topic_y", false), nullptr);

    // Verify remote topic associated with client 1 remains (it logs an error, but doesn't
    // unregister)
    EXPECT_NE(tm_.FindTopic("remote_topic_c1", false), nullptr);
}

TEST_F(TopicManagerTest, NotifyWritten_TriggersCallback) {
    const std::string topic_name = "notify_topic";
    auto metadata = CreateTestTopicMetadata(topic_name);
    auto mq = CreateTestAidlMQ(metadata.message_size, metadata.message_count);

    Topic* topic = tm_.RegisterTopic(metadata, client_id_1_, false, std::move(mq));
    ASSERT_NE(topic, nullptr);

    int callback_invocations = 0;
    topic->RegisterSubscriberCallback(
            [&callback_invocations](auto /*event_type*/, auto /*topic_id*/) {
                callback_invocations++;
            });

    EXPECT_TRUE(tm_.NotifyWritten(topic->GetId(), 1));
    EXPECT_FALSE(tm_.NotifyWritten(9999, 1));  // Non-existent ID
    EXPECT_EQ(callback_invocations, 1);
}

TEST_F(TopicManagerTest, DumpStats_DoesNotCrash) {
    // Register some topics
    tm_.RegisterTopic(CreateTestTopicMetadata("dump_topic_1"), client_id_1_, false,
                      CreateTestAidlMQ(100, 2));
    tm_.RegisterTopic(CreateTestTopicMetadata("dump_topic_2"), client_id_2_, true,
                      CreateTestAidlMQ(100, 2));
    Topic* topic_to_dump = tm_.RegisterTopic(CreateTestTopicMetadata("dump_topic_3"), client_id_1_,
                                             false, CreateTestAidlMQ(100, 2));
    ASSERT_NE(topic_to_dump, nullptr);

    // Unregister one to have deleted topics
    tm_.UnregisterTopic(topic_to_dump->GetId());

    // Test dumping all stats
    EXPECT_TRUE(tm_.DumpStats(STDOUT_FILENO, -1));

    // Test dumping specific topic
    EXPECT_TRUE(tm_.DumpStats(STDOUT_FILENO, topic_to_dump->GetId()));

    // Test dumping non-existent topic
    EXPECT_TRUE(tm_.DumpStats(STDOUT_FILENO, 9999));

    // Note: Actual output content verification is complex for unit tests
    // and usually done with integration/system tests or by redirecting stdout.
}

}  // namespace android::sdv::dt::agent::test