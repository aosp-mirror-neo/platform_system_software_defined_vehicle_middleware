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

#include "topic.h"

#include <android-base/logging.h>
#include <gtest/gtest.h>

#include <memory>

namespace android::sdv::dt::agent::test {

namespace {

static const size_t kTestMessageSize = 8;
static const size_t kTestMessageCount = 32;

Topic::Metadata TestMetadata() {
    return Topic::Metadata{"TestTopic", kTestMessageSize, kTestMessageCount};
}

struct TestTopicWithMQ {
    std::unique_ptr<Topic> topic;
    std::unique_ptr<AidlMQ> mq;
};

TestTopicWithMQ MakeTestTopicWithWriterMQ() {
    // Create metadata
    auto metadata = TestMetadata();

    auto mq = std::make_unique<AidlMQ>(metadata.message_size * metadata.message_count);
    CHECK(mq->isValid());

    // Get a descriptor and seal the MQ for writing via future handles / mappings.
    auto mq_desc = mq->dupeDesc();
    CHECK(mq_desc.handle.fds.size() == 1);
    CHECK(ashmem_set_prot_region(mq_desc.handle.fds[0].get(), PROT_READ) == 0);

    auto topic_mq = std::make_unique<AidlMQ>(mq_desc, /* resetPointers= */ false);
    CHECK(topic_mq->isValid());

    auto topic = Topic::Create(metadata, /* id= */ 101, /* client_id= */ 202,
                               /* is_remote= */ false, std::move(topic_mq));
    CHECK(topic);
    return TestTopicWithMQ{std::move(topic), std::move(mq)};
}

class DTAgentTopicTest : public testing::Test {
protected:
    DTAgentTopicTest() {}
};

TEST_F(DTAgentTopicTest, CreateReaderWithEmptyQueue) {
    auto [topic, writer_mq] = MakeTestTopicWithWriterMQ();

    auto reader = topic->CreateReader();
    ASSERT_TRUE(reader);
    ASSERT_EQ(reader->AvailableToRead(), 0);
}

TEST_F(DTAgentTopicTest, CreateReaderWithPopulatedQueue) {
    auto [topic, writer_mq] = MakeTestTopicWithWriterMQ();

    int8_t message_0[kTestMessageSize] = {0, 1, 2, 3, 4, 5, 6, 7};
    int8_t message_1[kTestMessageSize] = {10, 11, 12, 13, 14, 15, 16, 17};

    ASSERT_TRUE(writer_mq->write(message_0, sizeof(message_0)));
    ASSERT_TRUE(writer_mq->write(message_1, sizeof(message_1)));

    auto reader = topic->CreateReader();
    ASSERT_TRUE(reader);
    ASSERT_EQ(reader->AvailableToRead(), kTestMessageSize);

    uint8_t read_buffer[kTestMessageSize];
    ASSERT_TRUE(reader->Read(read_buffer, kTestMessageSize));
    ASSERT_EQ(read_buffer[0], message_1[0]);
}

TEST_F(DTAgentTopicTest, CreateReaderWithMisalignedQueue) {
    auto [topic, writer_mq] = MakeTestTopicWithWriterMQ();

    int8_t message_0[kTestMessageSize] = {0, 1, 2, 3, 4, 5, 6, 7};

    ASSERT_TRUE(writer_mq->write(message_0, kTestMessageSize - 1));

    auto reader = topic->CreateReader();
    ASSERT_FALSE(reader);
}

TEST_F(DTAgentTopicTest, CreateReaderWithOverflowedQueue) {
    auto [topic, writer_mq] = MakeTestTopicWithWriterMQ();

    int8_t message_0[kTestMessageSize] = {0, 1, 2, 3, 4, 5, 6, 7};

    // Write enough to completely fill the queue.
    for (auto i = 0; i < kTestMessageCount; i++) {
        ASSERT_TRUE(writer_mq->write(message_0, kTestMessageSize));
    }

    // Write another message with different content, which puts the
    // handle inside the topic in an overflowed state.
    int8_t message_1[kTestMessageSize] = {10, 11, 12, 13, 14, 15, 16, 17};
    ASSERT_TRUE(writer_mq->write(message_1, sizeof(message_0)));

    auto reader = topic->CreateReader();
    ASSERT_TRUE(reader);
    ASSERT_EQ(reader->AvailableToRead(), kTestMessageSize);

    uint8_t read_buffer[kTestMessageSize];
    ASSERT_TRUE(reader->Read(read_buffer, kTestMessageSize));
    ASSERT_EQ(read_buffer[0], message_1[0]);
}

TEST_F(DTAgentTopicTest, GetLastMessageWithEmptyQueue) {
    auto [topic, writer_mq] = MakeTestTopicWithWriterMQ();

    auto last_message = topic->GetLastMessage();
    ASSERT_TRUE(last_message);
    ASSERT_TRUE(last_message->empty());
}

TEST_F(DTAgentTopicTest, GetLastMessageWithPopulatedQueue) {
    auto [topic, writer_mq] = MakeTestTopicWithWriterMQ();

    int8_t message_0[kTestMessageSize] = {0, 1, 2, 3, 4, 5, 6, 7};
    int8_t message_1[kTestMessageSize] = {10, 11, 12, 13, 14, 15, 16, 17};

    ASSERT_TRUE(writer_mq->write(message_0, sizeof(message_0)));
    ASSERT_TRUE(writer_mq->write(message_1, sizeof(message_1)));

    auto last_message = topic->GetLastMessage();
    ASSERT_TRUE(last_message);
    ASSERT_EQ(last_message->at(0), message_1[0]);
}

TEST_F(DTAgentTopicTest, GetLastMessageWithMisalignedQueue) {
    auto [topic, writer_mq] = MakeTestTopicWithWriterMQ();

    int8_t message_0[kTestMessageSize] = {0, 1, 2, 3, 4, 5, 6, 7};

    ASSERT_TRUE(writer_mq->write(message_0, kTestMessageSize - 1));

    ASSERT_FALSE(topic->GetLastMessage());
}

TEST_F(DTAgentTopicTest, GetLastMessageWithOverflowedQueue) {
    auto [topic, writer_mq] = MakeTestTopicWithWriterMQ();

    int8_t message_0[kTestMessageSize] = {0, 1, 2, 3, 4, 5, 6, 7};

    // Write enough to completely fill the queue.
    for (auto i = 0; i < kTestMessageCount; i++) {
        ASSERT_TRUE(writer_mq->write(message_0, kTestMessageSize));
    }

    // Write another message with different content, which puts the
    // handle inside the topic in an overflowed state.
    int8_t message_1[kTestMessageSize] = {10, 11, 12, 13, 14, 15, 16, 17};
    ASSERT_TRUE(writer_mq->write(message_1, sizeof(message_0)));

    auto last_message = topic->GetLastMessage();
    ASSERT_TRUE(last_message);
    ASSERT_EQ(last_message->at(0), message_1[0]);
}

}  // namespace

}  // namespace android::sdv::dt::agent::test
