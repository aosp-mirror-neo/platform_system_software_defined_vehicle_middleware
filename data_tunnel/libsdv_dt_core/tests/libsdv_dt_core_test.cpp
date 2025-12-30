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

#include "libsdv_dt_core.h"

#include <gtest/gtest.h>

// For ALOGE, ALOGD, etc. if needed. In a real test setup, these might be mocked.
#include <log/log.h>

#include <array>
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

// For pipe(), read(), write(), close()
#include <unistd.h>

namespace android::sdv::dt::core::test {

// Helper to manage pipe file descriptors
class Pipe {
public:
    Pipe() = default;

    // Initializes the pipe. Returns true on success, false on failure.
    [[nodiscard]] bool Init() {
        if (pipe(fds_) == -1) {
            return false;
        }
        return true;
    }

    ~Pipe() {
        if (fds_[0] != -1) {
            close(fds_[0]);
        }
        if (fds_[1] != -1) {
            close(fds_[1]);
        }
    }

    int ReadFd() const { return fds_[0]; }
    int WriteFd() const { return fds_[1]; }

    void CloseReadFd() {
        if (fds_[0] != -1) {
            close(fds_[0]);
            fds_[0] = -1;
        }
    }

    void CloseWriteFd() {
        if (fds_[1] != -1) {
            close(fds_[1]);
            fds_[1] = -1;
        }
    }

private:
    int fds_[2] = {-1, -1};
};

// Test fixture for CommChannel
class CommChannelTest : public testing::Test {
protected:
    // Pipes for client-to-agent communication (CommChannel reads from this)
    Pipe client_to_agent_pipe_;
    // Pipes for agent-to-client communication (CommChannel writes to this)
    Pipe agent_to_client_pipe_;

    // For capturing messages from the ReadCallback
    std::vector<CommChannel::Message> received_messages_;
    std::mutex callback_mutex_;
    std::condition_variable callback_cv_;
    int expected_messages_ = 0;
    int client_id_ = 123;  // A dummy client ID for testing

    // The CommChannel instance under test
    std::unique_ptr<CommChannel> comm_channel_;

    void SetUp() override {
        // Initialize pipes and fail if creation is unsuccessful.
        ASSERT_TRUE(client_to_agent_pipe_.Init())
                << "Failed to create client->agent pipe: " << strerror(errno);
        ASSERT_TRUE(agent_to_client_pipe_.Init())
                << "Failed to create agent->client pipe: " << strerror(errno);

        // Reset state for each test
        received_messages_.clear();
        expected_messages_ = 0;
        comm_channel_.reset();
    }

    void TearDown() override {
        // Ensure comm_channel_ is destroyed to test its destructor behavior
        comm_channel_.reset();
    }

    // Static callback function to be passed to CommChannel::FromPipeFds
    static void TestReadCallback(const CommChannel::Message& message, void* ptr_arg,
                                 int32_t /*client_id*/) {
        CommChannelTest* self = static_cast<CommChannelTest*>(ptr_arg);
        std::lock_guard<std::mutex> lock(self->callback_mutex_);
        self->received_messages_.push_back(message);
        if (self->received_messages_.size() >= self->expected_messages_) {
            self->callback_cv_.notify_all();
        }
    }

    // Helper to wait for a certain number of messages to be received
    bool WaitForMessages(int count, std::chrono::seconds timeout = std::chrono::seconds(5)) {
        std::unique_lock<std::mutex> lock(callback_mutex_);
        expected_messages_ = count;
        return callback_cv_.wait_for(lock, timeout,
                                     [this, count] { return received_messages_.size() >= count; });
    }

