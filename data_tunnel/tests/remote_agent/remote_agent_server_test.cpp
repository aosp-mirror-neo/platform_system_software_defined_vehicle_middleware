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

#include "remote_agent/remote_agent_server.h"

#include "config.h"
#include "connection.h"
#include "mock_agents.h"
#include "proto/dt_proto.h"
#include "remote_agent/remote_agent_test_utils.h"
#include "remote_agent_test_utils.h"
#include "topic_manager.h"

#include <gtest/gtest.h>

#include <memory>

using ::testing::_;
using ::testing::Address;
using ::testing::AtLeast;
using ::testing::DoDefault;
using ::testing::Eq;
using ::testing::Mock;
using ::testing::Return;
using ::testing::StrictMock;

namespace android::sdv::dt::agent::test {

using ::aidl::google::sdv::identity::ServiceFqin;
using ::aidl::google::sdv::service_discovery::common::RegistrationToken;
using ::aidl::google::sdv::service_discovery::common::UnitType;
using ::aidl::google::sdv::service_discovery::common::metadata::TransportMetadata;

namespace {

static const char* kUnitName = "dt-agent-topic-server";
static const UnitType kDtAgentUnitType = UnitType{/*sdvPackageName*/ config::kPackageName,
                                                  /*serviceBundleName*/ config::kServiceBundleName,
                                                  /*typeName*/ "DataTunnelAgentUnitType"};
static const char* kTestVmName = "testvm1";
static const char* kSecondTestVmName = "testvm2";
static const char* kTestTopicName = "test-topic-name";

class RemoteAgentServerTest : public ::testing::Test {
protected:
    RemoteAgentServerTest() :
          service_registration_agent_(ndk::SharedRefBase::make<MockServiceRegistrationAgent>()),
          transport_support_agent_(ndk::SharedRefBase::make<MockTransportSupportAgent>()),
          server_(kUnitName, handshaker_, topic_manager_) {}

    ~RemoteAgentServerTest() {
        if (server_started_) {
            EXPECT_CALL(*service_registration_agent_, unregisterServiceUnit(_))
                    .WillOnce(Return(ndk::ScopedAStatus::ok()));
            server_.Shutdown();
        }
    }

    void StartupServer() {
        EXPECT_CALL(*service_registration_agent_, registerServiceUnit(_, _, _, _, _))
                .WillOnce(Return(ndk::ScopedAStatus::ok()));

        EXPECT_CALL(connection_manager_, Listen(_)).WillOnce([&](auto& eh) {
            event_handler_ = &eh;
            return true;
        });

        TransportMetadata expected_transport_metadata{};
        auto listen_address = SocketAddressV4(connection_manager_.GetLocalIp(),
                                              connection_manager_.GetListenPort());
        listen_address.Serialize(expected_transport_metadata.value_holder);
        EXPECT_CALL(*transport_support_agent_, setAgentMetadata(_, _))
                .WillOnce(Return(ndk::ScopedAStatus::ok()));

        ASSERT_TRUE(server_.Startup(connection_manager_, service_registration_agent_,
                                    transport_support_agent_));

        Mock::VerifyAndClearExpectations(&service_registration_agent_);
        Mock::VerifyAndClearExpectations(&connection_manager_);
        Mock::VerifyAndClearExpectations(&transport_support_agent_);

        server_started_ = true;
    }

