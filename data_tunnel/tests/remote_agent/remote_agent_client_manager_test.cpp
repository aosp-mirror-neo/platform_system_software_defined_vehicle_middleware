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

#include "remote_agent/remote_agent_client_manager.h"

#include "connection.h"
#include "remote_agent/remote_agent_common.h"
#include "remote_agent/remote_agent_test_utils.h"
#include "remote_agent_test_utils.h"
#include "tests/mock_agents.h"
#include "topic_manager.h"

#include <gtest/gtest.h>

namespace android::sdv::dt::agent::test {

using ::aidl::google::sdv::identity::ServiceFqin;
using ::aidl::google::sdv::identity::ServiceIdentity;
using ::aidl::google::sdv::service_discovery::common::metadata::TransportMetadata;
using ::aidl::google::sdv::service_discovery::discovery::IServiceUnitChangeListener;
using ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition;
using proto::Payload_LastMessageRequest;
using proto::Payload_LastMessageResult;
using proto::Payload_SubscribeRequest;
using proto::Payload_SubscribeResult;
using proto::Payload_TopicDestroyed;
using proto::Payload_TopicRawData;

using ::testing::_;
using ::testing::Address;
using ::testing::Eq;
using ::testing::Mock;
using ::testing::Return;
using ::testing::StrictMock;

namespace {

constexpr auto MakeTestFqin(const std::string& vm_name) {
    return ServiceFqin{/*sdvVmName*/ vm_name,
                       /*sdvPackageName*/ config::kPackageName,
                       /*serviceBundleName*/ config::kServiceBundleName,
                       /*serviceInstanceName*/ "default"};
}

constexpr auto MakeTestIdentity(const std::string& vm_name, int64_t sid) {
    return ServiceIdentity{
            .name = MakeTestFqin(vm_name),
            .sid = sid,
    };
}

auto MakeTestServiceUnitDefinition(const std::string& vm_name, uint64_t sid) {
    return ServiceUnitDefinition{
            .identity = MakeTestIdentity(vm_name, sid),
            .serviceUnitName = config::kServiceUnitName,
    };
}

const char* kTestSelfVmName = "self-vm-name";
const int64_t kTestSelfSid = 47;
const ServiceIdentity kTestSelfIdentity = MakeTestIdentity(kTestSelfVmName, kTestSelfSid);
const ServiceUnitDefinition kTestSelfServiceUnitDefinition =
        MakeTestServiceUnitDefinition(kTestSelfVmName, kTestSelfSid);

const char* kTestPeerVmName = "peer-vm-name";
const ServiceFqin kTestPeerFqin = MakeTestFqin(kTestPeerVmName);
const int64_t kTestPeerSid = 74;
const ServiceUnitDefinition kTestPeerServiceUnitDefinition =
        MakeTestServiceUnitDefinition(kTestPeerVmName, kTestPeerSid);

const char* kTestTopicName = "test-topic-name";
class RemoteAgentClientManagerTest : public ::testing::Test {
protected:
    RemoteAgentClientManagerTest() :
          service_discovery_agent_(
                  ndk::SharedRefBase::make<StrictMock<MockServiceDiscoveryAgent>>()),
          transport_support_agent_(
                  ndk::SharedRefBase::make<StrictMock<MockTransportSupportAgent>>()),
          client_manager_(kTestSelfIdentity, handshaker_, topic_manager_) {}

    StrictMock<MockConnectionManager> connection_manager_;
    StrictMock<MockHandshaker> handshaker_;
    TopicManager topic_manager_;
    std::shared_ptr<StrictMock<MockServiceDiscoveryAgent>> service_discovery_agent_;
    std::shared_ptr<StrictMock<MockTransportSupportAgent>> transport_support_agent_;
    std::shared_ptr<IServiceUnitChangeListener> listener_;
    RemoteAgentClientManager client_manager_;
    ConnectionManager::EventHandler* event_handler_;
    FakeConnection* connection_;

    ~RemoteAgentClientManagerTest() {
        if (event_handler_) {
            listener_->onServiceUnitUnregistered(kTestPeerServiceUnitDefinition);
            if (!connection_) {
                event_handler_->OnConnectionManagerEvent(
                        base::unexpected(::sdv::SdvStatus(::sdv::SdvStatusCode::kInternal)));
            }
        }
    }

