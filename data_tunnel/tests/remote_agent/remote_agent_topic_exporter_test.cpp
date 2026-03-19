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

#include "remote_agent/remote_agent_topic_exporter.h"

#include "connection.h"
#include "proto/dt_proto.h"
#include "remote_agent/remote_agent_test_utils.h"
#include "topic_manager.h"

#include <gtest/gtest.h>

#include <initializer_list>

using ::testing::_;
using ::testing::Address;
using ::testing::AllArgs;
using ::testing::Eq;
using ::testing::Mock;
using ::testing::MockFunction;
using ::testing::Return;
using ::testing::StrictMock;

namespace android::sdv::dt::agent::test {

using proto::Payload_SubscribeRequest;
using proto::Payload_SubscribeResult;
using proto::Payload_TopicDestroyed;
using proto::Payload_TopicRawData;

namespace {

static const char* kTestVmName = "testvm2";
static const char* kTestTopicName = "test-topic-name";
static uint32_t kTestTopicSubId = 42;

class RemoteAgentTopicExporterTest : public ::testing::Test {
protected:
    RemoteAgentTopicExporterTest() :
          exporter_(kTestVmName, topic_manager_, connection_), send_buffer_(4096) {}

    using TestMessageType = int;
    static constexpr size_t kTestMessageSize = sizeof(TestMessageType);
    static constexpr size_t kTestMessageCount = 4;

    // For testing handling of messages bigger than default buffer sizes.
    struct BigMessageType {
        uint8_t data[32768];
    };
    static constexpr size_t kBigMessageSize = sizeof(BigMessageType);

    void CreateTestTopic(bool is_remote, size_t message_size = kTestMessageSize,
                         size_t message_count = kTestMessageCount) {
        Topic::Metadata metadata(kTestTopicName, message_size, message_count);
        auto mq = std::make_unique<AidlMQ>(message_size * message_count);
        topic_ = topic_manager_.RegisterTopic(metadata, /* client_id = */ 0, is_remote,
                                              std::move(mq));
    }

    void PrepareActiveSubscription() {
        CreateTestTopic(/* is_remote= */ false);

        SimulateSubscribeRequest();

        auto sub_result_payload = connection_.GetNextSentDataIfPayload<Payload_SubscribeResult>();
        ASSERT_TRUE(sub_result_payload);
        ASSERT_EQ(sub_result_payload->subscriber_pub_id, kTestTopicSubId);
        ASSERT_EQ(sub_result_payload->result, Payload_SubscribeResult::Result_Ok);
        ASSERT_EQ(sub_result_payload->message_size, kTestMessageSize);
        ASSERT_EQ(sub_result_payload->message_count, kTestMessageCount);
    }

    void DestroyTestTopic() { topic_manager_.UnregisterTopic(topic_->GetId()); }

    void WriteTestTopicRawData(const void* data, size_t bytes) {
        ASSERT_NE(topic_, nullptr);
        ASSERT_NE(data, nullptr);
        ASSERT_NE(bytes, 0);

        auto message_size = topic_->GetMetadata().message_size;
        ASSERT_EQ(bytes % message_size, 0);
        auto message_count = bytes / message_size;

        // Break the writes into multiple calls so that underlying MQ write doesn't
        // fail in the overflow case.
        for (size_t offset = 0; offset < bytes; offset += message_size) {
            ASSERT_TRUE(
                    topic_->Write(reinterpret_cast<const uint8_t*>(data) + offset, message_size));
        }
        topic_->NotifyWritten(message_count);
    }

    void WriteTestTopicMessages(std::initializer_list<TestMessageType> messages) {
        WriteTestTopicRawData(std::data(messages), kTestMessageSize * messages.size());
    }

    void SimulateSubscribeRequest() {
        exporter_.HandleSubscribeRequest(Payload_SubscribeRequest(kTestTopicName, kTestTopicSubId),
                                         send_buffer_);
    }

