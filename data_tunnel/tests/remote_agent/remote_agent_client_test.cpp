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

#include "remote_agent/remote_agent_client.h"

#include "connection.h"
#include "proto/dt_proto.h"
#include "remote_agent/remote_agent_test_utils.h"
#include "remote_agent_test_utils.h"
#include "topic_manager.h"

#include <android-base/logging.h>
#include <gtest/gtest.h>

#include <initializer_list>

using ::testing::_;
using ::testing::Address;
using ::testing::Eq;
using ::testing::Mock;
using ::testing::Return;
using ::testing::StrictMock;

namespace android::sdv::dt::agent::test {

using proto::Payload_LastMessageRequest;
using proto::Payload_LastMessageResult;
using proto::Payload_SubscribeRequest;
using proto::Payload_SubscribeResult;
using proto::Payload_TopicDestroyed;
using proto::Payload_TopicRawData;

namespace {

static const SocketAddressV4 kTestServerAddress = SocketAddressV4(in_addr{0x1234}, 5760);

static const char* kTestVmName = "testvm2";
static const char* kTestTopicName = "test-topic-name";

using TestMessageType = uint64_t;
static const size_t kTestMessageSize = sizeof(TestMessageType);
static const size_t kTestMessageCount = 4;

// For testing handling of messages bigger than default buffer sizes.
struct BigMessageType {
    uint8_t data[32768];
};
static constexpr size_t kBigMessageSize = sizeof(BigMessageType);

static const ::aidl::google::sdv::identity::ServiceIdentity kTestServiceIdentity =
        ::aidl::google::sdv::identity::ServiceIdentity{
                .name = ::aidl::google::sdv::identity::ServiceFqin{
                        /*sdvVmName*/ kTestVmName,
                        /*sdvPackageName*/ "com.legoog.sdv",
                        /*serviceBundleName*/ "DtAgent",
                        /*serviceInstanceName*/ "default",
                }};

static const char* kTestServerFqinString = "testvm2:com.legoog.sdv.DtAgent/default";

class RemoteAgentClientTest : public ::testing::Test {
public:
    ~RemoteAgentClientTest() {
        if (!client_) {
            return;
        }
        CHECK(connect_event_handler_ != nullptr);
        client_->RequestTerminate();
        // Fake a failed connect in case the client is waiting for a Connect() result.
        connect_event_handler_->OnConnectionManagerEvent(
                base::unexpected(::sdv::SdvStatus(::sdv::SdvStatusCode::kInternal, "")));
        client_.reset();
    }

    void CreateTestTopic(bool is_remote, size_t message_size = kTestMessageSize) {
        Topic::Metadata metadata(kTestTopicName, message_size, kTestMessageCount);
        auto mq = std::make_unique<AidlMQ>(message_size * kTestMessageCount);
        topic_ = topic_manager_.RegisterTopic(metadata, /* client_id = */ 0, is_remote,
                                              std::move(mq));
        ASSERT_TRUE(topic_);
        auto reader = topic_->CreateReader();
        ASSERT_TRUE(reader);
        topic_reader_.emplace(std::move(*reader));
    }

    void EnqueueTestTopicDataForReceive(const uint8_t* data, size_t bytes) {
        ASSERT_NE(connection_, nullptr);
        ASSERT_NE(data, nullptr);
        ASSERT_NE(bytes, 0);

        Payload_TopicRawData payload(bytes);
        payload.topic_name_ = kTestTopicName;
        memcpy(payload.buffer_.Ptr(), data, bytes);
        connection_->EnqueueParcelForReceive(777, payload);
    }

    void EnqueueTestTopicMessagesForReceive(std::initializer_list<TestMessageType> messages) {
        EnqueueTestTopicDataForReceive(reinterpret_cast<const uint8_t*>(std::data(messages)),
                                       kTestMessageSize * messages.size());
    }

    void PrepareClientAwaitingConnectResult() {
        std::promise<void> connect_promise;
        EXPECT_CALL(connection_manager_,
                    Connect(Eq(kTestServerAddress), Eq(kTestServerFqinString), _))
                .WillOnce([&](const auto&, const auto&, auto& event_handler) {
                    connect_event_handler_ = &event_handler;
                    connect_promise.set_value();
                });

        client_.emplace(kTestServiceIdentity, kTestServerAddress, connection_manager_, handshaker_,
                        topic_manager_);

        ASSERT_EQ(connect_promise.get_future().wait_for(std::chrono::seconds(2)),
                  std::future_status::ready);

        Mock::VerifyAndClearExpectations(&connection_manager_);
    }

