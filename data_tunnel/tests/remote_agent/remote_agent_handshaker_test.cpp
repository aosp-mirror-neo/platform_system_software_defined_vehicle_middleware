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

#include "remote_agent/remote_agent_handshaker.h"

#include "connection.h"
#include "proto/dt_proto.h"
#include "remote_agent/remote_agent_test_utils.h"
#include "topic_manager.h"

#include <gtest/gtest.h>

#include <future>

namespace android::sdv::dt::agent::test {

using proto::Payload_Ping;
using proto::Payload_Pong;
using ::testing::Return;
using ::testing::StrictMock;

namespace {

static const char* kSelfVmName = "self_vm";
static const char* kPeerVmName_1 = "peer_vm_1";
static const char* kPeerVmName_2 = "peer_vm_2";

// EncodedNameForCert("peer_vm_1:google.sdv.dt.IDataTunnelAgent/default")
static const char* kPeerEncodedIdentity_1 = "8ugkptjiqkt0odn9rlodhtdvts7hlmk79vsmiv2ad1qulbnffh9g";

// EncodedNameForCert("peer_vm_2:google.sdv.dt.IDataTunnelAgent/default")
static const char* kPeerEncodedIdentity_2 = "hd0g6qbe2hpar3s6osjkdlkt31jhs1gdhuod8g0th0sl08haffu0";

class RemoteAgentHandshakerTest : public ::testing::Test {
protected:
    std::future<std::optional<std::string>> StartSecureHandshake(
            std::optional<std::string> expected_peer_vm_name) {
        return StartHandshake(/*require_authentication=*/true, expected_peer_vm_name);
    }

    std::future<std::optional<std::string>> StartInsecureHandshake(
            std::optional<std::string> expected_peer_vm_name) {
        return StartHandshake(/*require_authentication=*/false, expected_peer_vm_name);
    }

    StrictMock<FakeConnection> connection_;
    std::optional<RemoteAgentHandshaker> handshaker_;

private:
    std::future<std::optional<std::string>> StartHandshake(
            bool require_authentication, std::optional<std::string> expected_peer_vm_name) {
        // It's odd to spin up a thread to run the handshake, but since our FakeConnection class is
        // built to support testing of classes that do all their send/recv in dedicated threads
        // (i.e. RemoteAgentServerSession and RemoteAgentClient) it doesn't work if we try to run
        // the code under test and the test itself in the same thread.
        std::promise<std::optional<std::string>> result_promise;
        auto result_future = result_promise.get_future();
        handshaker_.emplace(kSelfVmName, require_authentication);
        handshake_thread_ = std::jthread(
                [this, expected_peer_vm_name, rp = std::move(result_promise)]() mutable {
                    rp.set_value(handshaker_->PerformHandshake(connection_, expected_peer_vm_name));
                });
        return result_future;
    }