    // Ownership of the connection will be handed into the RemoteAgentServerSession, but tests
    // retain a raw pointer so they can interact with it.
    TopicManager topic_manager_;
    FakeConnection connection_;
    RemoteAgentTopicExporter exporter_;
    Topic* topic_ = nullptr;
    RawBuffer send_buffer_;
};

TEST_F(RemoteAgentTopicExporterTest, HandleSubscribeRequest_SendsFailureForUnknownTopic) {
    // No topic created.

    SimulateSubscribeRequest();

    auto payload = connection_.GetNextSentDataIfPayload<Payload_SubscribeResult>();
    ASSERT_TRUE(payload);
    EXPECT_EQ(payload->subscriber_pub_id, kTestTopicSubId);
    EXPECT_EQ(payload->result, Payload_SubscribeResult::Result_Failure);
}

TEST_F(RemoteAgentTopicExporterTest, HandleSubscribeRequest_SendsFailureForRemoteTopic) {
    CreateTestTopic(/* is_remote= */ true);

    SimulateSubscribeRequest();

    auto payload = connection_.GetNextSentDataIfPayload<Payload_SubscribeResult>();
    ASSERT_TRUE(payload);
    EXPECT_EQ(payload->subscriber_pub_id, kTestTopicSubId);
    EXPECT_EQ(payload->result, Payload_SubscribeResult::Result_Failure);
}

TEST_F(RemoteAgentTopicExporterTest, HandleSubscribeRequest_SendsSuccessAndAlreadyAvailableData) {
    CreateTestTopic(/* is_remote= */ false);

    WriteTestTopicMessages({15, 25});

    SimulateSubscribeRequest();

    auto sub_result_payload = connection_.GetNextSentDataIfPayload<Payload_SubscribeResult>();
    ASSERT_TRUE(sub_result_payload);
    EXPECT_EQ(sub_result_payload->subscriber_pub_id, kTestTopicSubId);
    EXPECT_EQ(sub_result_payload->result, Payload_SubscribeResult::Result_Ok);
    EXPECT_EQ(sub_result_payload->message_size, kTestMessageSize);
    EXPECT_EQ(sub_result_payload->message_count, kTestMessageCount);
    ASSERT_EQ(sub_result_payload->has_last_message, 1);
    ASSERT_EQ(sub_result_payload->last_message_content.Size(), kTestMessageSize);
    const auto* messages = reinterpret_cast<const TestMessageType*>(
            sub_result_payload->last_message_content.Ptr());
    EXPECT_EQ(messages[0], 25);
}

TEST_F(RemoteAgentTopicExporterTest, HandleSubscribeRequest_SendsSuccessAndNoDataForEmptyTopic) {
    CreateTestTopic(/* is_remote= */ false);

    SimulateSubscribeRequest();

    auto sub_result_payload = connection_.GetNextSentDataIfPayload<Payload_SubscribeResult>();
    ASSERT_TRUE(sub_result_payload);
    EXPECT_EQ(sub_result_payload->subscriber_pub_id, kTestTopicSubId);
    EXPECT_EQ(sub_result_payload->result, Payload_SubscribeResult::Result_Ok);
    EXPECT_EQ(sub_result_payload->message_size, kTestMessageSize);
    EXPECT_EQ(sub_result_payload->message_count, kTestMessageCount);
    ASSERT_EQ(sub_result_payload->has_last_message, 0);
}

TEST_F(RemoteAgentTopicExporterTest, ActiveSubscription_SendsNewlyWrittenDataAndTopicDestroyed) {
    PrepareActiveSubscription();

    WriteTestTopicMessages({15, 25});

    auto data_payload = connection_.GetNextSentDataIfPayload<Payload_TopicRawData>();
    ASSERT_TRUE(data_payload);
    EXPECT_EQ(data_payload->subscriber_pub_id, kTestTopicSubId);
    EXPECT_EQ(data_payload->data_size, 2 * kTestMessageSize);
    EXPECT_GE(data_payload->buffer.Size(), 2 * kTestMessageSize);
    const auto* messages = reinterpret_cast<const TestMessageType*>(data_payload->buffer.Ptr());
    EXPECT_EQ(messages[0], 15);
    EXPECT_EQ(messages[1], 25);

    DestroyTestTopic();

    auto destroyed_payload = connection_.GetNextSentDataIfPayload<Payload_TopicDestroyed>();
    ASSERT_TRUE(destroyed_payload);
    EXPECT_EQ(destroyed_payload->subscriber_pub_id, kTestTopicSubId);
}

TEST_F(RemoteAgentTopicExporterTest, ActiveSubscription_RecoversFromOverflow) {
    PrepareActiveSubscription();

    // Writing more than kTestMessageCount (4) messages will trigger reader overflow.
    WriteTestTopicMessages({15, 25, 35, 45, 55});

    // Overflow handling resets read pointer a half-buffer distance behind the write pointer,
    // so we get a half-buffer worth of messages, here.
    auto data_payload = connection_.GetNextSentDataIfPayload<Payload_TopicRawData>();
    ASSERT_TRUE(data_payload);
    EXPECT_EQ(data_payload->subscriber_pub_id, kTestTopicSubId);
    EXPECT_EQ(data_payload->data_size, 2 * kTestMessageSize);
    EXPECT_GE(data_payload->buffer.Size(), 2 * kTestMessageSize);
    const auto* messages = reinterpret_cast<const TestMessageType*>(data_payload->buffer.Ptr());
    EXPECT_EQ(messages[0], 45);
    EXPECT_EQ(messages[1], 55);
}

TEST_F(RemoteAgentTopicExporterTest, HandleSubscribeRequest_WorksForLargeMessages) {
    BigMessageType big_messages[2];

    CreateTestTopic(/* is_remote= */ false, /* message_size= */ kBigMessageSize);

    // Write a message to the topic to be included in SubscribeResult.
    memset(&big_messages[0], 1, kBigMessageSize);
    WriteTestTopicRawData(big_messages, sizeof(BigMessageType));

    SimulateSubscribeRequest();

    auto sub_result_payload = connection_.GetNextSentDataIfPayload<Payload_SubscribeResult>();
    ASSERT_TRUE(sub_result_payload);
    EXPECT_EQ(sub_result_payload->subscriber_pub_id, kTestTopicSubId);
    EXPECT_EQ(sub_result_payload->result, Payload_SubscribeResult::Result_Ok);
    EXPECT_EQ(sub_result_payload->message_size, kBigMessageSize);
    EXPECT_EQ(sub_result_payload->message_count, kTestMessageCount);
    EXPECT_EQ(sub_result_payload->has_last_message, 1);
    EXPECT_EQ(sub_result_payload->last_message_content.Size(), kBigMessageSize);
    EXPECT_EQ(memcmp(sub_result_payload->last_message_content.Ptr(), &big_messages[0],
                     kBigMessageSize),
              0);

    // Write two messages to the topic.
    memset(&big_messages[0], 3, kBigMessageSize);
    memset(&big_messages[1], 4, kBigMessageSize);
    WriteTestTopicRawData(big_messages, sizeof(big_messages));

    // Expect two parcels; messages > 8kb are always sent one per parcel.
    auto data_payload_1 = connection_.GetNextSentDataIfPayload<Payload_TopicRawData>();
    ASSERT_TRUE(data_payload_1);
    EXPECT_EQ(data_payload_1->subscriber_pub_id, kTestTopicSubId);
    EXPECT_EQ(data_payload_1->data_size, kBigMessageSize);
    ASSERT_GE(data_payload_1->buffer.Size(), kBigMessageSize);
    EXPECT_EQ(memcmp(data_payload_1->buffer.Ptr(), &big_messages[0], kBigMessageSize), 0);

    auto data_payload_2 = connection_.GetNextSentDataIfPayload<Payload_TopicRawData>();
    ASSERT_TRUE(data_payload_2);
    EXPECT_EQ(data_payload_2->subscriber_pub_id, kTestTopicSubId);
    EXPECT_EQ(data_payload_2->data_size, kBigMessageSize);
    ASSERT_GE(data_payload_2->buffer.Size(), kBigMessageSize);
    EXPECT_EQ(memcmp(data_payload_2->buffer.Ptr(), &big_messages[1], kBigMessageSize), 0);
}

TEST_F(RemoteAgentTopicExporterTest, HandleSubscribeRequest_IsNoOpForExistingSubscription) {
    PrepareActiveSubscription();

    // Check that subscription working
    WriteTestTopicMessages({16});
    auto data_payload = connection_.GetNextSentDataIfPayload<Payload_TopicRawData>();
    ASSERT_TRUE(data_payload);
    const auto* messages = reinterpret_cast<const TestMessageType*>(data_payload->buffer.Ptr());
    EXPECT_EQ(messages[0], 16);

    // Send another request for the same subscription.
    SimulateSubscribeRequest();

    // No additional subscribe result.
    ASSERT_FALSE(connection_.GetNextSentDataIfPayload<Payload_SubscribeResult>());

    // Check that subscription behavior unchanged
    WriteTestTopicMessages({17});
    auto data_payload_2 = connection_.GetNextSentDataIfPayload<Payload_TopicRawData>();
    ASSERT_TRUE(data_payload_2);
    const auto* messages_2 = reinterpret_cast<const TestMessageType*>(data_payload_2->buffer.Ptr());
    EXPECT_EQ(messages_2[0], 17);

    // Only see the one raw data message
    ASSERT_FALSE(connection_.GetNextSentData(/*timeout_seconds=*/1));
}

TEST_F(RemoteAgentTopicExporterTest, ProcessTopicDataAvailable_BreaksUpLargeDataBlocks) {
    auto messages_per_payload =
            (RemoteAgentTopicExporter::kTopicRawDataSplitThreshold / kTestMessageSize);
    auto max_payload_size = messages_per_payload * kTestMessageSize;

    // Create topic with FMQ big enough to accommodate multiple payloads of data, since we need
    // to do one big write with multiple payloads worth of data.
    CreateTestTopic(/*is_remote=*/false, /*message_size=*/kTestMessageSize,
                    /*message_count=*/messages_per_payload * 8);

    SimulateSubscribeRequest();

    auto sub_result_payload = connection_.GetNextSentDataIfPayload<Payload_SubscribeResult>();
    ASSERT_TRUE(sub_result_payload);
    ASSERT_EQ(sub_result_payload->result, Payload_SubscribeResult::Result_Ok);

    // Write a single message to the topic.  By waiting for the TopicRawData that's sent because of
    // this we can ensure that the large write below doesn't start before the ProcessingThread has
    // done its initial poll of the FMQ, which can lead to spurious test failures.
    WriteTestTopicMessages({19});

    // Wait for the first data packet; once we get this we know ProcessingThread is done with its
    // initial FMQ poll and waiting for a notification.
    auto data_payload_0 = connection_.GetNextSentDataIfPayload<Payload_TopicRawData>();
    ASSERT_TRUE(data_payload_0);

    // Build write big enough to constitute 2 full payloads and one half.
    auto write_size = 5 * max_payload_size / 2;
    std::vector<uint8_t> write_data(write_size);
    // Fill with a pattern whose repetition won't align with payload split boundaries.
    for (int i = 0; i < write_size; i++) {
        write_data[i] = i % 251;
    }

    // Send it all in one go so there's only one notification.
    WriteTestTopicRawData(write_data.data(), write_size);

    // Expect 3 parcels:  Two full and one for the remainder
    auto data_payload_1 = connection_.GetNextSentDataIfPayload<Payload_TopicRawData>();
    ASSERT_TRUE(data_payload_1);
    EXPECT_EQ(data_payload_1->subscriber_pub_id, kTestTopicSubId);
    EXPECT_EQ(data_payload_1->data_size, max_payload_size);
    ASSERT_GE(data_payload_1->buffer.Size(), max_payload_size);
    EXPECT_EQ(memcmp(data_payload_1->buffer.Ptr(), write_data.data(), max_payload_size), 0);

    auto data_payload_2 = connection_.GetNextSentDataIfPayload<Payload_TopicRawData>();
    ASSERT_TRUE(data_payload_2);
    EXPECT_EQ(data_payload_2->subscriber_pub_id, kTestTopicSubId);
    EXPECT_EQ(data_payload_2->data_size, max_payload_size);
    ASSERT_GE(data_payload_2->buffer.Size(), max_payload_size);
    EXPECT_EQ(memcmp(data_payload_2->buffer.Ptr(), write_data.data() + max_payload_size,
                     max_payload_size),
              0);

    auto tail_payload_size = write_size - (2 * max_payload_size);
    auto data_payload_3 = connection_.GetNextSentDataIfPayload<Payload_TopicRawData>();
    ASSERT_TRUE(data_payload_3);
    EXPECT_EQ(data_payload_3->subscriber_pub_id, kTestTopicSubId);
    EXPECT_EQ(data_payload_3->data_size, tail_payload_size);
    ASSERT_GE(data_payload_3->buffer.Size(), tail_payload_size);
    EXPECT_EQ(memcmp(data_payload_3->buffer.Ptr(), write_data.data() + (2 * max_payload_size),
                     tail_payload_size),
              0);
}

}  // namespace

}  // namespace android::sdv::dt::agent::test
