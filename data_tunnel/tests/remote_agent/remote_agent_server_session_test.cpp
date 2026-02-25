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

#include "remote_agent/remote_agent_server_session.h"

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

using proto::Payload_LastMessageRequest;
using proto::Payload_LastMessageResult;
using proto::Payload_SubscribeRequest;
using proto::Payload_SubscribeResult;
using proto::Payload_TopicDestroyed;
using proto::Payload_TopicRawData;

namespace {

static const char* kTestVmName = "testvm2";
static const char* kTestTopicName = "test-topic-name";
static uint32_t kTestTopicSubId = 42;

class RemoteAgentServerSessionTest : public ::testing::Test {
protected:
    void PrepareSession() {
        auto [connection, deleter] = MakeFakeConnection();
        connection_ = static_cast<FakeConnection*>(connection.get());

        EXPECT_CALL(handshaker_,
                    PerformHandshake(Address(static_cast<IConnection*>(connection.get())),
                                     Eq(std::nullopt)))
                .WillOnce(Return(kTestVmName));

        std::promise<void> handshake_promise;
        EXPECT_CALL(handshake_callback_, Call(_, kTestVmName)).Times(1).WillOnce([&](auto, auto) {
            handshake_promise.set_value();
        });

        session_.emplace(std::move(connection), handshaker_, handshake_callback_.AsStdFunction(),
                         topic_manager_);

        // Wait for. handshake to ensure we're ready.
        ASSERT_EQ(handshake_promise.get_future().wait_for(std::chrono::seconds(1)),
                  std::future_status::ready);

        ASSERT_FALSE(session_->IsTerminated());

        Mock::VerifyAndClearExpectations(&handshaker_);
        Mock::VerifyAndClearExpectations(&handshake_callback_);
    }

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