    MockConnectionManager connection_manager_;
    ConnectionManager::EventHandler* event_handler_;
    FakeConnection* connection_ = nullptr;
    StrictMock<MockHandshaker> handshaker_;
    TopicManager topic_manager_;
    std::shared_ptr<MockServiceRegistrationAgent> service_registration_agent_;
    std::shared_ptr<MockTransportSupportAgent> transport_support_agent_;
    RemoteAgentServer server_;
    bool server_started_;
};

TEST_F(RemoteAgentServerTest, Startup_And_Shutdown_Success) {
    // Registers service unit
    RegistrationToken fake_registration_token;
    fake_registration_token.identity.name.sdvVmName = kTestVmName;
    EXPECT_CALL(*service_registration_agent_,
                registerServiceUnit(Eq(kUnitName), Eq(kDtAgentUnitType), _, _, _))
            .WillOnce([&](auto, auto, auto, auto, RegistrationToken* rt) {
                *rt = fake_registration_token;
                return ndk::ScopedAStatus::ok();
            });

    // Starts connection manager listening
    EXPECT_CALL(connection_manager_, Listen(_)).WillOnce([&](auto& eh) {
        event_handler_ = &eh;
        return true;
    });

    // Registers transport data using registration token and listen address from connection manager.
    TransportMetadata expected_transport_metadata{};
    auto listen_address =
            SocketAddressV4(connection_manager_.GetLocalIp(), connection_manager_.GetListenPort());
    listen_address.Serialize(expected_transport_metadata.value_holder);
    EXPECT_CALL(*transport_support_agent_,
                setAgentMetadata(Eq(fake_registration_token), Eq(expected_transport_metadata)))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));

    // Do Startup and check it was successful.
    ASSERT_TRUE(server_.Startup(connection_manager_, service_registration_agent_,
                                transport_support_agent_));

    // Destruction will unregister service unit.
    EXPECT_CALL(*service_registration_agent_, unregisterServiceUnit(Eq(kUnitName)))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));

    server_.Shutdown();
}

TEST_F(RemoteAgentServerTest, Startup_FailsOnServiceUnitRegistrationFailure) {
    RegistrationToken fake_registration_token;
    fake_registration_token.identity.name.sdvVmName = kTestVmName;
    EXPECT_CALL(*service_registration_agent_,
                registerServiceUnit(Eq(kUnitName), Eq(kDtAgentUnitType), _, _, _))
            .WillOnce(Return(ndk::ScopedAStatus::fromExceptionCode(EX_ILLEGAL_ARGUMENT)));

    ASSERT_FALSE(server_.Startup(connection_manager_, service_registration_agent_,
                                 transport_support_agent_));
}

TEST_F(RemoteAgentServerTest, Startup_FailsOnListenFailure) {
    // Registers service unit
    RegistrationToken fake_registration_token;
    fake_registration_token.identity.name.sdvVmName = kTestVmName;
    EXPECT_CALL(*service_registration_agent_,
                registerServiceUnit(Eq(kUnitName), Eq(kDtAgentUnitType), _, _, _))
            .WillOnce([&](auto, auto, auto, auto, RegistrationToken* rt) {
                *rt = fake_registration_token;
                return ndk::ScopedAStatus::ok();
            });

    // Tries to start connection manager listening.
    EXPECT_CALL(connection_manager_, Listen(_)).WillOnce(Return(false));

    // We'll also see an unregisterServiceUnit call to undo the successful registration.
    EXPECT_CALL(*service_registration_agent_, unregisterServiceUnit(_))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));

    // Do Startup and check that it fails.
    ASSERT_FALSE(server_.Startup(connection_manager_, service_registration_agent_,
                                 transport_support_agent_));

    server_.Shutdown();
}

TEST_F(RemoteAgentServerTest, Startup_FailsOnTransportRegistrationFailure) {
    // Register service unit succeeds.
    EXPECT_CALL(*service_registration_agent_, registerServiceUnit(_, _, _, _, _))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));

    // Listen starts.
    EXPECT_CALL(connection_manager_, Listen(_)).WillOnce([&](auto& eh) {
        event_handler_ = &eh;
        return true;
    });

    // Metadata registration fails.
    EXPECT_CALL(*transport_support_agent_, setAgentMetadata(_, _))
            .WillOnce(Return(ndk::ScopedAStatus::fromExceptionCode(EX_ILLEGAL_ARGUMENT)));

    // We'll also see an unregisterServiceUnit call to undo the successful registration.
    EXPECT_CALL(*service_registration_agent_, unregisterServiceUnit(_))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));

    ASSERT_FALSE(server_.Startup(connection_manager_, service_registration_agent_,
                                 transport_support_agent_));
}

