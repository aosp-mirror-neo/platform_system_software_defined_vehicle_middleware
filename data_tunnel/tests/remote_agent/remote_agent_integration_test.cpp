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

#include "config.h"
#include "connection.h"
#include "connection_grpc/connection_manager.h"
#include "proto/dt_proto.h"
#include "remote_agent/remote_agent_client.h"
#include "remote_agent/remote_agent_handshaker.h"
#include "remote_agent/remote_agent_server_session.h"
#include "remote_agent/remote_agent_test_utils.h"
#include "remote_agent_test_utils.h"
#include "topic_manager.h"

#include <android-base/logging.h>
#include <gtest/gtest.h>
#include <openssl/bio.h>
#include <openssl/crypto.h>

#include <future>
#include <memory>
#include <queue>

using ::testing::_;
using ::testing::StrictMock;

using ::aidl::google::sdv::identity::ServiceFqin;
using ::aidl::google::sdv::identity::ServiceIdentity;

namespace android::sdv::dt::agent::test {

namespace {

static const char* kTestVmName_1 = "peer_vm_1";
static const char* kTestVmName_2 = "peer_vm_2";

// EncodedNameForCert("peer_vm_1:com.android.sdv.dt.IDataTunnelAgent/default")
static const char* kPeerEncodedIdentity_1 = "456qtdgtj9vgmjj273vt3dh215r3qfar57g8vbemocj6iaiv4k00";

// EncodedNameForCert("peer_vm_2:com.android.sdv.dt.IDataTunnelAgent/default")
static const char* kPeerEncodedIdentity_2 = "v2kccpjo4ct81v85hugk8vnuc989hkhh107dt8ct2nm7l833l7pg";

static const char* kTestTopicName = "test-topic";
static const size_t kTestTopicMessageSize = 57;
static const size_t kTestTopicMessageCount = 64;
static const int kTestTopicPublisherClientId = 14;

static const ServiceFqin kDtAgentFqin_1 = ServiceFqin{
        /*sdvVmName*/ kTestVmName_1,
        /*sdvPackageName*/ config::kPackageName,
        /*serviceBundleName*/ config::kServiceBundleName,
        /*serviceInstanceName*/ "default",
};

static const ServiceIdentity kDtAgentServiceIdentity_1 = ServiceIdentity{
        .name = kDtAgentFqin_1,
};

using ConnectionResult = ::sdv::SdvResult<IConnectionHolder>;
struct TestEventHandler : ConnectionManager::EventHandler {
    virtual void OnConnectionManagerEvent(::sdv::SdvResult<IConnectionHolder> result) override {
        std::unique_lock guard(lock_);
        result_queue_.emplace(std::move(result));
        cv_.notify_one();
    }

    ConnectionResult GetNextConnectionResult() {
        std::unique_lock guard(lock_);
        if (!cv_.wait_for(guard, std::chrono::seconds(5),
                          [this]() { return !result_queue_.empty(); })) {
            return base::unexpected(::sdv::SdvStatus(::sdv::SdvStatusCode::kDeadlineExceeded));
        }
        auto result = std::move(result_queue_.front());
        result_queue_.pop();
        return result;
    }

private:
    std::mutex lock_;
    std::condition_variable cv_;
    std::queue<::sdv::SdvResult<IConnectionHolder>> result_queue_;
};

struct IntegrationTestFixture {
    TopicManager server_topic_manager;

    Topic* server_topic = nullptr;
    std::unique_ptr<ConnectionManager> server_connection_manager;
    TestEventHandler server_event_handler;

    TopicManager client_topic_manager;
    Topic* client_topic = nullptr;
    std::unique_ptr<ConnectionManager> client_connection_manager;

    static IntegrationTestFixture MakeInsecure() {
        return IntegrationTestFixture(/*server_ssl_config=*/std::nullopt,
                                      /*client_ssl_config=*/std::nullopt);
    }

    SocketAddressV4 StartServerListen() {
        server_connection_manager->Listen(server_event_handler);
        return SocketAddressV4(server_connection_manager->GetLocalIp(),
                               server_connection_manager->GetListenPort());
    }