    // Ownership of the connection will be handed into the RemoteAgentServerSession, but tests
    // retain a raw pointer so they can interact with it.
    FakeConnection* connection_;
    StrictMock<MockHandshaker> handshaker_;
    StrictMock<MockFunction<RemoteAgentServerSession::HandshakeCallback>> handshake_callback_;
    TopicManager topic_manager_;
    Topic* topic_ = nullptr;
    std::optional<RemoteAgentServerSession> session_;
};

TEST_F(RemoteAgentServerSessionTest, Construction_BecomesInactiveOnHandshakeFailure) {
    auto [connection, deleter] = MakeFakeConnection();
    connection_ = static_cast<FakeConnection*>(connection.get());

    // Will attempt handshake on construction and get failure.
    EXPECT_CALL(handshaker_, PerformHandshake(Address(connection.get()), Eq(std::nullopt)))
            .WillOnce(Return(std::nullopt));

    // Handshake callback never fires since handshake fails.
    EXPECT_CALL(handshake_callback_, Call(_, _)).Times(0);

    // ConnectionThread should call Disconnect before exiting.
    EXPECT_CALL(*connection_, Disconnect());

    RemoteAgentServerSession session(std::move(connection), handshaker_,
                                     handshake_callback_.AsStdFunction(), topic_manager_);

    // Session should reach terminated state.
    WaitForConditionOrTimeout([&]() { return session.IsTerminated(); });

    // Explicitly verify connection_ expectations before Session destruction to
    // ensure they don't depend on destruction.
    Mock::VerifyAndClearExpectations(connection_);
}

TEST_F(RemoteAgentServerSessionTest, Construction_ReportsHandshakeSuccess) {
    auto [connection, deleter] = MakeFakeConnection();

    // Will attempt handshake on construction, which returns success with VM name.
    EXPECT_CALL(handshaker_,
                PerformHandshake(Address(static_cast<IConnection*>(connection.get())),
                                 Eq(std::nullopt)))
            .WillOnce(Return(kTestVmName));

    // Handshake callback fires and passes VM name from successful handshake.
    std::promise<void> handshake_promise;
    EXPECT_CALL(handshake_callback_, Call(_, kTestVmName)).Times(1).WillOnce([&](auto, auto) {
        handshake_promise.set_value();
    });

    RemoteAgentServerSession session(std::move(connection), handshaker_,
                                     handshake_callback_.AsStdFunction(), topic_manager_);

    // Wait for handshake to avoid flakes.
    EXPECT_EQ(handshake_promise.get_future().wait_for(std::chrono::seconds(1)),
              std::future_status::ready);

    ASSERT_FALSE(session.IsTerminated());
}

TEST_F(RemoteAgentServerSessionTest, RequestTerminate_Shuts_Down_Connection) {
    PrepareSession();

    EXPECT_CALL(*connection_, RequestDisconnect()).WillOnce([&]() {
        connection_->EnqueueReceiveFailure();
        connection_->SetSendDataReturnValue(false);
    });

    EXPECT_CALL(*connection_, Disconnect());

    session_->RequestTerminate();

    // Session should reach terminated state.
    WaitForConditionOrTimeout([&]() { return session_->IsTerminated(); });

    // Explicitly verify connection_ expectations before Session destruction to
    // ensure they don't depend on destruction.
    Mock::VerifyAndClearExpectations(connection_);
}

TEST_F(RemoteAgentServerSessionTest, LastMessage_FailsForUnknownTopic) {
    PrepareSession();

    connection_->EnqueueParcelForReceive(123, Payload_LastMessageRequest(kTestTopicName));

    auto payload = connection_->GetNextSentDataIfPayload<Payload_LastMessageResult>();
    ASSERT_TRUE(payload);
    EXPECT_EQ(payload->topic_name, kTestTopicName);
    EXPECT_EQ(payload->result, Payload_LastMessageResult::Result_Failure);
    EXPECT_EQ(payload->buffer.Size(), 0);
}

TEST_F(RemoteAgentServerSessionTest, LastMessage_FailsForRemoteTopic) {
    PrepareSession();
    CreateTestTopic(/* is_remote= */ true);

    connection_->EnqueueParcelForReceive(123, Payload_LastMessageRequest(kTestTopicName));

    auto payload = connection_->GetNextSentDataIfPayload<Payload_LastMessageResult>();
    ASSERT_TRUE(payload);
    EXPECT_EQ(payload->topic_name, kTestTopicName);
    EXPECT_EQ(payload->result, Payload_LastMessageResult::Result_Failure);
    EXPECT_EQ(payload->buffer.Size(), 0);
}

TEST_F(RemoteAgentServerSessionTest, LastMessage_SucceedsWithEmptyBufferForEmptyTopic) {
    PrepareSession();
    CreateTestTopic(/* is_remote= */ false);

    connection_->EnqueueParcelForReceive(123, Payload_LastMessageRequest(kTestTopicName));

    auto payload = connection_->GetNextSentDataIfPayload<Payload_LastMessageResult>();
    ASSERT_TRUE(payload);
    EXPECT_EQ(payload->topic_name, kTestTopicName);
    EXPECT_EQ(payload->result, Payload_LastMessageResult::Result_Ok);
    EXPECT_EQ(payload->buffer.Size(), 0);
}

TEST_F(RemoteAgentServerSessionTest, LastMessage_Success) {
    PrepareSession();
    CreateTestTopic(/* is_remote= */ false);
    WriteTestTopicMessages({42, 31337});

    connection_->EnqueueParcelForReceive(123, Payload_LastMessageRequest(kTestTopicName));

    auto payload = connection_->GetNextSentDataIfPayload<Payload_LastMessageResult>();
    ASSERT_TRUE(payload);
    EXPECT_EQ(payload->topic_name, kTestTopicName);
    EXPECT_EQ(payload->result, Payload_LastMessageResult::Result_Ok);
    EXPECT_EQ(payload->buffer.Size(), kTestMessageSize);
    const auto* message = reinterpret_cast<const TestMessageType*>(payload->buffer.Ptr());
    EXPECT_EQ(*message, 31337);
}

TEST_F(RemoteAgentServerSessionTest, LastMessage_SuccessWithLargeMessages) {
    PrepareSession();
    CreateTestTopic(/* is_remote= */ false, /*message_size=*/kBigMessageSize);

    // Write two messages to the topic.
    BigMessageType big_messages[2];
    memset(&big_messages[0], 1, kBigMessageSize);
    memset(&big_messages[1], 2, kBigMessageSize);
    WriteTestTopicRawData(big_messages, sizeof(big_messages));

    connection_->EnqueueParcelForReceive(123, Payload_LastMessageRequest(kTestTopicName));

    auto payload = connection_->GetNextSentDataIfPayload<Payload_LastMessageResult>();
    ASSERT_TRUE(payload);
    EXPECT_EQ(payload->topic_name, kTestTopicName);
    EXPECT_EQ(payload->result, Payload_LastMessageResult::Result_Ok);
    EXPECT_EQ(payload->buffer.Size(), kBigMessageSize);
    EXPECT_EQ(memcmp(payload->buffer.Ptr(), &big_messages[1], kBigMessageSize), 0);
}

// Single happy-path case for working with RemoteAgentTopicExporter; most server-side
// subscription-related behaviors are covered in the unit tests for that class.
TEST_F(RemoteAgentServerSessionTest, Subscription_FullFlow) {
    PrepareSession();
    CreateTestTopic(/* is_remote= */ false);

    WriteTestTopicMessages({15, 25});

    connection_->EnqueueParcelForReceive(1234,
                                         Payload_SubscribeRequest(kTestTopicName, kTestTopicSubId));

    // Get a subscribe result including the last message content.
    auto sub_result_payload = connection_->GetNextSentDataIfPayload<Payload_SubscribeResult>();
    ASSERT_TRUE(sub_result_payload);
    EXPECT_EQ(sub_result_payload->subscriber_pub_id, kTestTopicSubId);
    EXPECT_EQ(sub_result_payload->result, Payload_SubscribeResult::Result_Ok);
    EXPECT_EQ(sub_result_payload->message_size, kTestMessageSize);
    EXPECT_EQ(sub_result_payload->message_count, kTestMessageCount);
    EXPECT_EQ(sub_result_payload->has_last_message, 1);
    EXPECT_GE(sub_result_payload->last_message_content.Size(), 1 * kTestMessageSize);
    const auto* messages = reinterpret_cast<const TestMessageType*>(
            sub_result_payload->last_message_content.Ptr());
    EXPECT_EQ(messages[0], 25);

    // Brief sleep to ensure Exporter processing thread returns to waiting and doesn't
    // manage to catch the first message write below and send it in its own data parcel.
    std::this_thread::sleep_for(std::chrono::milliseconds(100));

    WriteTestTopicMessages({35, 45});

    auto data_payload_2 = connection_->GetNextSentDataIfPayload<Payload_TopicRawData>();
    ASSERT_TRUE(data_payload_2);
    EXPECT_EQ(data_payload_2->subscriber_pub_id, kTestTopicSubId);
    EXPECT_EQ(data_payload_2->data_size, 2 * kTestMessageSize);
    EXPECT_GE(data_payload_2->buffer.Size(), 2 * kTestMessageSize);
    const auto* messages_2 = reinterpret_cast<const TestMessageType*>(data_payload_2->buffer.Ptr());
    EXPECT_EQ(messages_2[0], 35);
    EXPECT_EQ(messages_2[1], 45);

    DestroyTestTopic();

    auto destroyed_payload = connection_->GetNextSentDataIfPayload<Payload_TopicDestroyed>();
    ASSERT_TRUE(destroyed_payload);
    EXPECT_EQ(destroyed_payload->subscriber_pub_id, kTestTopicSubId);
}

}  // namespace
}  // namespace android::sdv::dt::agent::test
