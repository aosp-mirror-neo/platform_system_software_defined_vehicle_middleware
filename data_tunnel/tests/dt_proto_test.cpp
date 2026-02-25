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

#include <android-base/logging.h>
#include <gtest/gtest.h>
#include <src/proto/dt_buffer.h>
#include <src/proto/dt_proto.h>

namespace android::sdv::dt::agent::test {

constexpr size_t kSmallBufferSize = 1024;
constexpr size_t kLargeBufferSize = 1024 * 1024;

TEST(DataTunnelProto, BufferAllocation) {
    RawBuffer buffer(kSmallBufferSize);
    ASSERT_EQ(buffer.Size(), kSmallBufferSize);
    buffer.Resize(kLargeBufferSize, false);
    ASSERT_EQ(buffer.Size(), kLargeBufferSize);
}

TEST(DataTunnelProto, BufferPositionTracker) {
    RawBuffer buffer(kSmallBufferSize);
    BufferPosTracker pos_tracker(buffer);
    size_t mid_pos = buffer.Size() / 2;
    ASSERT_EQ(pos_tracker.Available(), buffer.Size());
    pos_tracker.SetPos(mid_pos);
    ASSERT_EQ(pos_tracker.GetPos(), mid_pos);
    ASSERT_EQ(pos_tracker.Available(), buffer.Size() - mid_pos);
    ASSERT_EQ(pos_tracker.CurPtr() - pos_tracker.Ptr(), static_cast<std::ptrdiff_t>(mid_pos));
}

TEST(DataTunnelProto, BufferReaderWriterSerialization) {
    constexpr int8_t i8c = -64;
    constexpr uint8_t u8c = 192;
    constexpr int16_t i16c = -(1 << 14);
    constexpr uint16_t u16c = 1 << 15;
    constexpr int32_t i32c = -(1 << 28);
    constexpr uint32_t u32c = 1 << 30;
    const std::string strc("Test string of text");
    RawBuffer buffer(kLargeBufferSize);
    BufferWriter writer(buffer);
    writer.Write(i8c);
    writer.Write(u8c);
    writer.Write(i16c);
    writer.Write(u16c);
    writer.Write(i32c);
    writer.Write(u32c);
    writer.Write(strc);
    int8_t i8v;
    uint8_t u8v;
    int16_t i16v;
    uint16_t u16v;
    int32_t i32v;
    uint32_t u32v;
    std::string strv;
    BufferReader reader(writer);
    ASSERT_TRUE(reader.Read(i8v));
    ASSERT_EQ(i8v, i8c);
    ASSERT_TRUE(reader.Read(u8v));
    ASSERT_EQ(u8v, u8c);
    ASSERT_TRUE(reader.Read(i16v));
    ASSERT_EQ(i16v, i16c);
    ASSERT_TRUE(reader.Read(u16v));
    ASSERT_EQ(u16v, u16c);
    ASSERT_TRUE(reader.Read(i32v));
    ASSERT_EQ(i32v, i32c);
    ASSERT_TRUE(reader.Read(u32v));
    ASSERT_EQ(u32v, u32c);
    ASSERT_TRUE(reader.Read(strv));
    ASSERT_EQ(strv, strc);
}

TEST(DataTunnelProto, BufferReaderWriterOperations) {
    RawBuffer buffer(kSmallBufferSize);
    const size_t kInflateBy = 128;
    const size_t kRefMessageSize = 10;
    const uint8_t kRefMessage[kRefMessageSize] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9};
    uint8_t read_message_buffer[kRefMessageSize];
    ASSERT_EQ(buffer.Size(), kSmallBufferSize);

    // Test Resize
    BufferWriter writer(buffer);
    ASSERT_EQ(writer.Buffer().Size(), kSmallBufferSize);
    ASSERT_EQ(writer.Buffer().Size(), writer.Available());
    writer.Write(kRefMessage, kRefMessageSize);
    ASSERT_EQ(writer.Buffer().Size() - kRefMessageSize, writer.Available());
    writer.Inflate(kInflateBy);
    ASSERT_EQ(writer.Buffer().Size(), kSmallBufferSize + kInflateBy);
    ASSERT_EQ(writer.Buffer().Size() - kRefMessageSize, writer.Available());
    BufferReader reader(writer);
    reader.Read(read_message_buffer, kRefMessageSize);
    ASSERT_TRUE(memcmp(kRefMessage, read_message_buffer, kRefMessageSize) == 0);

    // TODO: Test Cut And Move
}

template <typename PayloadType>
std::unique_ptr<PayloadType> SerializeAndDeserialize(const PayloadType& payload) {
    RawBuffer buffer(kLargeBufferSize);
    BufferWriter writer(buffer);
    proto::Parcel parcel(64);
    if (!parcel.Serialize(writer, payload)) {
        return nullptr;
    }
    BufferReader reader(writer);
    auto result = proto::Parcel::Deserialize(reader, NULL);
    if (result.first != proto::Parcel::ParseResult::Success || !result.second) {
        return nullptr;
    }
    return std::unique_ptr<PayloadType>(static_cast<PayloadType*>(result.second.release()));
}