    std::jthread handshake_thread_;
};

TEST_F(RemoteAgentHandshakerTest, PerformHandshake_Outbound_Secure_Success) {
    auto result_future = StartSecureHandshake(kPeerVmName_1);

    // Should send ping.
    Payload_Ping expected_ping(kSelfVmName);

    const auto ping_payload = connection_.GetNextSentDataIfPayload<Payload_Ping>();
    ASSERT_TRUE(ping_payload);
    EXPECT_EQ(ping_payload->vm_name, kSelfVmName);

    // Since security is enabled connection is queried for cert details when we get peer ping.
    EXPECT_CALL(connection_, AuthenticatedVmName()).WillOnce(Return(kPeerVmName_1));
    EXPECT_CALL(connection_, AuthenticatedIdentity()).WillOnce(Return(kPeerEncodedIdentity_1));

    // Simulate receipt of expected peer ping.
    connection_.EnqueueParcelForReceive(123, Payload_Ping(kPeerVmName_1));

    // Should send pong.
    const auto pong_payload = connection_.GetNextSentDataIfPayload<Payload_Pong>();
    ASSERT_TRUE(pong_payload);

    // Check that we still haven't gotten a result.
    EXPECT_EQ(result_future.wait_for(std::chrono::seconds(0)), std::future_status::timeout);

    // Simulate receipt of expected peer pong.
    connection_.EnqueueParcelForReceive(123, Payload_Pong());

    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    EXPECT_EQ(result_future.get(), kPeerVmName_1);
}

TEST_F(RemoteAgentHandshakerTest, PerformHandshake_Inbound_Secure_Success) {
    auto result_future = StartSecureHandshake(/*expected_peer_vm_name=*/std::nullopt);

    // Should send ping.
    Payload_Ping expected_ping(kSelfVmName);

    const auto ping_payload = connection_.GetNextSentDataIfPayload<Payload_Ping>();
    ASSERT_TRUE(ping_payload);
    EXPECT_EQ(ping_payload->vm_name, kSelfVmName);

    // Since security is enabled connection is queried for cert details when we get peer ping.
    EXPECT_CALL(connection_, AuthenticatedVmName()).WillOnce(Return(kPeerVmName_1));
    EXPECT_CALL(connection_, AuthenticatedIdentity()).WillOnce(Return(kPeerEncodedIdentity_1));

    // Simulate receipt of expected peer ping.
    connection_.EnqueueParcelForReceive(123, Payload_Ping(kPeerVmName_1));

    // Should send pong.
    const auto pong_payload = connection_.GetNextSentDataIfPayload<Payload_Pong>();
    ASSERT_TRUE(pong_payload);

    // Check that we still haven't gotten a result.
    EXPECT_EQ(result_future.wait_for(std::chrono::seconds(0)), std::future_status::timeout);

    // Simulate receipt of expected peer pong.
    connection_.EnqueueParcelForReceive(123, Payload_Pong());

    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    EXPECT_EQ(result_future.get(), kPeerVmName_1);
}

TEST_F(RemoteAgentHandshakerTest, PerformHandshake_Outbound_Insecure_Success) {
    auto result_future = StartInsecureHandshake(kPeerVmName_1);

    // Should send ping.
    Payload_Ping expected_ping(kSelfVmName);

    const auto ping_payload = connection_.GetNextSentDataIfPayload<Payload_Ping>();
    ASSERT_TRUE(ping_payload);
    EXPECT_EQ(ping_payload->vm_name, kSelfVmName);

    // Simulate receipt of expected peer ping.
    connection_.EnqueueParcelForReceive(123, Payload_Ping(kPeerVmName_1));

    // Should send pong.
    const auto pong_payload = connection_.GetNextSentDataIfPayload<Payload_Pong>();
    ASSERT_TRUE(pong_payload);

    // Simulate receipt of expected peer pong.
    connection_.EnqueueParcelForReceive(123, Payload_Pong());

    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    EXPECT_EQ(result_future.get(), kPeerVmName_1);
}

TEST_F(RemoteAgentHandshakerTest, PerformHandshake_Inbound_Insecure_Success) {
    auto result_future = StartInsecureHandshake(/*expected_peer_vm_name=*/std::nullopt);

    // Should send ping.
    Payload_Ping expected_ping(kSelfVmName);

    const auto ping_payload = connection_.GetNextSentDataIfPayload<Payload_Ping>();
    ASSERT_TRUE(ping_payload);
    EXPECT_EQ(ping_payload->vm_name, kSelfVmName);

    // Simulate receipt of expected peer ping.
    connection_.EnqueueParcelForReceive(123, Payload_Ping(kPeerVmName_1));

    // Should send pong.
    const auto pong_payload = connection_.GetNextSentDataIfPayload<Payload_Pong>();
    ASSERT_TRUE(pong_payload);

    // Simulate receipt of expected peer pong.
    connection_.EnqueueParcelForReceive(123, Payload_Pong());

    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    EXPECT_EQ(result_future.get(), kPeerVmName_1);
}

TEST_F(RemoteAgentHandshakerTest, PerformHandshake_FailsOnMissingPong) {
    auto result_future = StartInsecureHandshake(std::nullopt);

    // Should send ping.
    Payload_Ping expected_ping(kSelfVmName);

    const auto ping_payload = connection_.GetNextSentDataIfPayload<Payload_Ping>();
    ASSERT_TRUE(ping_payload);
    EXPECT_EQ(ping_payload->vm_name, kSelfVmName);

    // Simulate receipt of expected peer ping.
    connection_.EnqueueParcelForReceive(123, Payload_Ping(kPeerVmName_1));

    // Should send pong.
    const auto pong_payload = connection_.GetNextSentDataIfPayload<Payload_Pong>();
    ASSERT_TRUE(pong_payload);

    // Simulate receipt of UNEXPECTED peer ping when we should be getting pong
    connection_.EnqueueReceiveFailure();

    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    EXPECT_EQ(result_future.get(), std::nullopt);
}

TEST_F(RemoteAgentHandshakerTest, PerformHandshake_FailsOnNonPong) {
    auto result_future = StartInsecureHandshake(std::nullopt);

    // Should send ping.
    Payload_Ping expected_ping(kSelfVmName);

    const auto ping_payload = connection_.GetNextSentDataIfPayload<Payload_Ping>();
    ASSERT_TRUE(ping_payload);
    EXPECT_EQ(ping_payload->vm_name, kSelfVmName);

    // Simulate receipt of expected peer ping.
    connection_.EnqueueParcelForReceive(123, Payload_Ping(kPeerVmName_1));

    // Should send pong.
    const auto pong_payload = connection_.GetNextSentDataIfPayload<Payload_Pong>();
    ASSERT_TRUE(pong_payload);

    // Simulate receipt of UNEXPECTED peer ping when we should be getting pong
    connection_.EnqueueParcelForReceive(123, Payload_Ping(kPeerVmName_1));

    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    EXPECT_EQ(result_future.get(), std::nullopt);
}

TEST_F(RemoteAgentHandshakerTest, PerformHandshake_Secure_FailsOnMissingAuthIdentity) {
    auto result_future = StartSecureHandshake(kPeerVmName_1);

    // Should send ping.
    Payload_Ping expected_ping(kSelfVmName);

    const auto ping_payload = connection_.GetNextSentDataIfPayload<Payload_Ping>();
    ASSERT_TRUE(ping_payload);
    EXPECT_EQ(ping_payload->vm_name, kSelfVmName);

    // Since security is enabled connection is queried for cert details when we get peer ping.
    EXPECT_CALL(connection_, AuthenticatedVmName()).WillOnce(Return(kPeerVmName_1));
    EXPECT_CALL(connection_, AuthenticatedIdentity()).WillOnce(Return(std::nullopt));

    // Simulate receipt of expected peer ping.
    connection_.EnqueueParcelForReceive(123, Payload_Ping(kPeerVmName_1));

    // Failure returned
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    EXPECT_EQ(result_future.get(), std::nullopt);

    // Nothing else was sent
    EXPECT_FALSE(connection_.GetNextSentData(/*timeout_seconds=*/0));
}

TEST_F(RemoteAgentHandshakerTest, PerformHandshake_Secure_FailsOnIncorrectAuthIdentity) {
    auto result_future = StartSecureHandshake(kPeerVmName_1);

    // Should send ping.
    Payload_Ping expected_ping(kSelfVmName);

    const auto ping_payload = connection_.GetNextSentDataIfPayload<Payload_Ping>();
    ASSERT_TRUE(ping_payload);
    EXPECT_EQ(ping_payload->vm_name, kSelfVmName);

    // Since security is enabled connection is queried for cert details when we get peer ping.
    EXPECT_CALL(connection_, AuthenticatedVmName()).WillOnce(Return(kPeerVmName_1));
    EXPECT_CALL(connection_, AuthenticatedIdentity()).WillOnce(Return(kPeerEncodedIdentity_2));

    // Simulate receipt of expected peer ping.
    connection_.EnqueueParcelForReceive(123, Payload_Ping(kPeerVmName_1));

    // Failure returned
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    EXPECT_EQ(result_future.get(), std::nullopt);

    // Nothing else was sent
    EXPECT_FALSE(connection_.GetNextSentData(/*timeout_seconds=*/0));
}

TEST_F(RemoteAgentHandshakerTest, PerformHandshake_Secure_FailsOnMissingAuthVmName) {
    auto result_future = StartSecureHandshake(kPeerVmName_1);

    // Should send ping.
    Payload_Ping expected_ping(kSelfVmName);

    const auto ping_payload = connection_.GetNextSentDataIfPayload<Payload_Ping>();
    ASSERT_TRUE(ping_payload);
    EXPECT_EQ(ping_payload->vm_name, kSelfVmName);

    // Since security is enabled connection is queried for cert details when we get peer ping.
    EXPECT_CALL(connection_, AuthenticatedVmName()).WillOnce(Return(std::nullopt));

    // Simulate receipt of expected peer ping.
    connection_.EnqueueParcelForReceive(123, Payload_Ping(kPeerVmName_1));

    // Failure returned
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    EXPECT_EQ(result_future.get(), std::nullopt);

    // Nothing else was sent
    EXPECT_FALSE(connection_.GetNextSentData(/*timeout_seconds=*/0));
}

TEST_F(RemoteAgentHandshakerTest, PerformHandshake_Secure_FailsOnIncorrectAuthVmName) {
    auto result_future = StartSecureHandshake(kPeerVmName_1);

    // Should send ping.
    Payload_Ping expected_ping(kSelfVmName);

    const auto ping_payload = connection_.GetNextSentDataIfPayload<Payload_Ping>();
    ASSERT_TRUE(ping_payload);
    EXPECT_EQ(ping_payload->vm_name, kSelfVmName);

    // Since security is enabled connection is queried for cert details when we get peer ping.
    EXPECT_CALL(connection_, AuthenticatedVmName()).WillOnce(Return(kPeerVmName_2));

    // Simulate receipt of expected peer ping.
    connection_.EnqueueParcelForReceive(123, Payload_Ping(kPeerVmName_1));

    // Failure returned
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    EXPECT_EQ(result_future.get(), std::nullopt);

    // Nothing else was sent
    EXPECT_FALSE(connection_.GetNextSentData(/*timeout_seconds=*/0));
}

TEST_F(RemoteAgentHandshakerTest, PerformHandshake_Outbound_FailsOnUnexpectedPingVmName) {
    auto result_future = StartSecureHandshake(kPeerVmName_1);

    // Should send ping.
    Payload_Ping expected_ping(kSelfVmName);

    const auto ping_payload = connection_.GetNextSentDataIfPayload<Payload_Ping>();
    ASSERT_TRUE(ping_payload);
    EXPECT_EQ(ping_payload->vm_name, kSelfVmName);

    // Simulate receipt of peer ping with unexpected VM name.
    connection_.EnqueueParcelForReceive(123, Payload_Ping(kPeerVmName_2));

    // Failure returned
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    EXPECT_EQ(result_future.get(), std::nullopt);

    // Nothing else was sent
    EXPECT_FALSE(connection_.GetNextSentData(/*timeout_seconds=*/0));
}

TEST_F(RemoteAgentHandshakerTest, PerformHandshake_FailsOnMissingPing) {
    auto result_future = StartSecureHandshake(kPeerVmName_1);

    // Should send ping.
    Payload_Ping expected_ping(kSelfVmName);

    const auto ping_payload = connection_.GetNextSentDataIfPayload<Payload_Ping>();
    ASSERT_TRUE(ping_payload);
    EXPECT_EQ(ping_payload->vm_name, kSelfVmName);

    // Simulate receipt failure.
    connection_.EnqueueReceiveFailure();

    // Failure returned
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    EXPECT_EQ(result_future.get(), std::nullopt);

    // Nothing else was sent
    EXPECT_FALSE(connection_.GetNextSentData(/*timeout_seconds=*/0));
}

TEST_F(RemoteAgentHandshakerTest, PerformHandshake_FailsOnNonPing) {
    auto result_future = StartSecureHandshake(kPeerVmName_1);

    // Should send ping.
    Payload_Ping expected_ping(kSelfVmName);

    const auto ping_payload = connection_.GetNextSentDataIfPayload<Payload_Ping>();
    ASSERT_TRUE(ping_payload);
    EXPECT_EQ(ping_payload->vm_name, kSelfVmName);

    // Simulate receipt of non-ping payload.
    connection_.EnqueueParcelForReceive(123, Payload_Pong());

    // Failure returned
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    EXPECT_EQ(result_future.get(), std::nullopt);

    // Nothing else was sent
    EXPECT_FALSE(connection_.GetNextSentData(/*timeout_seconds=*/0));
}

}  // namespace
}  // namespace android::sdv::dt::agent::test
