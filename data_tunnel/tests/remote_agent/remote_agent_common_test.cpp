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

#include "remote_agent/remote_agent_common.h"

#include "connection.h"
#include "remote_agent/remote_agent_test_utils.h"
#include "topic_manager.h"

#include <aidl/google/sdv/identity/ServiceFqin.h>
#include <gtest/gtest.h>

using ::testing::_;
using ::testing::Return;

namespace android::sdv::dt::agent::test {

static const char* kTestTopicName = "test-topic-name";
class RemoteAgentCommonTest : public testing::Test {
protected:
    RemoteAgentCommonTest() : buffer_(1024) {}

    FakeConnection connection_;
    RawBuffer buffer_;
};

TEST_F(RemoteAgentCommonTest, MakeFqinString) {
    auto fqin = ::aidl::google::sdv::identity::ServiceFqin{
            /*sdvVmName*/ "testvm1",
            /*sdvPackageName*/ "com.legoog.sdv",
            /*serviceBundleName*/ "TestServiceBundle",
            /*serviceInstanceName*/ "default",
    };
    ASSERT_EQ(MakeFqinString(fqin), "testvm1:com.legoog.sdv.TestServiceBundle/default");
}

TEST_F(RemoteAgentCommonTest, SendPayload_Success) {
    proto::Payload_LastMessageRequest sent_payload;
    sent_payload.topic_name_ = kTestTopicName;
    ASSERT_TRUE(SendPayload(connection_, sent_payload, 1234, buffer_));

    auto payload = connection_.GetNextSentDataIfPayload<proto::Payload_LastMessageRequest>();
    ASSERT_TRUE(payload);
    ASSERT_EQ(payload->topic_name_, kTestTopicName);

    ASSERT_EQ(connection_.RemainingSentDataCount(), 0);
}

TEST_F(RemoteAgentCommonTest, SendPayload_FailsWithTooSmallBuffer) {
    proto::Payload_LastMessageRequest sent_payload;
    sent_payload.topic_name_ = kTestTopicName;
    RawBuffer small_buffer(1);
    ASSERT_FALSE(SendPayload(connection_, sent_payload, 1234, small_buffer));
    ASSERT_FALSE(connection_.GetNextSentData(/* timeout_seconds= */ 2));
}

TEST_F(RemoteAgentCommonTest, SendPayload_FailsWhenSendDataFails) {
    connection_.SetSendDataReturnValue(false);

    proto::Payload_LastMessageRequest sent_payload;
    sent_payload.topic_name_ = kTestTopicName;
    ASSERT_FALSE(SendPayload(connection_, sent_payload, 4321, buffer_));
    ASSERT_TRUE(connection_.GetNextSentData());  // Send data was called.
}

TEST_F(RemoteAgentCommonTest, ReceivePayload_Failure) {
    connection_.EnqueueReceiveFailure();

    BufferWriter receive_writer(buffer_);
    auto [result, payload] = ReceivePayload(connection_, nullptr, receive_writer);
    ASSERT_EQ(result, proto::Parcel::ParseResult::Failure);
}

TEST_F(RemoteAgentCommonTest, ReceivePayload_ExactlyOne) {
    proto::Payload_LastMessageRequest sent_payload;
    sent_payload.topic_name_ = kTestTopicName;
    connection_.EnqueueParcelForReceive(123, sent_payload);

    uint32_t parcel_id;
    BufferWriter receive_writer(buffer_);
    auto [result, payload] = ReceivePayload(connection_, &parcel_id, receive_writer);
    ASSERT_EQ(result, proto::Parcel::ParseResult::Success);
    ASSERT_EQ(parcel_id, 123);
    ASSERT_NE(payload.get(), nullptr);
    ASSERT_EQ(payload->type_, proto::Payload::Type::LastMessageRequest);
    const auto* lmreq_payload =
            static_cast<const proto::Payload_LastMessageRequest*>(payload.get());
    ASSERT_EQ(lmreq_payload->topic_name_, kTestTopicName);
    ASSERT_EQ(connection_.ReceiveCallCount(), 1);
}

TEST_F(RemoteAgentCommonTest, ReceivePayload_OnePayloadInTwoParts) {
    proto::Payload_LastMessageRequest sent_payload;
    sent_payload.topic_name_ = kTestTopicName;
    proto::Parcel parcel(456);
    BufferWriter serialize_writer(buffer_);
    parcel.Serialize(serialize_writer, sent_payload);

    BufferReader enqueue_reader(serialize_writer);
    auto first_read_length = enqueue_reader.AvailableToRead() / 2;
    connection_.EnqueueDataForReceive(enqueue_reader, first_read_length);
    connection_.EnqueueDataForReceive(enqueue_reader);

    // Reset writer
    uint32_t parcel_id;
    BufferWriter receive_writer(buffer_);
    auto [result, payload] = ReceivePayload(connection_, &parcel_id, receive_writer);
    ASSERT_EQ(result, proto::Parcel::ParseResult::Success);
    std::cout << "AAAAA Received " << parcel_id << std::endl;
    ASSERT_EQ(parcel_id, 456);
    ASSERT_NE(payload.get(), nullptr);
    ASSERT_EQ(payload->type_, proto::Payload::Type::LastMessageRequest);
    const auto* lmreq_payload =
            static_cast<const proto::Payload_LastMessageRequest*>(payload.get());
    ASSERT_EQ(lmreq_payload->topic_name_, kTestTopicName);
    ASSERT_EQ(connection_.ReceiveCallCount(), 2);
}

TEST_F(RemoteAgentCommonTest, ReceivePayload_TwoPayloadsInOnePart) {
    // Writer to
    BufferWriter serialize_writer(buffer_);

    // First Parcel
    {
        proto::Payload_LastMessageRequest sent_payload;
        sent_payload.topic_name_ = kTestTopicName;
        proto::Parcel parcel(789);
        parcel.Serialize(serialize_writer, sent_payload);
    }

    // Second Parcel
    {
        proto::Payload_SubscribeRequest sent_payload;
        sent_payload.topic_name_ = kTestTopicName;
        proto::Parcel parcel(101112);
        parcel.Serialize(serialize_writer, sent_payload);
    }

    BufferReader enqueue_reader(serialize_writer);
    connection_.EnqueueDataForReceive(enqueue_reader);

    BufferWriter receive_writer(buffer_);

    {
        uint32_t parcel_id;
        auto [result, payload] = ReceivePayload(connection_, &parcel_id, receive_writer);
        ASSERT_EQ(result, proto::Parcel::ParseResult::Success);
        ASSERT_EQ(parcel_id, 789);
        ASSERT_NE(payload.get(), nullptr);
        ASSERT_EQ(payload->type_, proto::Payload::Type::LastMessageRequest);
        const auto* lmreq_payload =
                static_cast<const proto::Payload_LastMessageRequest*>(payload.get());
        ASSERT_EQ(lmreq_payload->topic_name_, kTestTopicName);
        ASSERT_EQ(connection_.ReceiveCallCount(), 1);
    }

    {
        uint32_t parcel_id;
        auto [result, payload] = ReceivePayload(connection_, &parcel_id, receive_writer);
        ASSERT_EQ(result, proto::Parcel::ParseResult::Success);
        ASSERT_EQ(parcel_id, 101112);
        ASSERT_NE(payload.get(), nullptr);
        ASSERT_EQ(payload->type_, proto::Payload::Type::SubscribeRequest);
        const auto* subreq_payload =
                static_cast<const proto::Payload_LastMessageRequest*>(payload.get());
        ASSERT_EQ(subreq_payload->topic_name_, kTestTopicName);
        // No additional ReceiveData call
        ASSERT_EQ(connection_.ReceiveCallCount(), 1);
    }
}

}  // namespace android::sdv::dt::agent::test