TEST_F(RemoteAgentServerTest, AcceptsConnectionAndSpawnsSession) {
    StartupServer();

    // Promise to make handhake block and wait on main thread so we have a chance to inspect
    // server at that point.
    std::promise<void> handshake_complete_promise;
    EXPECT_CALL(handshaker_, PerformHandshake(_, Eq(std::nullopt))).WillOnce([&](auto&, auto) {
        handshake_complete_promise.get_future().get();
        return std::optional<std::string>(kTestVmName);
    });

    event_handler_->OnConnectionManagerEvent(std::make_unique<FakeConnection>());

    WaitForConditionOrTimeout([&]() { return server_.GetTotalSessionCount() == 1; });
    EXPECT_TRUE(server_.GetPairedClientVmNames().empty());
    EXPECT_EQ(server_.GetTotalSessionCount(), 1);
    handshake_complete_promise.set_value();

    WaitForConditionOrTimeout([&]() { return !server_.GetPairedClientVmNames().empty(); });
    auto client_vm_names = server_.GetPairedClientVmNames();
    EXPECT_EQ(client_vm_names.size(), 1);
    EXPECT_TRUE(client_vm_names.contains(kTestVmName));
}

TEST_F(RemoteAgentServerTest, ReplacesExistingSessionForVmWithNew) {
    StartupServer();

    auto first_connection_owned = std::make_unique<FakeConnection>();
    auto* first_connection = first_connection_owned.get();

    EXPECT_CALL(handshaker_, PerformHandshake(_, Eq(std::nullopt))).WillOnce(Return(kTestVmName));

    event_handler_->OnConnectionManagerEvent(std::move(first_connection_owned));

    // Wait for session to show up in list.
    ASSERT_TRUE(WaitForConditionOrTimeout(
            [&]() { return server_.GetPairedClientVmNames().contains(kTestVmName); }));

    // Promise to make second connection thread wait on main thread during handshake so main
    // can wait on timeout.
    std::promise<void> handshake_complete_promise;
    EXPECT_CALL(handshaker_, PerformHandshake(_, Eq(std::nullopt))).WillOnce([&](auto&, auto) {
        handshake_complete_promise.get_future().get();
        return std::optional<std::string>(kTestVmName);
    });

    auto second_connection_owned = std::make_unique<FakeConnection>();
    auto* second_connection = second_connection_owned.get();
    event_handler_->OnConnectionManagerEvent(std::move(second_connection_owned));

    // Wait until second session shows up.
    EXPECT_TRUE(server_.GetPairedClientVmNames().contains(kTestVmName));
    EXPECT_EQ(server_.GetTotalSessionCount(), 2);

    // Expect first connection to call disconnect after we let the connection thread proceed.
    EXPECT_CALL(*first_connection, RequestDisconnect()).Times(AtLeast(1));
    handshake_complete_promise.set_value();

    // Brief sleep to let first_connection thread call Disconnect.
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
    // Need to check expectation on first connection at this point because server_ destructor can
    // call Disconnect on first_connection, since it might not get garbage collected before the
    // destructor runs.  This also clears the expectation so that any call during the destructor
    // doesn't oversaturate.
    Mock::VerifyAndClearExpectations(first_connection);

    // Connection will get disconnect request on shutdown.
    EXPECT_CALL(*second_connection, RequestDisconnect()).Times(AtLeast(1));
}