    void StartupClientManager() {
        EXPECT_CALL(*service_discovery_agent_, registerServiceUnitChangeListenerByType(_, _))
                .WillOnce([&](auto&, auto listener) {
                    listener_ = listener;
                    return ndk::ScopedAStatus::ok();
                });

        ASSERT_TRUE(client_manager_.Startup(connection_manager_, service_discovery_agent_,
                                            transport_support_agent_));
        ASSERT_TRUE(listener_);

        // Set up expectation for eventual unregister on destruction.
        EXPECT_CALL(*service_discovery_agent_, unregisterServiceUnitChangeListenerByType(_, _))
                .WillOnce(Return(ndk::ScopedAStatus::ok()));
    }

    void CreatePendingClient() {
        // Well get call to get metadata for self or to initiate a connection.
        TransportMetadata transport_metadata;
        SocketAddressV4 peer_address(static_cast<in_addr>(0x01020304U), 31338);
        peer_address.Serialize(transport_metadata.value_holder);
        EXPECT_CALL(*transport_support_agent_, getAgentMetadata(_, _, _, _))
                .WillOnce([&](auto, auto, auto, auto out_md) {
                    *out_md = transport_metadata;
                    return ndk::ScopedAStatus::ok();
                });

        std::promise<void> connect_promise;
        EXPECT_CALL(connection_manager_,
                    Connect(Eq(peer_address),
                            Eq("peer-vm-name:google.sdv.dt.IDataTunnelAgent/default"), _))
                .WillOnce([&](auto&, auto&, auto& eh) {
                    event_handler_ = &eh;
                    connect_promise.set_value();
                });

        listener_->onServiceUnitRegistered(kTestPeerServiceUnitDefinition);

        ASSERT_EQ(connect_promise.get_future().wait_for(std::chrono::seconds(1)),
                  std::future_status::ready);
        ASSERT_NE(event_handler_, nullptr);

        Mock::VerifyAndClearExpectations(transport_support_agent_.get());
        Mock::VerifyAndClearExpectations(&connection_manager_);

        // Ensure that client is visible.
        ASSERT_TRUE(WaitForConditionOrTimeout(
                [&]() { return client_manager_.CurrentClientNames().contains(kTestPeerVmName); }));
    }

    void CompleteClientConnection() {
        auto owned_connection = std::make_unique<FakeConnection>();
        connection_ = owned_connection.get();

        // Client will request handshake and get success.
        std::promise<void> handshake_promise;
        EXPECT_CALL(handshaker_, PerformHandshake(_, _)).WillOnce([&](auto&, auto) {
            handshake_promise.set_value();
            return kTestPeerVmName;
        });

        event_handler_->OnConnectionManagerEvent(std::move(owned_connection));

        ASSERT_EQ(handshake_promise.get_future().wait_for(std::chrono::seconds(1)),
                  std::future_status::ready);

        Mock::VerifyAndClearExpectations(&handshaker_);

        // Brief wait to let client transition into ACTIVE
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }

    void CreateRemoteTestTopic() {
        Topic::Metadata metadata(kTestTopicName, /*message_size=*/8, /*message_count=*/8);
        auto mq = std::make_unique<AidlMQ>(8 * 8);
        ASSERT_TRUE(
                topic_manager_.RegisterTopic(metadata, /* client_id = */ 0, true, std::move(mq)));
    }
};

TEST_F(RemoteAgentClientManagerTest, StartupRegistersAndShutdownUnregistersListener) {
    // Create will register the change listener.
    EXPECT_CALL(*service_discovery_agent_,
                registerServiceUnitChangeListenerByType(Eq(kDtAgentUnitType), _))
            .WillOnce([&](auto&, auto listener) {
                listener_ = listener;
                return ndk::ScopedAStatus::ok();
            });

    ASSERT_TRUE(client_manager_.Startup(connection_manager_, service_discovery_agent_,
                                        transport_support_agent_));

    Mock::VerifyAndClearExpectations(service_discovery_agent_.get());

    EXPECT_CALL(*service_discovery_agent_,
                unregisterServiceUnitChangeListenerByType(Eq(kDtAgentUnitType), Eq(listener_)))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));

    client_manager_.Shutdown();

    Mock::VerifyAndClearExpectations(service_discovery_agent_.get());
}

TEST_F(RemoteAgentClientManagerTest, Startup_FailsOnListenerRegistrationFailure) {
    EXPECT_CALL(*service_discovery_agent_,
                registerServiceUnitChangeListenerByType(Eq(kDtAgentUnitType), _))
            .WillOnce(Return(ndk::ScopedAStatus::fromExceptionCode(EX_ILLEGAL_ARGUMENT)));

    // We'll also get an unregister call from the Shutdown() after the registration  fails.
    EXPECT_CALL(*service_discovery_agent_,
                unregisterServiceUnitChangeListenerByType(Eq(kDtAgentUnitType), _))
            .WillOnce(Return(ndk::ScopedAStatus::fromExceptionCode(EX_ILLEGAL_ARGUMENT)));

    ASSERT_FALSE(client_manager_.Startup(connection_manager_, service_discovery_agent_,
                                         transport_support_agent_));
}