    std::pair<IConnectionHolder, IConnectionHolder> MakeConnectionPair() {
        auto listen_address = StartServerListen();

        TestEventHandler client_event_handler;
        client_connection_manager->Connect(listen_address, "", client_event_handler);

        auto server_conn_result = server_event_handler.GetNextConnectionResult();
        CHECK(server_conn_result.has_value());

        auto client_conn_result = client_event_handler.GetNextConnectionResult();
        CHECK(client_conn_result.has_value());

        return std::make_pair(std::move(server_conn_result.value()),
                              std::move(client_conn_result.value()));
    }

    void RegisterTestTopicInClient() {
        ::aidl::google::sdv::data_tunnel::PublicationDescriptor pub_desc;
        ASSERT_TRUE(server_topic->MakeDescriptor(&pub_desc));
        auto client_mq = std::make_unique<AidlMQ>(pub_desc.mq_desc);
        Topic::Metadata metadata(kTestTopicName, kTestTopicMessageSize, kTestTopicMessageCount);
        client_topic = client_topic_manager.RegisterTopic(metadata, /* client_id= */ 0,
                                                          /*is_remote=*/true, std::move(client_mq));
    }

private:
    IntegrationTestFixture(std::optional<ConnectionManager::SslConfig> server_ssl_config,
                           std::optional<ConnectionManager::SslConfig> client_ssl_config) :
          server_connection_manager(
                  CreateGrpcConnectionManager(IPAddressV4::GetLocal().value(), server_ssl_config)),
          client_connection_manager(
                  CreateGrpcConnectionManager(IPAddressV4::GetLocal().value(), client_ssl_config)) {
        auto server_mq = std::make_unique<AidlMQ>(kTestTopicMessageSize * kTestTopicMessageCount);
        Topic::Metadata metadata(kTestTopicName, kTestTopicMessageSize, kTestTopicMessageCount);
        server_topic =
                server_topic_manager.RegisterTopic(metadata, kTestTopicPublisherClientId,
                                                   /*is_remote=*/false, std::move(server_mq));
    }
};

class RemoteAgentHandshakerPairTest : public ::testing::Test {};

TEST_F(RemoteAgentHandshakerPairTest, PerformHandshake_Insecure_Success) {
    RemoteAgentHandshaker handshaker_1(kTestVmName_1, false);
    RemoteAgentHandshaker handshaker_2(kTestVmName_2, false);

    auto fixture = IntegrationTestFixture::MakeInsecure();
    auto [conn_1, conn_2] = fixture.MakeConnectionPair();

    // Handshaker 1 takes server role (no expected peer name)
    std::promise<std::optional<std::string>> result_promise_1;
    auto result_future_1 = result_promise_1.get_future();
    std::jthread handshake_thread_1([&]() {
        result_promise_1.set_value(handshaker_1.PerformHandshake(*conn_1, std::nullopt));
    });

    // Handshaker 2 takes server role (no expected peer name)
    std::promise<std::optional<std::string>> result_promise_2;
    auto result_future_2 = result_promise_2.get_future();
    std::jthread handshake_thread_2([&]() {
        result_promise_2.set_value(handshaker_2.PerformHandshake(*conn_2, kTestVmName_1));
    });

    ASSERT_EQ(result_future_1.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    ASSERT_EQ(result_future_2.wait_for(std::chrono::seconds(1)), std::future_status::ready);

    EXPECT_EQ(result_future_1.get(), kTestVmName_2);
    EXPECT_EQ(result_future_2.get(), kTestVmName_1);
}

class RemoteAgentClientToServerSessionTest : public ::testing::Test {
protected:
    RemoteAgentClientToServerSessionTest() :
          server_handshaker_(kTestVmName_1, false),
          client_handshaker_(kTestVmName_2, false),
          fixture_(IntegrationTestFixture::MakeInsecure()) {}

    ~RemoteAgentClientToServerSessionTest() {
        if (client_) {
            // Explicitly terminate client first to prevent another Connect() call.
            client_->RequestTerminate();
        }
    }

    void PrepareConnectedClientAndServerSession() {
        auto server_address = fixture_.StartServerListen();

        client_.emplace(kDtAgentServiceIdentity_1, server_address,
                        *(fixture_.client_connection_manager), client_handshaker_,
                        fixture_.client_topic_manager);

        auto server_conn_result = fixture_.server_event_handler.GetNextConnectionResult();
        ASSERT_TRUE(server_conn_result.has_value());

        std::promise<void> handshake_promise;
        auto handshake_callback = [&](RemoteAgentServerSession*, const std::string&) {
            handshake_promise.set_value();
        };
        server_session_.emplace(std::move(server_conn_result.value()), server_handshaker_,
                                handshake_callback, fixture_.server_topic_manager);

        ASSERT_EQ(handshake_promise.get_future().wait_for(std::chrono::seconds(1)),
                  std::future_status::ready);

        // Brief additional sleep to complete client and server session startup.
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }

    RemoteAgentHandshaker server_handshaker_;
    RemoteAgentHandshaker client_handshaker_;
    IntegrationTestFixture fixture_;

    std::optional<RemoteAgentServerSession> server_session_;
    std::optional<RemoteAgentClient> client_;
};

TEST_F(RemoteAgentClientToServerSessionTest, Connect_And_Disconnect) {
    auto server_address = fixture_.StartServerListen();

    // Client will initiate connect.
    RemoteAgentClient client(kDtAgentServiceIdentity_1, server_address,
                             *(fixture_.client_connection_manager), client_handshaker_,
                             fixture_.client_topic_manager);

    // Wait for server connection to arrive.
    auto server_conn_result = fixture_.server_event_handler.GetNextConnectionResult();
    ASSERT_TRUE(server_conn_result.has_value());

    // Promise-resolving callback to let test wait for handshake success from server session.
    std::promise<std::string> handshake_vm_name_promise;
    auto handshake_vm_name_future = handshake_vm_name_promise.get_future();
    auto handshake_callback = [&](RemoteAgentServerSession*, const std::string& vm_name) {
        handshake_vm_name_promise.set_value(vm_name);
    };

    // Construct server session with inbound connection.
    RemoteAgentServerSession server_session(std::move(server_conn_result.value()),
                                            server_handshaker_, handshake_callback,
                                            fixture_.server_topic_manager);

    // Wait for and validate handshake result from server session.
    ASSERT_EQ(handshake_vm_name_future.wait_for(std::chrono::seconds(1)),
              std::future_status::ready);
    EXPECT_EQ(handshake_vm_name_future.get(), kTestVmName_2);

    // Brief wait then check that both endpoints are operational.
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
    EXPECT_FALSE(client.IsTerminated());
    EXPECT_FALSE(server_session.IsTerminated());

    // Explicitly terminate Client first so we don't get another Connect()
    client.RequestTerminate();

    // Look for both endpoints to show terminated state.
    WaitForConditionOrTimeout(
            [&]() { return client.IsTerminated() && server_session.IsTerminated(); });
    EXPECT_TRUE(client.IsTerminated());
    EXPECT_TRUE(server_session.IsTerminated());
}

TEST_F(RemoteAgentClientToServerSessionTest, FetchLastMessage_Success_With_Empty) {
    PrepareConnectedClientAndServerSession();

    auto pending_result = client_->FetchLastMessage(kTestTopicName);
    ASSERT_EQ(pending_result.wait_for(std::chrono::seconds(1)), std::future_status::ready);

    // We get an OK result with an empty vector.
    auto result = pending_result.get();
    ASSERT_TRUE(result.has_value());
    EXPECT_TRUE(result.value().empty());
}

TEST_F(RemoteAgentClientToServerSessionTest, FetchLastMessage_Success_With_Nonempty) {
    PrepareConnectedClientAndServerSession();

    // Construct messages with nonrandom pattern.
    uint8_t test_message[kTestTopicMessageSize];
    for (size_t i = 0; i < kTestTopicMessageSize; i++) {
        test_message[i] = (i + 17) % 256;
    }

    // Write message to topic on server side.
    auto server_topic = fixture_.server_topic_manager.FindTopic(kTestTopicName, false);
    ASSERT_TRUE(server_topic);
    server_topic->Write(test_message, sizeof(test_message));

    auto pending_result = client_->FetchLastMessage(kTestTopicName);
    ASSERT_EQ(pending_result.wait_for(std::chrono::seconds(1)), std::future_status::ready);

    // We get an OK result with an empty vector.
    auto result = pending_result.get();
    ASSERT_TRUE(result.has_value());
    const auto& result_data = result.value();
    EXPECT_EQ(result_data.size(), kTestTopicMessageSize);
    EXPECT_EQ(memcmp(result_data.data(), test_message, kTestTopicMessageSize), 0);
}

TEST_F(RemoteAgentClientToServerSessionTest, FetchLastMessage_Failure_For_Unknown) {
    PrepareConnectedClientAndServerSession();

    auto pending_result = client_->FetchLastMessage("bad-topic-name");
    ASSERT_EQ(pending_result.wait_for(std::chrono::seconds(1)), std::future_status::ready);

    // We get an error result
    auto result = pending_result.get();
    EXPECT_FALSE(result.has_value());
}

TEST_F(RemoteAgentClientToServerSessionTest, Subscription_Success_Full_Lifecycle) {
    PrepareConnectedClientAndServerSession();
    fixture_.RegisterTestTopicInClient();

    auto client_reader = fixture_.client_topic->CreateReader();
    ASSERT_TRUE(client_reader);

    auto subscribe_result = client_->RequestSubscribe(kTestTopicName);
    ASSERT_TRUE(subscribe_result.isOk());

    EXPECT_EQ(client_reader->AvailableToRead(), 0);

    // Construct messages with nonrandom pattern.
    uint8_t test_messages[3 * kTestTopicMessageSize];
    for (size_t i = 0; i < 3 * kTestTopicMessageSize; i++) {
        test_messages[i] = (i + 17) % 256;
    }

    // Write messages then notify; should be enough to get data to client mq.
    fixture_.server_topic->Write(test_messages, 3 * kTestTopicMessageSize);
    fixture_.server_topic->NotifyWritten(3);

    // Wait for expected messages to show up in client topic then verify content.
    WaitForConditionOrTimeout(
            [&]() { return client_reader->AvailableToRead() >= 3 * kTestTopicMessageSize; });
    ASSERT_GE(client_reader->AvailableToRead(), 3 * kTestTopicMessageSize);
    uint8_t recv_messages_1[3 * kTestTopicMessageSize];
    ASSERT_TRUE(client_reader->Read(recv_messages_1, 3 * kTestTopicMessageSize));
    EXPECT_EQ(memcmp(recv_messages_1, test_messages, 3 * kTestTopicMessageSize), 0);
    ASSERT_EQ(client_reader->AvailableToRead(), 0);  // Verify we emptied queue.

    // Write some more, then notify again.
    fixture_.server_topic->Write(test_messages + kTestTopicMessageSize, 2 * kTestTopicMessageSize);
    fixture_.server_topic->NotifyWritten(2);

    // Wait for messages to reach client again.
    WaitForConditionOrTimeout(
            [&]() { return client_reader->AvailableToRead() >= 2 * kTestTopicMessageSize; });
    EXPECT_GE(client_reader->AvailableToRead(), 2 * kTestTopicMessageSize);

    // Simulate death of publisher on server side.
    fixture_.server_topic_manager.ClientDisconnected(kTestTopicPublisherClientId);

    // Should lead to client topic becoming inactive.
    WaitForConditionOrTimeout([&]() { return !fixture_.client_topic->IsActive(); });
    EXPECT_FALSE(fixture_.client_topic->IsActive());
}

TEST_F(RemoteAgentClientToServerSessionTest, Subscription_Failure_When_Unknown_Topic) {
    PrepareConnectedClientAndServerSession();

    // Topic needs to exist in client to send request to server.
    fixture_.RegisterTestTopicInClient();

    // Kill the topic in the server.
    fixture_.server_topic_manager.ClientDisconnected(kTestTopicPublisherClientId);

    // Subscribe will return ok because it doesn't wait for peer.
    auto subscribe_result = client_->RequestSubscribe(kTestTopicName);
    ASSERT_TRUE(subscribe_result.isOk());

    // But if we wait a bit, the topic gets destroyed on the client, once
    // the server's error message gets back to us.
    WaitForConditionOrTimeout([&]() { return !fixture_.client_topic->IsActive(); });
    EXPECT_FALSE(fixture_.client_topic->IsActive());
}

}  // namespace
}  // namespace android::sdv::dt::agent::test