    void CompleteConnection() {
        auto owned_connection = std::make_unique<FakeConnection>();
        connection_ = owned_connection.get();

        std::promise<void> handshake_promise;
        EXPECT_CALL(handshaker_,
                    PerformHandshake(Address(static_cast<IConnection*>(connection_)),
                                     Eq(kTestVmName)))
                .WillOnce([&](auto&, auto) {
                    handshake_promise.set_value();
                    return kTestVmName;
                });

        connect_event_handler_->OnConnectionManagerEvent(std::move(owned_connection));

        ASSERT_EQ(handshake_promise.get_future().wait_for(std::chrono::seconds(2)),
                  std::future_status::ready);

        // Brief sleep since actual transition to Active happens after handshake.
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }

    void PrepareClientWithActiveConnection() {
        PrepareClientAwaitingConnectResult();
        CompleteConnection();
    }

    void PrepareClientWithPendingLastMessageRequest() {
        PrepareClientWithActiveConnection();

        pending_last_message_result_ = client_->FetchLastMessage(kTestTopicName);
        ASSERT_TRUE(pending_last_message_result_.valid());
        ASSERT_EQ(pending_last_message_result_.wait_for(std::chrono::seconds(0)),
                  std::future_status::timeout);

        auto payload = connection_->GetNextSentDataIfPayload<Payload_LastMessageRequest>();
        ASSERT_TRUE(payload);
        ASSERT_EQ(payload->topic_name_, kTestTopicName);
    }

    void PrepareClientWithPendingSubscribeRequest(size_t message_size = kTestMessageSize) {
        PrepareClientWithActiveConnection();
        CreateTestTopic(/*is_remote=*/true, message_size);

        // Subscribe returns success
        auto subscribe_result = client_->RequestSubscribe(kTestTopicName);
        ASSERT_TRUE(subscribe_result.isOk());

        auto payload = connection_->GetNextSentDataIfPayload<Payload_SubscribeRequest>();
        ASSERT_TRUE(payload);
        ASSERT_EQ(payload->topic_name_, kTestTopicName);
    }

    void EnqueueSubscribeResultOkForReceive() {
        Payload_SubscribeResult subscribe_result;
        subscribe_result.topic_name_ = kTestTopicName;
        subscribe_result.result_ = Payload_SubscribeResult::Result_Ok;
        subscribe_result.message_size_ = topic_->GetMetadata().message_size;
        subscribe_result.message_count_ = topic_->GetMetadata().message_count;
        connection_->EnqueueParcelForReceive(242, subscribe_result);
    }