TEST_F(RemoteAgentClientManagerTest, onServiceUnitRegistered_IgnoresSelfIdentity) {
    StartupClientManager();

    // Should not see call to get metadata for self or to initiate a connection.
    EXPECT_CALL(*transport_support_agent_, getAgentMetadata(_, _, _, _)).Times(0);
    EXPECT_CALL(connection_manager_, Connect(_, _, _)).Times(0);

    listener_->onServiceUnitRegistered(kTestSelfServiceUnitDefinition);
}

TEST_F(RemoteAgentClientManagerTest, onServiceUnitRegistered_CreatesClientThatInitiatesConnect) {
    StartupClientManager();

    ASSERT_TRUE(listener_);

    // Well get call to get metadata for self or to initiate a connection.
    TransportMetadata transport_metadata;
    SocketAddressV4 peer_address(static_cast<in_addr>(0x01020304U), 31338);
    peer_address.Serialize(transport_metadata.value_holder);
    EXPECT_CALL(*transport_support_agent_,
                getAgentMetadata(Eq(kTestPeerSid), Eq(config::kServiceUnitName), _, _))
            .WillOnce([&](auto, auto, auto out_sud, auto out_md) {
                *out_sud = kTestPeerServiceUnitDefinition;
                *out_md = transport_metadata;
                return ndk::ScopedAStatus::ok();
            });

    std::promise<void> connect_promise;
    EXPECT_CALL(connection_manager_,
                Connect(Eq(peer_address), Eq(MakeFqinString(kTestPeerFqin)), _))
            .WillOnce([&](auto&, auto&, auto& eh) {
                event_handler_ = &eh;
                connect_promise.set_value();
            });

    listener_->onServiceUnitRegistered(kTestPeerServiceUnitDefinition);

    ASSERT_EQ(connect_promise.get_future().wait_for(std::chrono::seconds(1)),
              std::future_status::ready);
}

TEST_F(RemoteAgentClientManagerTest, onServiceUnitUnregistered_TerminatesClient) {
    StartupClientManager();
    CreatePendingClient();
    CompleteClientConnection();

    // Unreg will cause connection to get RequestDisconnect to shut down client.
    EXPECT_CALL(*connection_, RequestDisconnect()).WillOnce([&]() {
        connection_->EnqueueReceiveFailure();
    });

    // Unregister the peer service unit to request Client termination.
    listener_->onServiceUnitUnregistered(kTestPeerServiceUnitDefinition);

    // Ensure that client is no longer visible.
    EXPECT_FALSE(client_manager_.CurrentClientNames().contains(kTestPeerVmName));

    // Brief sleep to let terminate complete.
    std::this_thread::sleep_for(std::chrono::milliseconds(100));

    // Send another unregister event; this wouldn't normally happen (and we'll log a warning when it
    // does), but we do it here to trigger garbage collection so we can verify that terminating
    // clients get cleaned up.
    listener_->onServiceUnitUnregistered(kTestPeerServiceUnitDefinition);

    // Check garbage collection worked.
    EXPECT_EQ(client_manager_.TerminatingClientCount(), 0);
}

TEST_F(RemoteAgentClientManagerTest, FetchLastMessage_FailsBeforeStartup) {
    auto fetch_result = client_manager_.FetchLastMessage(kTestPeerVmName, kTestTopicName);
    ASSERT_EQ(fetch_result.wait_for(std::chrono::seconds(0)), std::future_status::ready);
    EXPECT_FALSE(fetch_result.get().has_value());
}

TEST_F(RemoteAgentClientManagerTest, FetchLastMessage_FailsWhenClientPending) {
    StartupClientManager();
    CreatePendingClient();

    auto fetch_result = client_manager_.FetchLastMessage(kTestPeerVmName, kTestTopicName);
    ASSERT_EQ(fetch_result.wait_for(std::chrono::seconds(0)), std::future_status::ready);
    EXPECT_FALSE(fetch_result.get().has_value());
}

TEST_F(RemoteAgentClientManagerTest, FetchLastMessage_FailsWhenUnknownClient) {
    StartupClientManager();

    auto fetch_result = client_manager_.FetchLastMessage("wacky-vm-name", kTestTopicName);
    ASSERT_EQ(fetch_result.wait_for(std::chrono::seconds(0)), std::future_status::ready);
    EXPECT_FALSE(fetch_result.get().has_value());
}