    // Helper to create the CommChannel instance
    void CreateCommChannel() {
        comm_channel_ = CommChannel::FromPipeFds(client_to_agent_pipe_.ReadFd(),
                                                 agent_to_client_pipe_.WriteFd(), TestReadCallback,
                                                 this, client_id_);
    }
};

// --- CommChannel::Message Tests ---

TEST(CommChannelMessageTest, DefaultConstructor) {
    CommChannel::Message msg;
    EXPECT_EQ(msg.hdr_, CommChannel::Message::HeaderV1);
    EXPECT_EQ(msg.type_, CommChannel::Message::Undefined);
    EXPECT_EQ(msg.args_[0], 0);
    EXPECT_EQ(msg.args_[1], 0);
    EXPECT_EQ(msg.args_[2], 0);
    EXPECT_FALSE(msg.IsValid());
}

TEST(CommChannelMessageTest, ParameterizedConstructor) {
    CommChannel::Message msg(CommChannel::Message::DataAvailable, 1, 2, 3);
    EXPECT_EQ(msg.hdr_, CommChannel::Message::HeaderV1);
    EXPECT_EQ(msg.type_, CommChannel::Message::DataAvailable);
    EXPECT_EQ(msg.args_[0], 1);
    EXPECT_EQ(msg.args_[1], 2);
    EXPECT_EQ(msg.args_[2], 3);
    EXPECT_TRUE(msg.IsValid());
}

TEST(CommChannelMessageTest, DataAvailableMessage) {
    CommChannel::Message msg = CommChannel::Message::DataAvailableMessage(10, 5);
    EXPECT_EQ(msg.type(), CommChannel::Message::DataAvailable);
    EXPECT_EQ(msg.args_[0], 10);  // topic_id
    EXPECT_EQ(msg.args_[1], 5);   // message count
    EXPECT_TRUE(msg.IsValid());
}

TEST(CommChannelMessageTest, ExceptionMessage) {
    CommChannel::Message msg =
            CommChannel::Message::ExceptionMessage(20,
                                                   CommChannel::Message::Exception_TopicDestroyed);
    EXPECT_EQ(msg.type(), CommChannel::Message::Exception);
    EXPECT_EQ(msg.args_[0], 20);                                              // topic_id
    EXPECT_EQ(msg.args_[1], CommChannel::Message::Exception_TopicDestroyed);  // exception_type
    EXPECT_TRUE(msg.IsValid());
}

TEST(CommChannelMessageTest, SetNotificationStateMessage) {
    CommChannel::Message msg_enabled = CommChannel::Message::SetNotificationStateMessage(30, true);
    EXPECT_EQ(msg_enabled.type(), CommChannel::Message::SetNotificationState);
    EXPECT_EQ(msg_enabled.args_[0], 30);  // topic_id
    EXPECT_EQ(msg_enabled.args_[1], 1);   // enabled
    EXPECT_TRUE(msg_enabled.IsValid());

    CommChannel::Message msg_disabled =
            CommChannel::Message::SetNotificationStateMessage(30, false);
    EXPECT_EQ(msg_disabled.type(), CommChannel::Message::SetNotificationState);
    EXPECT_EQ(msg_disabled.args_[0], 30);  // topic_id
    EXPECT_EQ(msg_disabled.args_[1], 0);   // disabled
    EXPECT_TRUE(msg_disabled.IsValid());
}

TEST(CommChannelMessageTest, EqualityOperator) {
    CommChannel::Message msg1(CommChannel::Message::DataAvailable, 1, 2, 3);
    CommChannel::Message msg2(CommChannel::Message::DataAvailable, 1, 2, 3);
    CommChannel::Message msg3(CommChannel::Message::Exception, 1, 2, 3);

    EXPECT_EQ(msg1, msg2);
    EXPECT_NE(msg1, msg3);
}

// --- CommChannel Tests ---

TEST_F(CommChannelTest, FromPipeFds_Success) {
    CreateCommChannel();
    ASSERT_NE(comm_channel_, nullptr);
    EXPECT_TRUE(comm_channel_->IsConnected());
}

TEST_F(CommChannelTest, SendMessage_Success) {
    CreateCommChannel();
    ASSERT_TRUE(comm_channel_->IsConnected());

    CommChannel::Message sent_msg = CommChannel::Message::DataAvailableMessage(100, 5);
    ASSERT_TRUE(comm_channel_->SendMessage(sent_msg));

    CommChannel::Message received_from_pipe;
    ssize_t bytes_read =
            read(agent_to_client_pipe_.ReadFd(), &received_from_pipe, sizeof(CommChannel::Message));
    ASSERT_EQ(bytes_read, sizeof(CommChannel::Message));
    EXPECT_EQ(received_from_pipe, sent_msg);
}

TEST_F(CommChannelTest, ReadCallback_DataAvailable) {
    CreateCommChannel();
    ASSERT_TRUE(comm_channel_->IsConnected());

    CommChannel::Message msg_to_send = CommChannel::Message::DataAvailableMessage(200, 10);
    ssize_t bytes_written =
            write(client_to_agent_pipe_.WriteFd(), &msg_to_send, sizeof(CommChannel::Message));
    ASSERT_EQ(bytes_written, sizeof(CommChannel::Message));

    ASSERT_TRUE(WaitForMessages(1));
    ASSERT_EQ(received_messages_.size(), 1);
    EXPECT_EQ(received_messages_[0], msg_to_send);
}

TEST_F(CommChannelTest, ReadCallback_MultipleMessages) {
    CreateCommChannel();
    ASSERT_TRUE(comm_channel_->IsConnected());

    CommChannel::Message msg1 = CommChannel::Message::DataAvailableMessage(201, 1);
    CommChannel::Message msg2 = CommChannel::Message::
            ExceptionMessage(202, CommChannel::Message::Exception_ClientDisconnected);
    CommChannel::Message msg3 = CommChannel::Message::SetNotificationStateMessage(203, true);

    ASSERT_EQ(write(client_to_agent_pipe_.WriteFd(), &msg1, sizeof(CommChannel::Message)),
              sizeof(CommChannel::Message));
    ASSERT_EQ(write(client_to_agent_pipe_.WriteFd(), &msg2, sizeof(CommChannel::Message)),
              sizeof(CommChannel::Message));
    ASSERT_EQ(write(client_to_agent_pipe_.WriteFd(), &msg3, sizeof(CommChannel::Message)),
              sizeof(CommChannel::Message));

    ASSERT_TRUE(WaitForMessages(3));
    ASSERT_EQ(received_messages_.size(), 3);
    EXPECT_EQ(received_messages_[0], msg1);
    EXPECT_EQ(received_messages_[1], msg2);
    EXPECT_EQ(received_messages_[2], msg3);
}

TEST_F(CommChannelTest, ReadCallback_POLLNVAL_TriggersException) {
    CreateCommChannel();
    ASSERT_TRUE(comm_channel_->IsConnected());

    // Close the read end of the pipe that CommChannel reads from.
    // This should cause POLLNVAL on the read end.
    client_to_agent_pipe_.CloseReadFd();

    // Give the poll loop a moment to detect the hang-up.
    std::this_thread::sleep_for(std::chrono::milliseconds(10));

    // Expect an exception message due to disconnect
    ASSERT_TRUE(WaitForMessages(1));
    ASSERT_EQ(received_messages_.size(), 1);
    EXPECT_EQ(received_messages_[0].type(), CommChannel::Message::Exception);
    EXPECT_EQ(received_messages_[0].args_[1], CommChannel::Message::Exception_ClientDisconnected);
    EXPECT_FALSE(comm_channel_->IsConnected());  // Should be disconnected after exception
}

TEST_F(CommChannelTest, ReadCallback_POLLHUP_TriggersException) {
    CreateCommChannel();
    ASSERT_TRUE(comm_channel_->IsConnected());

    // Close the write end of the pipe that CommChannel reads from.
    // This should cause POLLHUP on the read end.
    client_to_agent_pipe_.CloseWriteFd();

    // Give the poll loop a moment to detect the hang-up.
    std::this_thread::sleep_for(std::chrono::milliseconds(10));

    // Expect an exception message due to disconnect
    ASSERT_TRUE(WaitForMessages(1));
    ASSERT_EQ(received_messages_.size(), 1);
    EXPECT_EQ(received_messages_[0].type(), CommChannel::Message::Exception);
    EXPECT_EQ(received_messages_[0].args_[1], CommChannel::Message::Exception_ClientDisconnected);
    EXPECT_FALSE(comm_channel_->IsConnected());  // Should be disconnected after exception
}

TEST_F(CommChannelTest, Destructor_JoinsThread) {
    CreateCommChannel();
    ASSERT_TRUE(comm_channel_->IsConnected());

    // Resetting the unique_ptr will call the destructor of CommChannelImpl.
    // The destructor writes to the internal termination pipe and then joins the read thread.
    comm_channel_.reset();

    // If the thread didn't join properly, the test would likely hang or crash.
    SUCCEED() << "CommChannel destructor executed and thread should have joined.";
}

}  // namespace android::sdv::dt::core::test