    void PrepareClientWithActiveSubscription(size_t message_size = kTestMessageSize) {
        PrepareClientWithPendingSubscribeRequest(message_size);
        EnqueueSubscribeResultOkForReceive();

        // Give ConnectionThread a chance to work.
        WaitForConditionOrTimeout([&]() {
            auto subs = client_->Subscriptions();
            return subs.contains(kTestTopicName) && subs.at(kTestTopicName).is_active;
        });

        // Topic active
        ASSERT_TRUE(topic_manager_.FindTopic(kTestTopicName, /*local_only=*/false));
        ASSERT_TRUE(client_->Subscriptions().at(kTestTopicName).is_active);
    }

protected:
    MockConnectionManager connection_manager_;
    FakeConnection* connection_;
    ConnectionManager::EventHandler* connect_event_handler_ = nullptr;
    StrictMock<MockHandshaker> handshaker_;
    TopicManager topic_manager_;
    Topic* topic_;
    std::optional<Topic::Reader> topic_reader_;
    std::optional<RemoteAgentClient> client_;
    RemoteAgentClient::LastMessageResultFuture pending_last_message_result_;
};

TEST_F(RemoteAgentClientTest, Construction_InitiatesConnect) {
    std::promise<void> connect_promise;
    EXPECT_CALL(connection_manager_, Connect(Eq(kTestServerAddress), Eq(kTestServerFqinString), _))
            .WillOnce([&](const auto&, const auto&, auto& event_handler) {
                connect_event_handler_ = &event_handler;
                connect_promise.set_value();
            });

    client_.emplace(kTestServiceIdentity, kTestServerAddress, connection_manager_, handshaker_,
                    topic_manager_);

    ASSERT_EQ(connect_promise.get_future().wait_for(std::chrono::seconds(2)),
              std::future_status::ready);
}

TEST_F(RemoteAgentClientTest, ConnectionThread_RetriesOnConnectFailure) {
    PrepareClientAwaitingConnectResult();

    std::promise<void> connect_promise;
    EXPECT_CALL(connection_manager_,
                Connect(Eq(kTestServerAddress), Eq(kTestServerFqinString),
                        Address(connect_event_handler_)))
            .WillOnce([&](const auto&, const auto&, auto&) { connect_promise.set_value(); });

    connect_event_handler_->OnConnectionManagerEvent(
            base::unexpected(::sdv::SdvStatus(::sdv::SdvStatusCode::kInternal, "failure")));

    ASSERT_EQ(connect_promise.get_future().wait_for(std::chrono::seconds(4)),
              std::future_status::ready);
}

TEST_F(RemoteAgentClientTest, ConnectionThread_RetriesOnHandshakeFailure) {
    PrepareClientAwaitingConnectResult();

    auto owned_connection = std::make_unique<FakeConnection>();
    connection_ = owned_connection.get();

    std::promise<void> second_connect_promise;
    {
        ::testing::InSequence s;

        EXPECT_CALL(handshaker_,
                    PerformHandshake(Address(static_cast<IConnection*>(connection_)),
                                     Eq(kTestVmName)))
                .WillOnce(Return(std::nullopt));

        EXPECT_CALL(connection_manager_,
                    Connect(Eq(kTestServerAddress), Eq(kTestServerFqinString),
                            Address(connect_event_handler_)))
                .WillOnce([&](const auto&, const auto&, auto&) {
                    second_connect_promise.set_value();
                });
    }

    connect_event_handler_->OnConnectionManagerEvent(std::move(owned_connection));

    ASSERT_EQ(second_connect_promise.get_future().wait_for(std::chrono::seconds(4)),
              std::future_status::ready);
}

TEST_F(RemoteAgentClientTest, ConnectionThread_ReconnectsAfterFailure) {
    PrepareClientWithActiveConnection();

    std::promise<void> reconnect_promise;
    EXPECT_CALL(connection_manager_,
                Connect(Eq(kTestServerAddress), Eq(kTestServerFqinString),
                        Address(connect_event_handler_)))
            .WillOnce([&](const auto&, const auto&, auto&) { reconnect_promise.set_value(); });

    // Simulate connection failure.
    connection_->Disconnect();

    ASSERT_EQ(reconnect_promise.get_future().wait_for(std::chrono::seconds(4)),
              std::future_status::ready);
}

TEST_F(RemoteAgentClientTest, FetchLastMessage_SendsRequestAndReturnsUnresolvedFuture) {
    PrepareClientWithActiveConnection();

    // Make request.
    auto result_future = client_->FetchLastMessage(kTestTopicName);

    // Message was sent to peer.
    auto payload = connection_->GetNextSentDataIfPayload<Payload_LastMessageRequest>();
    ASSERT_TRUE(payload);
    EXPECT_EQ(payload->topic_name_, kTestTopicName);

    // Future not already resolved
    EXPECT_EQ(result_future.wait_for(std::chrono::seconds(0)), std::future_status::timeout);
}

TEST_F(RemoteAgentClientTest, FetchLastMessage_ReturnsReadyFailureWhenConnectionInactive) {
    PrepareClientAwaitingConnectResult();

    auto result_future = client_->FetchLastMessage(kTestTopicName);

    // Returned future already ready with failure.
    EXPECT_EQ(result_future.wait_for(std::chrono::seconds(0)), std::future_status::ready);
    EXPECT_FALSE(result_future.get().has_value());
}

TEST_F(RemoteAgentClientTest, FetchLastMessage_ReturnsReadyFailureWhenSendPayloadFails) {
    PrepareClientWithActiveConnection();

    // Connection will simulate send failure
    connection_->SetSendDataReturnValue(false);

    // Make request
    auto result_future = client_->FetchLastMessage(kTestTopicName);

    // Returned future already ready with failure.
    EXPECT_EQ(result_future.wait_for(std::chrono::seconds(0)), std::future_status::ready);
    EXPECT_FALSE(result_future.get().has_value());
}

TEST_F(RemoteAgentClientTest, FetchLastMessage_ResolvesFutureToFailureOnLostConnection) {
    PrepareClientWithPendingLastMessageRequest();

    // Simulate receive failure
    connection_->EnqueueReceiveFailure();

    // Future resolves to failure.
    ASSERT_EQ(pending_last_message_result_.wait_for(std::chrono::seconds(2)),
              std::future_status::ready);
    EXPECT_FALSE(pending_last_message_result_.get().has_value());
}

TEST_F(RemoteAgentClientTest, FetchLastMessage_ResolvesFutureToFailureOnFailureMessageFromPeer) {
    PrepareClientWithPendingLastMessageRequest();

    // Make client receive failure result.
    Payload_LastMessageResult payload;
    payload.topic_name_ = kTestTopicName;
    payload.result_ = Payload_LastMessageResult::Result_Failure;
    connection_->EnqueueParcelForReceive(124, payload);

    // Future resolves to failure.
    ASSERT_EQ(pending_last_message_result_.wait_for(std::chrono::seconds(2)),
              std::future_status::ready);
    EXPECT_FALSE(pending_last_message_result_.get().has_value());
}

TEST_F(RemoteAgentClientTest, FetchLastMessage_ResolvesFutureToEmptyBufferOnOkButEmptyResult) {
    PrepareClientWithPendingLastMessageRequest();

    // Make client receive OK result with empty buffer from peer.
    Payload_LastMessageResult payload;
    payload.topic_name_ = kTestTopicName;
    payload.result_ = Payload_LastMessageResult::Result_Ok;
    connection_->EnqueueParcelForReceive(124, payload);

    // Future resolved to empty buffer.
    ASSERT_EQ(pending_last_message_result_.wait_for(std::chrono::seconds(2)),
              std::future_status::ready);
    auto& last_message_result = pending_last_message_result_.get();
    ASSERT_TRUE(last_message_result.has_value());
    EXPECT_TRUE(last_message_result.value().empty());
}

TEST_F(RemoteAgentClientTest, FetchLastMessage_ResolvesFutureToPopulatedBufferOnPopulatedResult) {
    PrepareClientWithPendingLastMessageRequest();

    std::vector<uint8_t> fake_message(16, 0xA);

    // Client receives OK result with populated buffer from peer.
    Payload_LastMessageResult payload;
    payload.topic_name_ = kTestTopicName;
    payload.result_ = Payload_LastMessageResult::Result_Ok;
    payload.buffer_ = RawBuffer(fake_message);
    connection_->EnqueueParcelForReceive(124, payload);

    // Future resolved to vector whose contents match received buffer.
    ASSERT_EQ(pending_last_message_result_.wait_for(std::chrono::seconds(2)),
              std::future_status::ready);
    auto& last_message_result = pending_last_message_result_.get();
    ASSERT_TRUE(last_message_result.has_value());
    EXPECT_EQ(last_message_result.value(), fake_message);
}

TEST_F(RemoteAgentClientTest, FetchLastMessage_LeavesFutureUnresolvedOnUnrelatedResultFromPeer) {
    PrepareClientWithPendingLastMessageRequest();

    // Make client receive result for different topic from peer.
    Payload_LastMessageResult payload;
    payload.topic_name_ = "different-test-topic";
    payload.result_ = Payload_LastMessageResult::Result_Failure;
    connection_->EnqueueParcelForReceive(124, payload);

    // Future remains unresolved.
    EXPECT_EQ(pending_last_message_result_.wait_for(std::chrono::seconds(1)),
              std::future_status::timeout);
}

TEST_F(RemoteAgentClientTest, FetchLastMessage_SharesResultForDuplicateRequests) {
    PrepareClientWithPendingLastMessageRequest();

    // Make second request
    auto second_result_future = client_->FetchLastMessage(kTestTopicName);

    // Check that no additional request sent.
    EXPECT_EQ(connection_->RemainingSentDataCount(), 0);

    // Client receives OK result with empty buffer.
    Payload_LastMessageResult payload;
    payload.topic_name_ = kTestTopicName;
    payload.result_ = Payload_LastMessageResult::Result_Ok;
    connection_->EnqueueParcelForReceive(124, payload);

    // Both futures resolved.
    ASSERT_EQ(pending_last_message_result_.wait_for(std::chrono::seconds(1)),
              std::future_status::ready);
    EXPECT_EQ(second_result_future.wait_for(std::chrono::seconds(0)), std::future_status::ready);

    // Both futures reference the same result object.
    ASSERT_EQ(&pending_last_message_result_.get(), &second_result_future.get());
}

TEST_F(RemoteAgentClientTest,
       RequestSubscribe_SucceedsButWaitsForActiveConnectionBeforeSendingRequest) {
    PrepareClientAwaitingConnectResult();
    CreateTestTopic(/*is_remote=*/true);

    // Subscribe returns success
    auto subscribe_result = client_->RequestSubscribe(kTestTopicName);
    EXPECT_TRUE(subscribe_result.isOk());

    // Have connect complete.
    CompleteConnection();

    // SubscribeRequest sent on connect.
    auto payload = connection_->GetNextSentDataIfPayload<Payload_SubscribeRequest>();
    ASSERT_TRUE(payload);
    EXPECT_EQ(payload->topic_name_, kTestTopicName);
}

TEST_F(RemoteAgentClientTest,
       RequestSubscribe_SucceedsAndSendsRequestImmediatelyWhenConnectionActive) {
    PrepareClientWithActiveConnection();
    CreateTestTopic(/*is_remote=*/true);

    // Subscribe returns success
    auto subscribe_result = client_->RequestSubscribe(kTestTopicName);
    EXPECT_TRUE(subscribe_result.isOk());

    // Subscribe goes out immediately.
    auto payload = connection_->GetNextSentDataIfPayload<Payload_SubscribeRequest>();
    ASSERT_TRUE(payload);
    EXPECT_EQ(payload->topic_name_, kTestTopicName);
}

TEST_F(RemoteAgentClientTest, RequestSubscribe_SucceedsForDuplicateButDoesNotResend) {
    PrepareClientWithPendingSubscribeRequest();

    // Subscribe returns success
    auto subscribe_result = client_->RequestSubscribe(kTestTopicName);
    EXPECT_TRUE(subscribe_result.isOk());

    // Nothing sent to peer since request already pending.
    ASSERT_FALSE(connection_->GetNextSentData(/*timeout_seconds=*/1));
}

TEST_F(RemoteAgentClientTest, RequestSubscribe_FailsWhenInTerminatingState) {
    PrepareClientWithActiveConnection();
    CreateTestTopic(/*is_remote=*/true);

    // RequestTerminate will destroy *connection_, so we catch the Disconnect
    // and record whether any data was sent at that point.
    bool sent_any_data = false;
    EXPECT_CALL(*connection_, RequestDisconnect()).WillOnce([&]() {
        sent_any_data = !!connection_->GetNextSentData(/*timeout_seconds*/ 0);
        connection_->EnqueueReceiveFailure();
    });

    client_->RequestTerminate();

    // Subscribe returns failure
    auto subscribe_result = client_->RequestSubscribe(kTestTopicName);
    EXPECT_FALSE(subscribe_result.isOk());

    // No data sent to peer.
    EXPECT_FALSE(sent_any_data);
}

TEST_F(RemoteAgentClientTest, RequestSubscribe_FailsWhenTopicNotFound) {
    PrepareClientWithActiveConnection();

    // Subscribe returns failure when no topic was registered.
    auto subscribe_result = client_->RequestSubscribe(kTestTopicName);
    EXPECT_EQ(subscribe_result.getStatusCode(), ::sdv::SdvStatusCode::kNotFound);

    // No request sent to peer
    EXPECT_FALSE(connection_->GetNextSentData(/*timeout_seconds=*/1));
}

TEST_F(RemoteAgentClientTest, RequestSubscribe_FailsWhenTopicIsNotRemote) {
    PrepareClientWithActiveConnection();
    CreateTestTopic(/*is_remote=*/false);

    // Subscribe returns failure when no topic was registered.
    auto subscribe_result = client_->RequestSubscribe(kTestTopicName);
    EXPECT_EQ(subscribe_result.getStatusCode(), ::sdv::SdvStatusCode::kFailedPrecondition);

    // No request sent to peer
    EXPECT_FALSE(connection_->GetNextSentData(/*timeout_seconds=*/1));
}

TEST_F(RemoteAgentClientTest, RequestSubscribe_FailsForDuplicateWithDifferentTopic) {
    PrepareClientWithPendingSubscribeRequest();

    // Unregister and re-create to change the topic instance
    topic_manager_.UnregisterRemoteTopic(kTestTopicName);
    CreateTestTopic(/*is_remote=*/true);

    // Subscribe returns failure when no topic was registered.
    auto subscribe_result = client_->RequestSubscribe(kTestTopicName);
    EXPECT_EQ(subscribe_result.getStatusCode(), ::sdv::SdvStatusCode::kAlreadyExists);

    // No request sent to peer
    EXPECT_FALSE(connection_->GetNextSentData(/*timeout_seconds=*/1));
}

TEST_F(RemoteAgentClientTest, SubscribeResult_IgnoredWhenTopicUnrecognized) {
    PrepareClientWithPendingSubscribeRequest();

    Payload_SubscribeResult subscribe_result;
    subscribe_result.topic_name_ = "unknown-topic-name";
    subscribe_result.result_ = Payload_SubscribeResult::Result_Failure;
    connection_->EnqueueParcelForReceive(484, subscribe_result);

    // Give ConnectionThread a chance to work.
    std::this_thread::sleep_for(std::chrono::seconds(1));

    // Topic unaffected.
    EXPECT_TRUE(topic_manager_.FindTopic(kTestTopicName, /*local_only=*/false));
    EXPECT_FALSE(client_->Subscriptions().at(kTestTopicName).is_active);
}

TEST_F(RemoteAgentClientTest, SubscribeResult_UnregistersTopicOnFailure) {
    PrepareClientWithPendingSubscribeRequest();

    Payload_SubscribeResult subscribe_result;
    subscribe_result.topic_name_ = kTestTopicName;
    subscribe_result.result_ = Payload_SubscribeResult::Result_Failure;
    connection_->EnqueueParcelForReceive(242, subscribe_result);

    // Give ConnectionThread a chance to work.
    WaitForConditionOrTimeout(
            [&]() { return !topic_manager_.FindTopic(kTestTopicName, /*local_only=*/false); });

    // Topic unregistered and no longer among imports.
    EXPECT_FALSE(topic_manager_.FindTopic(kTestTopicName, /*local_only=*/false));
    EXPECT_FALSE(client_->Subscriptions().contains(kTestTopicName));
}

TEST_F(RemoteAgentClientTest, SubscribeResult_UnregistersTopicOnMessageSizeMismatch) {
    PrepareClientWithPendingSubscribeRequest();

    Payload_SubscribeResult subscribe_result;
    subscribe_result.topic_name_ = kTestTopicName;
    subscribe_result.result_ = Payload_SubscribeResult::Result_Ok;
    subscribe_result.message_size_ = kTestMessageSize + 2;
    subscribe_result.message_count_ = kTestMessageCount;
    connection_->EnqueueParcelForReceive(242, subscribe_result);

    // Give ConnectionThread a chance to work.
    WaitForConditionOrTimeout(
            [&]() { return !topic_manager_.FindTopic(kTestTopicName, /*local_only=*/false); });

    // Topic unregistered and no longer among imports.
    EXPECT_FALSE(topic_manager_.FindTopic(kTestTopicName, /*local_only=*/false));
    EXPECT_FALSE(client_->Subscriptions().contains(kTestTopicName));
}

TEST_F(RemoteAgentClientTest, SubscribeResult_UnregistersTopicOnMessageCountMismatch) {
    PrepareClientWithPendingSubscribeRequest();

    Payload_SubscribeResult subscribe_result;
    subscribe_result.topic_name_ = kTestTopicName;
    subscribe_result.result_ = Payload_SubscribeResult::Result_Ok;
    subscribe_result.message_size_ = kTestMessageSize;
    subscribe_result.message_count_ = kTestMessageCount * 2;
    connection_->EnqueueParcelForReceive(242, subscribe_result);

    // Give ConnectionThread a chance to work.
    WaitForConditionOrTimeout(
            [&]() { return !topic_manager_.FindTopic(kTestTopicName, /*local_only=*/false); });

    // Topic unregistered and no longer among imports.
    EXPECT_FALSE(topic_manager_.FindTopic(kTestTopicName, /*local_only=*/false));
    EXPECT_FALSE(client_->Subscriptions().contains(kTestTopicName));
}

TEST_F(RemoteAgentClientTest, SubscribeResult_OkMakesSubscriptionActiveAndAcceptingData) {
    PrepareClientWithPendingSubscribeRequest();

    ASSERT_EQ(topic_reader_->AvailableToRead(), 0);

    // Set up handler for readiness notification.
    std::optional<::sdv::SdvStatusCode> on_ready_result;
    auto topic = topic_manager_.FindTopic(kTestTopicName, /*local_only*/ false);
    ASSERT_TRUE(topic);
    topic->RegisterOnReadyCallback([&](auto status) { on_ready_result = status; });
    EXPECT_FALSE(on_ready_result);  // Not yet ready.

    Payload_SubscribeResult subscribe_result;
    subscribe_result.topic_name_ = kTestTopicName;
    subscribe_result.result_ = Payload_SubscribeResult::Result_Ok;
    subscribe_result.message_size_ = kTestMessageSize;
    subscribe_result.message_count_ = kTestMessageCount;
    subscribe_result.has_last_message_ = 0;
    connection_->EnqueueParcelForReceive(242, subscribe_result);

    // Give ConnectionThread a chance to work.
    WaitForConditionOrTimeout(
            [&]() { return client_->Subscriptions().at(kTestTopicName).is_active; });

    EXPECT_TRUE(client_->Subscriptions().at(kTestTopicName).is_active);

    // Check that readiness callback fired.
    ASSERT_TRUE(on_ready_result);
    EXPECT_EQ(*on_ready_result, ::sdv::SdvStatusCode::kOk);

    // No data available yet since has_last_message_ was false.
    ASSERT_EQ(topic_reader_->AvailableToRead(), 0);

    EnqueueTestTopicMessagesForReceive({123, 456});

    WaitForConditionOrTimeout(
            [&]() { return topic_reader_->AvailableToRead() == (2 * kTestMessageSize); });

    ASSERT_EQ(topic_reader_->AvailableToRead(), 2 * kTestMessageSize);
    TestMessageType received_messages[2];
    ASSERT_TRUE(topic_reader_->Read(reinterpret_cast<uint8_t*>(received_messages),
                                    sizeof(received_messages)));
    EXPECT_EQ(received_messages[0], 123);
    EXPECT_EQ(received_messages[1], 456);
}

TEST_F(RemoteAgentClientTest, SubscribeResult_OkWithLastMessageWritesMessageToTopic) {
    PrepareClientWithPendingSubscribeRequest();

    ASSERT_EQ(topic_reader_->AvailableToRead(), 0);

    TestMessageType sent_last_message = 912812;

    Payload_SubscribeResult subscribe_result;
    subscribe_result.topic_name_ = kTestTopicName;
    subscribe_result.result_ = Payload_SubscribeResult::Result_Ok;
    subscribe_result.message_size_ = kTestMessageSize;
    subscribe_result.message_count_ = kTestMessageCount;
    subscribe_result.has_last_message_ = 1;
    subscribe_result.last_message_content_.Resize(kTestMessageSize, false);
    memcpy(subscribe_result.last_message_content_.Ptr(), &sent_last_message, kTestMessageSize);
    connection_->EnqueueParcelForReceive(242, subscribe_result);

    // Give ConnectionThread a chance to work.
    WaitForConditionOrTimeout(
            [&]() { return client_->Subscriptions().at(kTestTopicName).is_active; });

    EXPECT_TRUE(client_->Subscriptions().at(kTestTopicName).is_active);

    // Last message available since provided in result.
    ASSERT_EQ(topic_reader_->AvailableToRead(), kTestMessageSize);

    TestMessageType received_last_message;
    ASSERT_TRUE(topic_reader_->Read(reinterpret_cast<uint8_t*>(&received_last_message),
                                    kTestMessageSize));
    EXPECT_EQ(received_last_message, sent_last_message);
}

TEST_F(RemoteAgentClientTest, TopicRawData_IgnoredIfNoSubscription) {
    PrepareClientWithActiveConnection();
    // Topic exists but no subscription requested.
    CreateTestTopic(/*is_remote=*/true);

    ASSERT_EQ(topic_reader_->AvailableToRead(), 0);

    EnqueueTestTopicMessagesForReceive({123, 456});

    // Sleep briefly so we don't get a false success due to propagation delay
    std::this_thread::sleep_for(std::chrono::seconds(1));

    // Verify nothing made it to the FMQ.
    ASSERT_EQ(topic_reader_->AvailableToRead(), 0);
}

TEST_F(RemoteAgentClientTest, TopicRawData_IgnoredIfSubscriptionInactive) {
    PrepareClientWithPendingSubscribeRequest();

    ASSERT_FALSE(client_->Subscriptions().at(kTestTopicName).is_active);
    ASSERT_EQ(topic_reader_->AvailableToRead(), 0);

    EnqueueTestTopicMessagesForReceive({123, 456});

    // Sleep briefly so we don't get a false success due to propagation delay
    std::this_thread::sleep_for(std::chrono::seconds(1));

    // Verify nothing made it to the FMQ.
    ASSERT_EQ(topic_reader_->AvailableToRead(), 0);
}

TEST_F(RemoteAgentClientTest, Subscription_BecomesInactiveOnDisconnectAndReactivatesOnReconnect) {
    PrepareClientWithActiveSubscription();

    // Ensure subscription is active to start out.
    ASSERT_TRUE(client_->Subscriptions().at(kTestTopicName).is_active);

    // Simulate failure.

    // Should see reconnect attempt after failure.
    std::promise<void> connect_promise;
    EXPECT_CALL(connection_manager_, Connect(Eq(kTestServerAddress), Eq(kTestServerFqinString), _))
            .WillOnce([&](const auto&, const auto&, auto& event_handler) {
                connect_event_handler_ = &event_handler;
                connect_promise.set_value();
            });

    connection_->EnqueueReceiveFailure();

    ASSERT_EQ(connect_promise.get_future().wait_for(std::chrono::seconds(2)),
              std::future_status::ready);

    // Subscription marked inactive by disconnect.
    EXPECT_FALSE(client_->Subscriptions().at(kTestTopicName).is_active);

    // Receive a connect result and return to ACTIVE.
    CompleteConnection();

    // Should have sent out a new suscribe request.
    auto resubscribe_payload = connection_->GetNextSentDataIfPayload<Payload_SubscribeRequest>();
    ASSERT_TRUE(resubscribe_payload);
    ASSERT_EQ(resubscribe_payload->topic_name_, kTestTopicName);

    // Simulate receiving subscribe result.
    EnqueueSubscribeResultOkForReceive();

    // Give ConnectionThread a chance to work.
    WaitForConditionOrTimeout(
            [&]() { return client_->Subscriptions().at(kTestTopicName).is_active; });

    // Subscription has returned to active.
    EXPECT_TRUE(client_->Subscriptions().at(kTestTopicName).is_active);
}

TEST_F(RemoteAgentClientTest, TopicDestroyed_IgnoredForUnsubscribedTopic) {
    PrepareClientWithActiveConnection();
    CreateTestTopic(/*is_remote=*/true);

    ASSERT_FALSE(client_->Subscriptions().contains(kTestTopicName));

    Payload_TopicDestroyed payload;
    payload.topic_name_ = kTestTopicName;
    connection_->EnqueueParcelForReceive(894, payload);

    std::this_thread::sleep_for(std::chrono::seconds(1));

    // Topic not destroyed.
    ASSERT_TRUE(topic_manager_.FindTopic(kTestTopicName, /*local_only*/ false));
}

TEST_F(RemoteAgentClientTest, TopicDestroyed_ProcessedForInactiveSubscription) {
    PrepareClientWithPendingSubscribeRequest();

    ASSERT_TRUE(client_->Subscriptions().contains(kTestTopicName));
    ASSERT_FALSE(client_->Subscriptions().at(kTestTopicName).is_active);

    Payload_TopicDestroyed payload;
    payload.topic_name_ = kTestTopicName;
    connection_->EnqueueParcelForReceive(894, payload);

    WaitForConditionOrTimeout([&]() { return !client_->Subscriptions().contains(kTestTopicName); });

    // Subscription removed
    EXPECT_FALSE(client_->Subscriptions().contains(kTestTopicName));

    // Topic was destroyed
    EXPECT_FALSE(topic_manager_.FindTopic(kTestTopicName, /*local_only*/ false));
}

TEST_F(RemoteAgentClientTest, TopicDestroyed_ProcessedForActiveSubscription) {
    PrepareClientWithActiveSubscription();

    ASSERT_TRUE(client_->Subscriptions().contains(kTestTopicName));
    ASSERT_TRUE(client_->Subscriptions().at(kTestTopicName).is_active);

    Payload_TopicDestroyed payload;
    payload.topic_name_ = kTestTopicName;
    connection_->EnqueueParcelForReceive(894, payload);

    WaitForConditionOrTimeout([&]() { return !client_->Subscriptions().contains(kTestTopicName); });

    // Subscription removed
    EXPECT_FALSE(client_->Subscriptions().contains(kTestTopicName));

    // Topic was destroyed
    EXPECT_FALSE(topic_manager_.FindTopic(kTestTopicName, /*local_only*/ false));
}

}  // namespace
}  // namespace android::sdv::dt::agent::test