TEST(DataTunnelProto, Serialization_Ping) {
    auto payload = SerializeAndDeserialize(proto::Payload_Ping("vmname2"));
    ASSERT_TRUE(!!payload);
    EXPECT_EQ(payload->vm_name, "vmname2");
}

TEST(DataTunnelProto, Serialization_Pong) {
    auto payload = SerializeAndDeserialize(proto::Payload_Pong(32));
    ASSERT_TRUE(!!payload);
    EXPECT_EQ(payload->ping_id, 32U);
}

TEST(DataTunnelProto, Serialization_SubscribeRequest) {
    auto payload = SerializeAndDeserialize(proto::Payload_SubscribeRequest("Topic A", 1234));
    ASSERT_TRUE(!!payload);
    EXPECT_EQ(payload->topic_name, "Topic A");
    EXPECT_EQ(payload->subscriber_pub_id, 1234);
}

TEST(DataTunnelProto, Serialization_SubscribeResult) {
    proto::Payload_SubscribeResult original;
    original.result = 1;
    original.message_size = sizeof(uint32_t);
    original.message_count = 15;
    original.subscriber_pub_id = 17;
    original.has_last_message = 0;
    original.last_message_content.Resize(0, false);

    // First check with no last message data.
    auto no_data = SerializeAndDeserialize(original);
    ASSERT_TRUE(!!no_data);
    EXPECT_EQ(no_data->result, 1U);
    EXPECT_EQ(no_data->message_size, sizeof(uint32_t));
    EXPECT_EQ(no_data->message_count, 15U);
    EXPECT_EQ(no_data->subscriber_pub_id, 17);
    EXPECT_EQ(no_data->has_last_message, 0U);
    EXPECT_EQ(no_data->last_message_content.Size(), 0U);

    // Add last message data and try again.
    original.has_last_message = 1;
    original.last_message_content.Resize(sizeof(uint32_t), true);
    *(reinterpret_cast<uint32_t*>(original.last_message_content.Ptr())) = 0xDEADBEEF;
    auto with_data = SerializeAndDeserialize(original);
    ASSERT_TRUE(!!with_data);
    EXPECT_EQ(with_data->result, 1U);
    EXPECT_EQ(with_data->message_size, sizeof(uint32_t));
    EXPECT_EQ(with_data->message_count, 15U);
    EXPECT_EQ(with_data->subscriber_pub_id, 17);
    EXPECT_EQ(with_data->has_last_message, 1U);
    EXPECT_EQ(with_data->last_message_content.Size(), sizeof(uint32_t));
    EXPECT_EQ(*(reinterpret_cast<uint32_t*>(with_data->last_message_content.Ptr())), 0xDEADBEEF);
}

TEST(DataTunnelProto, Serialization_TopicRawData) {
    proto::Payload_TopicRawData original_payload(sizeof(uint32_t));
    original_payload.subscriber_pub_id = 11;
    *(reinterpret_cast<uint32_t*>(original_payload.buffer.Ptr())) = 0xDEADBEEF;

    // First check with no last message data.
    auto deserialized_payload = SerializeAndDeserialize(original_payload);
    ASSERT_TRUE(!!deserialized_payload);
    EXPECT_EQ(deserialized_payload->subscriber_pub_id, 11);
    EXPECT_EQ(deserialized_payload->data_size, sizeof(uint32_t));
    EXPECT_EQ(*(reinterpret_cast<uint32_t*>(deserialized_payload->buffer.Ptr())), 0xDEADBEEF);
}

TEST(DataTunnelProto, Serialization_TopicDestroyed) {
    auto payload = SerializeAndDeserialize(proto::Payload_TopicDestroyed(1234));
    ASSERT_TRUE(!!payload);
    EXPECT_EQ(payload->subscriber_pub_id, 1234);
}

TEST(DataTunnelProto, Serialization_LastMessageRequest) {
    auto payload = SerializeAndDeserialize(proto::Payload_LastMessageRequest("Topic A"));
    ASSERT_TRUE(!!payload);
    EXPECT_EQ(payload->topic_name, "Topic A");
}

TEST(DataTunnelProto, Serialization_LastMessageResult) {
    proto::Payload_LastMessageResult original_payload;
    original_payload.topic_name = "pub-name";

    // First check failure case with no data.
    original_payload.result = proto::Payload_LastMessageResult::Result_Failure;
    auto payload_no_data = SerializeAndDeserialize(original_payload);
    ASSERT_TRUE(!!payload_no_data);
    EXPECT_EQ(payload_no_data->topic_name, "pub-name");
    EXPECT_EQ(payload_no_data->result, proto::Payload_LastMessageResult::Result_Failure);
    EXPECT_EQ(payload_no_data->buffer.Size(), 0U);

    // Now check success case with data present
    original_payload.result = proto::Payload_LastMessageResult::Result_Ok;
    original_payload.buffer.Resize(sizeof(uint32_t), false);
    *(reinterpret_cast<uint32_t*>(original_payload.buffer.Ptr())) = 0xDEADBEEF;
    auto payload_with_data = SerializeAndDeserialize(original_payload);
    ASSERT_TRUE(!!payload_with_data);
    EXPECT_EQ(payload_with_data->topic_name, "pub-name");
    EXPECT_EQ(payload_with_data->result, proto::Payload_LastMessageResult::Result_Ok);
    ASSERT_EQ(payload_with_data->buffer.Size(), sizeof(uint32_t));
    EXPECT_EQ(*(reinterpret_cast<uint32_t*>(payload_with_data->buffer.Ptr())), 0xDEADBEEF);
}