TEST_F(RemoteAgentServerTest, CleansUpTerminatedSessions_OnNewConnection) {
    StartupServer();
    auto first_connection_owned = std::make_unique<FakeConnection>();
    auto* first_connection = first_connection_owned.get();

    // First connection handshake.
    EXPECT_CALL(handshaker_,
                PerformHandshake(Address(static_cast<IConnection*>(first_connection)),
                                 Eq(std::nullopt)))
            .WillOnce(Return(kTestVmName));
    event_handler_->OnConnectionManagerEvent(std::move(first_connection_owned));

    // Fake failure of first connection and sleep briefly to give it a chance to process.
    first_connection->EnqueueReceiveFailure();
    std::this_thread::sleep_for(std::chrono::milliseconds(100));

    // Wait for termination to complete, but server still has the record.
    EXPECT_TRUE(server_.GetPairedClientVmNames().contains(kTestVmName));
    EXPECT_EQ(server_.GetTotalSessionCount(), 1);

    auto second_connection_owned = std::make_unique<FakeConnection>();
    auto* second_connection = second_connection_owned.get();

    // Get a new connection for a different VM; handshake will block and wait for main
    // thread so we can inspect connections present at that point, after OnConnectionManagerEvent
    // has run but before OnHandshakeSuccess does.
    std::promise<void> handshake_block;
    EXPECT_CALL(handshaker_,
                PerformHandshake(Address(static_cast<IConnection*>(second_connection)),
                                 Eq(std::nullopt)))
            .WillOnce([&](auto&, auto) {
                handshake_block.get_future().get();
                return kSecondTestVmName;
            });
    event_handler_->OnConnectionManagerEvent(std::move(second_connection_owned));

    // Give new connection thread a bit of time start handshake, then check that only new
    // (not-yet-paired) session present.
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
    EXPECT_TRUE(server_.GetPairedClientVmNames().empty());
    EXPECT_EQ(server_.GetTotalSessionCount(), 1);

    // Let new connection thread resume and give it a bit of time to complete handshake.
    handshake_block.set_value();
    std::this_thread::sleep_for(std::chrono::milliseconds(100));

    // Check that now only second session is present.
    EXPECT_TRUE(server_.GetPairedClientVmNames().contains(kSecondTestVmName));
    EXPECT_EQ(server_.GetTotalSessionCount(), 1);
}

TEST_F(RemoteAgentServerTest, CleansUpTerminatedSessions_OnHandshakeSuccess) {
    StartupServer();
    auto first_connection_owned = std::make_unique<FakeConnection>();
    auto* first_connection = first_connection_owned.get();

    // First connection handshake.
    EXPECT_CALL(handshaker_,
                PerformHandshake(Address(static_cast<IConnection*>(first_connection)),
                                 Eq(std::nullopt)))
            .WillOnce(Return(kTestVmName));
    event_handler_->OnConnectionManagerEvent(std::move(first_connection_owned));

    auto second_connection_owned = std::make_unique<FakeConnection>();
    auto* second_connection = second_connection_owned.get();

    // Get a new connection for a different VM; handshake will block and wait for main
    // thread so we can inspect connections present at that point, after OnConnectionManagerEvent
    // has run but before OnHandshakeSuccess does.
    std::promise<void> handshake_block;
    EXPECT_CALL(handshaker_,
                PerformHandshake(Address(static_cast<IConnection*>(second_connection)),
                                 Eq(std::nullopt)))
            .WillOnce([&](auto&, auto) {
                handshake_block.get_future().get();
                return kSecondTestVmName;
            });
    event_handler_->OnConnectionManagerEvent(std::move(second_connection_owned));

    // Give new connection thread a bit of time start handshake, then check that both sessions
    // present but only the first is paired (so far).
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
    EXPECT_TRUE(server_.GetPairedClientVmNames().contains(kTestVmName));
    EXPECT_EQ(server_.GetPairedClientVmNames().size(), 1);
    EXPECT_EQ(server_.GetTotalSessionCount(), 2);

    // Fake failure of first connection and give its connection thread time to exit.
    first_connection->EnqueueReceiveFailure();
    std::this_thread::sleep_for(std::chrono::milliseconds(100));

    // Let new connection thread resume and give it a bit of time to complete handshake and
    // call OnHandhakeSuccess
    handshake_block.set_value();
    std::this_thread::sleep_for(std::chrono::milliseconds(100));

    // Check that now only second session is present.
    EXPECT_TRUE(server_.GetPairedClientVmNames().contains(kSecondTestVmName));
    EXPECT_EQ(server_.GetTotalSessionCount(), 1);
}

