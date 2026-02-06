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

static bool CheckPayloadSerialization(RawBuffer& buffer, const proto::Payload& payload) {
    BufferWriter writer(buffer);
    proto::Parcel parcel(64);
    if (!parcel.Serialize(writer, payload)) return false;
    BufferReader reader(writer);
    auto result = proto::Parcel::Deserialize(reader, NULL);
    return result.first == proto::Parcel::ParseResult::Success && result.second &&
            *result.second.get() == payload;
}

TEST(DataTunnelProto, ParcelSerialization) {
    RawBuffer buffer(kLargeBufferSize);
    ASSERT_TRUE(CheckPayloadSerialization(buffer, proto::Payload_Ping("8")));
    ASSERT_TRUE(CheckPayloadSerialization(buffer, proto::Payload_Pong(32)));
    ASSERT_TRUE(CheckPayloadSerialization(buffer, proto::Payload_SubscribeRequest("Topic A")));
    ASSERT_TRUE(CheckPayloadSerialization(buffer, proto::Payload_SubscribeResult()));
}

TEST(DataTunnelProto, LastMessageResult_DeserializationFailsWithoutResizeUntilAllDataAvailable) {
    RawBuffer buffer(kLargeBufferSize);
    BufferWriter writer(buffer);

    // Write a serialized Payload_LastMessageResult; duplicating code from Serialize()
    // rather than calling that function because it wants to write the buffer.
    ASSERT_TRUE(writer.Write("Topic A"));                                    // topic_name_
    ASSERT_TRUE(writer.Write(proto::Payload_LastMessageResult::Result_Ok));  // result_
    ASSERT_TRUE(writer.Write(static_cast<uint32_t>(4096)));                  // buffer_.Size()

    // Attempt to deserialize payload; will fail due to insufficient data and will not
    // resize the buffer_ in the payload object.
    proto::Payload_LastMessageResult payload;
    BufferReader reader(writer);
    EXPECT_FALSE(payload.Deserialize(reader));
    EXPECT_EQ(payload.buffer_.Size(), static_cast<size_t>(0));

    // "write" the amount of data specified in the payload header and create new reader that
    // sees added data.
    writer.MovePos(4096);
    BufferReader reader_2(writer);

    // Now deserialize succeeds and buffer resized.
    EXPECT_TRUE(payload.Deserialize(reader_2));
    EXPECT_EQ(payload.buffer_.Size(), static_cast<size_t>(4096));
}

TEST(DataTunnelProto, TopicRawData_DeserializationFailsWithoutResizeUntilAllDataAvailable) {
    RawBuffer buffer(kLargeBufferSize);
    BufferWriter writer(buffer);

    // Write a serialized Payload_TopicRawData; duplicating code from Serialize()
    // rather than calling that function because it wants to write the buffer.
    ASSERT_TRUE(writer.Write("Topic A"));                    // topic_name_
    ASSERT_TRUE(writer.Write(static_cast<uint32_t>(4096)));  // data_size_

    // Attempt to deserialize payload; will fail due to insufficient data and will not
    // resize the buffer_ in the payload object.
    proto::Payload_TopicRawData payload(1024);
    BufferReader reader(writer);
    EXPECT_FALSE(payload.Deserialize(reader));
    EXPECT_EQ(payload.buffer_.Size(), static_cast<size_t>(1024));

    // "write" the amount of data specified in the payload header and create new reader that
    // sees added data.
    writer.MovePos(4096);
    BufferReader reader_2(writer);

    // Now deserialize succeeds and resizes the payload's buffer
    EXPECT_TRUE(payload.Deserialize(reader_2));
    EXPECT_EQ(payload.buffer_.Size(), static_cast<size_t>(4096));
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
    // resize the buffer_ in the payload object.
    proto::Payload_SubscribeResult payload;
    BufferReader reader(writer);
    EXPECT_FALSE(payload.Deserialize(reader));

    EXPECT_EQ(payload.last_message_content_.Size(), static_cast<size_t>(0));

    // "write" the amount of data specified in the payload header and create new reader that
    // sees added data.
    writer.MovePos(16);
    BufferReader reader_2(writer);

    // Now deserialize succeeds and resizes the payload's buffer
    ASSERT_TRUE(payload.Deserialize(reader_2));
    EXPECT_EQ(payload.last_message_content_.Size(), static_cast<size_t>(16));
}
// TODO: Add more DataTunnel protocol tests

}  // namespace android::sdv::dt::agent::test

int main(int argc, char** argv) {
    android::base::SetDefaultTag("dt_core_test");
    android::base::SetMinimumLogSeverity(android::base::VERBOSE);
    ::testing::InitGoogleTest(&argc, argv);
    return RUN_ALL_TESTS();
}