TEST_F(RemoteAgentClientManagerTest, FetchLastMessage_SucceedsAndSendsRequestIfConnected) {
    StartupClientManager();
    CreatePendingClient();
    CompleteClientConnection();

    auto fetch_result = client_manager_.FetchLastMessage(kTestPeerVmName, kTestTopicName);
    EXPECT_EQ(fetch_result.wait_for(std::chrono::seconds(0)), std::future_status::timeout);

    auto payload = connection_->GetNextSentDataIfPayload<Payload_LastMessageRequest>();
    ASSERT_TRUE(payload);
    EXPECT_EQ(payload->topic_name, kTestTopicName);
}

TEST_F(RemoteAgentClientManagerTest, RequestSubscribe_FailsBeforeStartup) {
    CreateRemoteTestTopic();

    EXPECT_FALSE(client_manager_.RequestSubscribe(kTestPeerVmName, kTestTopicName).isOk());
}

TEST_F(RemoteAgentClientManagerTest, RequestSubscribe_SucceedsWhenClientPending) {
    StartupClientManager();
    CreateRemoteTestTopic();
    CreatePendingClient();

    EXPECT_TRUE(client_manager_.RequestSubscribe(kTestPeerVmName, kTestTopicName).isOk());

    // No message could have been sent since connect is pending.
}

TEST_F(RemoteAgentClientManagerTest,
       RequestSubscribe_RoutesToCorrectClientAndSendsRequestIfConnected) {
    StartupClientManager();
    CreateRemoteTestTopic();
    CreatePendingClient();
    CompleteClientConnection();

    EXPECT_TRUE(client_manager_.RequestSubscribe(kTestPeerVmName, kTestTopicName).isOk());

    auto payload = connection_->GetNextSentDataIfPayload<Payload_SubscribeRequest>();
    ASSERT_TRUE(payload);
    EXPECT_EQ(payload->topic_name, kTestTopicName);
}

TEST_F(RemoteAgentClientManagerTest, RequestSubscribe_QueuesAndLaterProcessesRequestToUnknownPeer) {
    StartupClientManager();
    CreateRemoteTestTopic();

    EXPECT_TRUE(client_manager_.RequestSubscribe(kTestPeerVmName, kTestTopicName).isOk());

    CreatePendingClient();
    CompleteClientConnection();

    auto payload = connection_->GetNextSentDataIfPayload<Payload_SubscribeRequest>();
    ASSERT_TRUE(payload);
    EXPECT_EQ(payload->topic_name, kTestTopicName);
}

TEST_F(RemoteAgentClientManagerTest,
       RequestSubscribe_UnregistersPendingSubscriptionTopicsOnShutdown) {
    StartupClientManager();
    CreateRemoteTestTopic();

    EXPECT_TRUE(client_manager_.RequestSubscribe(kTestPeerVmName, kTestTopicName).isOk());

    auto topic = topic_manager_.FindTopic(kTestTopicName, /*local_only=*/false);
    ASSERT_TRUE(topic);
    EXPECT_TRUE(topic->IsActive());
    client_manager_.Shutdown();
    EXPECT_FALSE(topic->IsActive());
}

TEST_F(RemoteAgentClientManagerTest, DumpStats) {
    StartupClientManager();

    // No clients
    std::stringstream no_clients_output;
    client_manager_.DumpStats(no_clients_output);
    EXPECT_EQ(no_clients_output.str(), "\nOutbound Remote Agent Connections: NONE\n");

    // Set up a client with no connection yet.
    CreatePendingClient();

    // Unconnected client
    std::stringstream unconnected_client_output;
    client_manager_.DumpStats(unconnected_client_output);
    EXPECT_EQ(unconnected_client_output.str(),
              "\n"
              "Outbound Remote Agent Connections:\n"
              "\tvm_name,connection,bytes_sent,bytes_received\n"
              "\tpeer-vm-name,<NONE>,0,0\n");

    CompleteClientConnection();

    EXPECT_CALL(*connection_, BytesSent()).WillOnce(Return(13));
    EXPECT_CALL(*connection_, BytesReceived()).WillOnce(Return(15));

    std::stringstream connected_client_output;
    client_manager_.DumpStats(connected_client_output);
    EXPECT_THAT(connected_client_output.str(),
                ::testing::MatchesRegex("\n"
                                        "Outbound Remote Agent Connections:\n"
                                        "\tvm_name,connection,bytes_sent,bytes_received\n"
                                        "\tpeer-vm-name,FakeConnection\\(.*\\),13,15\n"));
}

}  // namespace
}  // namespace android::sdv::dt::agent::test