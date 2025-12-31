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

#include "connection.h"
#include "connection_grpc/connection_manager.h"

#include <gtest/gtest.h>

#include <future>
#include <memory>
#include <vector>

namespace android::sdv::dt::agent {

class TestEventHandler : public ConnectionManager::EventHandler {
public:
    void OnConnectionManagerEvent(::sdv::SdvResult<std::unique_ptr<IConnection>> result) override {
        if (result.ok()) {
            connection_promise_.set_value(std::move(result.value()));
        } else {
            // For this test, we'll just set a null pointer to signal failure.
            connection_promise_.set_value(nullptr);
        }
    }

    std::future<std::unique_ptr<IConnection>> GetFuture() {
        return connection_promise_.get_future();
    }

private:
    std::promise<std::unique_ptr<IConnection>> connection_promise_;
};

class GrpcConnectionBaseTest : public ::testing::Test {
protected:
    void SetUp() override {
        server_manager_ =
                CreateGrpcConnectionManager(IPAddressV4::GetLocal().value(), std::nullopt);
        client_manager_ =
                CreateGrpcConnectionManager(IPAddressV4::GetLocal().value(), std::nullopt);

        server_event_handler_ = std::make_unique<TestEventHandler>();
        server_manager_->Listen(*server_event_handler_);
    }

    void TearDown() override {
        if (client_connection_) {
            client_connection_->Disconnect();
        }
        if (server_connection_) {
            server_connection_->Disconnect();
        }
        client_manager_->Shutdown();
        server_manager_->Shutdown();
    }

    void EstablishConnection() {
        client_event_handler_ = std::make_unique<TestEventHandler>();

        const auto& server_ip = server_manager_->GetLocalIp();
        auto server_port = server_manager_->GetListenPort();
        ASSERT_TRUE(server_port != 0);
        SocketAddressV4 server_addr(server_ip, server_port);

        client_manager_->Connect(server_addr, "localhost", *client_event_handler_);

        auto client_future = client_event_handler_->GetFuture();
        auto server_future = server_event_handler_->GetFuture();

        client_connection_ = client_future.get();
        server_connection_ = server_future.get();

        ASSERT_NE(client_connection_, nullptr);
        ASSERT_NE(server_connection_, nullptr);
    }

    std::unique_ptr<ConnectionManager> server_manager_;
    std::unique_ptr<ConnectionManager> client_manager_;
    std::unique_ptr<TestEventHandler> server_event_handler_;
    std::unique_ptr<TestEventHandler> client_event_handler_;
    std::unique_ptr<IConnection> server_connection_;
    std::unique_ptr<IConnection> client_connection_;
};

TEST_F(GrpcConnectionBaseTest, ConnectAndDisconnect) {
    EstablishConnection();
    // Disconnect is handled in TearDown.
}

TEST_F(GrpcConnectionBaseTest, SendAndReceiveData_ClientToServer) {
    EstablishConnection();

    std::vector<char> send_buffer = {'h', 'e', 'l', 'l', 'o'};
    std::vector<char> recv_buffer(send_buffer.size());

    ASSERT_TRUE(client_connection_->SendData(send_buffer.data(), send_buffer.size()));

    size_t bytes_received = server_connection_->ReceiveData(recv_buffer.data(), recv_buffer.size());
    ASSERT_EQ(bytes_received, send_buffer.size());
    EXPECT_EQ(recv_buffer, send_buffer);
}

TEST_F(GrpcConnectionBaseTest, SendAndReceiveData_ServerToClient) {
    EstablishConnection();

    std::vector<char> send_buffer = {'w', 'o', 'r', 'l', 'd'};
    std::vector<char> recv_buffer(send_buffer.size());

    ASSERT_TRUE(server_connection_->SendData(send_buffer.data(), send_buffer.size()));

    size_t bytes_received = client_connection_->ReceiveData(recv_buffer.data(), recv_buffer.size());
    ASSERT_EQ(bytes_received, send_buffer.size());
    EXPECT_EQ(recv_buffer, send_buffer);
}

TEST_F(GrpcConnectionBaseTest, SendAndReceiveLargeData) {
    EstablishConnection();

    constexpr size_t kLargeSize = 1024 * 1024;  // 1MB
    std::vector<char> send_buffer(kLargeSize);
    for (size_t i = 0; i < kLargeSize; ++i) {
        send_buffer[i] = static_cast<char>(i % 256);
    }
    std::vector<char> recv_buffer(kLargeSize);

    ASSERT_TRUE(client_connection_->SendData(send_buffer.data(), send_buffer.size()));

    size_t total_received = 0;
    while (total_received < kLargeSize) {
        size_t bytes_received =
                server_connection_->ReceiveData(recv_buffer.data() + total_received,
                                                recv_buffer.size() - total_received);
        ASSERT_GT(bytes_received, 0) << "ReceiveData returned 0 before all data was received.";
        total_received += bytes_received;
    }

    ASSERT_EQ(total_received, kLargeSize);
    EXPECT_EQ(recv_buffer, send_buffer);
}

TEST_F(GrpcConnectionBaseTest, AuthenticatedIdentityWithoutSsl) {
    EstablishConnection();
    EXPECT_FALSE(client_connection_->AuthenticatedIdentity().has_value());
    EXPECT_FALSE(server_connection_->AuthenticatedIdentity().has_value());
    EXPECT_FALSE(client_connection_->AuthenticatedVmName().has_value());
    EXPECT_FALSE(server_connection_->AuthenticatedVmName().has_value());
}

}  // namespace android::sdv::dt::agent