TEST_F(RemoteAgentServerTest, OnHandshakeSuccess_IgnoresNullSessionPtr) {
    StartupServer();

    server_.OnHandshakeSuccess(nullptr, kTestVmName);

    // Check that we didn't do anything with the session.
    EXPECT_EQ(server_.GetTotalSessionCount(), 0);
}

TEST_F(RemoteAgentServerTest, OnHandshakeSuccess_TerminatesUnknownSession) {
    StartupServer();

    auto connection = std::make_unique<FakeConnection>();
    auto connection_ptr = connection.get();

    // Session will call handshaker on Create and handshake must succeed.
    EXPECT_CALL(handshaker_,
                PerformHandshake(Address(static_cast<IConnection*>(connection.get())),
                                 Eq(std::nullopt)))
            .WillOnce(Return(kTestVmName));

    // Create session that will fire OnHandshakeSuccess for a server that doesn't know about it.
    auto session = std::make_unique<RemoteAgentServerSession>(
            std::move(connection), handshaker_,
            [&](auto* session, auto& peer_vm_name) {
                server_.OnHandshakeSuccess(session, peer_vm_name);
            },
            topic_manager_);

    // Wait a bit for session to reach terminated state.
    WaitForConditionOrTimeout([&]() { return session->IsTerminated(); });
    ASSERT_TRUE(session->IsTerminated());

    // Check that we didn't do anything with the session.
    EXPECT_EQ(server_.GetTotalSessionCount(), 0);
}

TEST_F(RemoteAgentServerTest, DumpStats) {
    StartupServer();

    // No sessions
    std::stringstream no_sessions_output;
    server_.DumpStats(no_sessions_output);
    EXPECT_EQ(no_sessions_output.str(), "\nInbound Remote Agent Connections: NONE\n");

    auto connection = std::make_unique<FakeConnection>();
    EXPECT_CALL(*connection, BytesSent()).WillOnce(Return(13)).WillOnce(Return(14));
    EXPECT_CALL(*connection, BytesReceived()).WillOnce(Return(15)).WillOnce(Return(16));

    // Promise to make handshake block and wait on main thread so we have a chance to inspect
    // server at that point.
    std::promise<void> handshake_complete_promise;
    EXPECT_CALL(handshaker_, PerformHandshake(_, Eq(std::nullopt))).WillOnce([&](auto&, auto) {
        handshake_complete_promise.get_future().get();
        return std::optional<std::string>(kTestVmName);
    });

    event_handler_->OnConnectionManagerEvent(std::move(connection));
    ASSERT_TRUE(WaitForConditionOrTimeout([&]() { return server_.GetTotalSessionCount() == 1; }));

    // Check output with unpaired session.
    std::stringstream unpaired_session_output;
    server_.DumpStats(unpaired_session_output);
    EXPECT_THAT(unpaired_session_output.str(),
                ::testing::MatchesRegex("\n"
                                        "Inbound Remote Agent Connections:\n"
                                        "\tvm_name,connection,bytes_sent,bytes_received\n"
                                        "\t<UNKNOWN>,FakeConnection\\(.*\\),13,15\n"));

    // Allow pairing to complete.
    handshake_complete_promise.set_value();
    ASSERT_TRUE(
            WaitForConditionOrTimeout([&]() { return !server_.GetPairedClientVmNames().empty(); }));

    // Check output with paired session.
    std::stringstream paired_session_output;
    server_.DumpStats(paired_session_output);
    EXPECT_THAT(paired_session_output.str(),
                ::testing::MatchesRegex("\n"
                                        "Inbound Remote Agent Connections:\n"
                                        "\tvm_name,connection,bytes_sent,bytes_received\n"
                                        "\ttestvm1,FakeConnection\\(.*\\),14,16\n"));
}

}  // namespace

}  // namespace android::sdv::dt::agent::test