TEST(DataTunnelProto, LastMessageResult_DeserializationFailsWithoutResizeUntilAllDataAvailable) {
    RawBuffer buffer(kLargeBufferSize);
    BufferWriter writer(buffer);

    // Write a serialized Payload_LastMessageResult; duplicating code from Serialize()
    // rather than calling that function because it wants to write the buffer.
    ASSERT_TRUE(writer.Write("Topic A"));                                    // topic_name_
    ASSERT_TRUE(writer.Write(proto::Payload_LastMessageResult::Result_Ok));  // result_
    ASSERT_TRUE(writer.Write(static_cast<uint32_t>(4096)));                  // buffer.Size()

    // Attempt to deserialize payload; will fail due to insufficient data and will not
    // resize the buffer in the payload object.
    proto::Payload_LastMessageResult payload;
    BufferReader reader(writer);
    EXPECT_FALSE(payload.Deserialize(reader));
    EXPECT_EQ(payload.buffer.Size(), static_cast<size_t>(0));

    // "write" the amount of data specified in the payload header and create new reader that
    // sees added data.
    writer.MovePos(4096);
    BufferReader reader_2(writer);

    // Now deserialize succeeds and buffer resized.
    EXPECT_TRUE(payload.Deserialize(reader_2));
    EXPECT_EQ(payload.buffer.Size(), static_cast<size_t>(4096));
}

TEST(DataTunnelProto, TopicRawData_DeserializationFailsWithoutResizeUntilAllDataAvailable) {
    RawBuffer buffer(kLargeBufferSize);
    BufferWriter writer(buffer);

    // Write a serialized Payload_TopicRawData; duplicating code from Serialize()
    // rather than calling that function because it wants to write the buffer.
    ASSERT_TRUE(writer.Write(1234));                         // subscriber_pub_id
    ASSERT_TRUE(writer.Write(static_cast<uint32_t>(4096)));  // data_size_

    // Attempt to deserialize payload; will fail due to insufficient data and will not
    // resize the buffer in the payload object.
    proto::Payload_TopicRawData payload(1024);
    BufferReader reader(writer);
    EXPECT_FALSE(payload.Deserialize(reader));
    EXPECT_EQ(payload.buffer.Size(), static_cast<size_t>(1024));

    // "write" the amount of data specified in the payload header and create new reader that
    // sees added data.
    writer.MovePos(4096);
    BufferReader reader_2(writer);

    // Now deserialize succeeds and resizes the payload's buffer
    EXPECT_TRUE(payload.Deserialize(reader_2));
    EXPECT_EQ(payload.buffer.Size(), static_cast<size_t>(4096));
}

TEST(DataTunnelProto, SubscribeResult_DeserializationFailsWithoutResizeUntilAllDataAvailable) {
    RawBuffer buffer(kLargeBufferSize);
    BufferWriter writer(buffer);

    // Write a serialized Payload_SubscribeResult; duplicating code from Serialize()
    // rather than calling that function because it wants to write the buffer.
    ASSERT_TRUE(writer.Write(static_cast<uint16_t>(0)));   // result_
    ASSERT_TRUE(writer.Write(static_cast<uint32_t>(16)));  // message_size_
    ASSERT_TRUE(writer.Write(static_cast<uint32_t>(16)));  // message_count_
    ASSERT_TRUE(writer.Write("Topic A"));                  // topic_name_
    ASSERT_TRUE(writer.Write(static_cast<uint8_t>(1)));    // has_last_message_

    // Attempt to deserialize payload; will fail due to insufficient data and will not
    // resize the buffer in the payload object.
    proto::Payload_SubscribeResult payload;
    BufferReader reader(writer);
    EXPECT_FALSE(payload.Deserialize(reader));

    EXPECT_EQ(payload.last_message_content.Size(), static_cast<size_t>(0));

    // "write" the amount of data specified in the payload header and create new reader that
    // sees added data.
    writer.MovePos(16);
    BufferReader reader_2(writer);

    // Now deserialize succeeds and resizes the payload's buffer
    ASSERT_TRUE(payload.Deserialize(reader_2));
    EXPECT_EQ(payload.last_message_content.Size(), static_cast<size_t>(16));
}

}  // namespace android::sdv::dt::agent::test

int main(int argc, char** argv) {
    android::base::SetDefaultTag("dt_core_test");
    android::base::SetMinimumLogSeverity(android::base::VERBOSE);
    ::testing::InitGoogleTest(&argc, argv);
    return RUN_ALL_TESTS();
}
