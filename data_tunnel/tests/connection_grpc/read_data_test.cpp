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

#include "connection_grpc/read_data.h"

#include <gmock/gmock.h>
#include <grpc/byte_buffer.h>
#include <grpc/slice.h>
#include <gtest/gtest.h>

#include <string>
#include <vector>

namespace android::sdv::dt::agent {

namespace {

using ::testing::IsEmpty;
using ::testing::StrEq;

// Helper function to create a ByteBuffer from a string.
grpc::ByteBuffer CreateByteBuffer(const std::string& data) {
    grpc::Slice slice(data);
    grpc::ByteBuffer buffer(&slice, 1);
    return buffer;
}

TEST(ReadDataTest, ExtractValidBuffer) {
    const std::string test_data = "hello world";
    grpc::ByteBuffer buffer = CreateByteBuffer(test_data);

    auto read_data = ReadData::Extract(buffer);

    ASSERT_TRUE(read_data.has_value());
    EXPECT_FALSE(read_data->Exhausted());

    std::vector<char> consumed_data(test_data.size());
    size_t bytes_consumed = read_data->Consume(consumed_data.data(), consumed_data.size());

    EXPECT_EQ(bytes_consumed, test_data.size());
    EXPECT_THAT(std::string(consumed_data.begin(), consumed_data.end()), StrEq(test_data));
    EXPECT_TRUE(read_data->Exhausted());
}

TEST(ReadDataTest, ExtractEmptyBuffer) {
    grpc::ByteBuffer buffer;  // Empty buffer
    auto read_data = ReadData::Extract(buffer);
    EXPECT_FALSE(read_data.has_value());
}

TEST(ReadDataTest, ExtractBufferWithEmptySlice) {
    grpc::ByteBuffer buffer = CreateByteBuffer("");
    auto read_data = ReadData::Extract(buffer);
    EXPECT_FALSE(read_data.has_value());
}

TEST(ReadDataTest, ConsumePartially) {
    const std::string test_data = "this is a longer test string";
    grpc::ByteBuffer buffer = CreateByteBuffer(test_data);
    auto read_data = ReadData::Extract(buffer);
    ASSERT_TRUE(read_data.has_value());

    const size_t consume_size = 10;
    std::vector<char> consumed_data(consume_size);
    size_t bytes_consumed = read_data->Consume(consumed_data.data(), consumed_data.size());

    EXPECT_EQ(bytes_consumed, consume_size);
    EXPECT_THAT(std::string(consumed_data.begin(), consumed_data.end()), StrEq("this is a "));
    EXPECT_FALSE(read_data->Exhausted());
}

TEST(ReadDataTest, ConsumeFully) {
    const std::string test_data = "consume all";
    grpc::ByteBuffer buffer = CreateByteBuffer(test_data);
    auto read_data = ReadData::Extract(buffer);
    ASSERT_TRUE(read_data.has_value());

    std::vector<char> consumed_data(test_data.size());
    size_t bytes_consumed = read_data->Consume(consumed_data.data(), consumed_data.size());

    EXPECT_EQ(bytes_consumed, test_data.size());
    EXPECT_THAT(std::string(consumed_data.begin(), consumed_data.end()), StrEq(test_data));
    EXPECT_TRUE(read_data->Exhausted());
}

TEST(ReadDataTest, ConsumeMoreThanAvailable) {
    const std::string test_data = "short";
    grpc::ByteBuffer buffer = CreateByteBuffer(test_data);
    auto read_data = ReadData::Extract(buffer);
    ASSERT_TRUE(read_data.has_value());

    std::vector<char> consumed_data(test_data.size() + 5);
    size_t bytes_consumed = read_data->Consume(consumed_data.data(), consumed_data.size());

    EXPECT_EQ(bytes_consumed, test_data.size());
    EXPECT_THAT(std::string(consumed_data.begin(), consumed_data.begin() + bytes_consumed),
                StrEq(test_data));
    EXPECT_TRUE(read_data->Exhausted());
}

TEST(ReadDataTest, ConsumeInMultipleSteps) {
    const std::string test_data = "step by step consumption";
    grpc::ByteBuffer buffer = CreateByteBuffer(test_data);
    auto read_data = ReadData::Extract(buffer);
    ASSERT_TRUE(read_data.has_value());

    std::vector<char> part1(4), part2(3), part3(test_data.size() - 7);
    EXPECT_EQ(read_data->Consume(part1.data(), part1.size()), 4);
    EXPECT_THAT(std::string(part1.begin(), part1.end()), StrEq("step"));
    EXPECT_FALSE(read_data->Exhausted());

    EXPECT_EQ(read_data->Consume(part2.data(), part2.size()), 3);
    EXPECT_THAT(std::string(part2.begin(), part2.end()), StrEq(" by"));
    EXPECT_FALSE(read_data->Exhausted());

    EXPECT_EQ(read_data->Consume(part3.data(), part3.size()), 17);
    EXPECT_THAT(std::string(part3.begin(), part3.end()), StrEq(" step consumption"));
    EXPECT_TRUE(read_data->Exhausted());
}

}  // namespace

}  // namespace android::sdv::dt::agent
