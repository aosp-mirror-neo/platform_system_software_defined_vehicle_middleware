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

#include "certs.h"
#include "connection.h"
#include "connection_grpc/client_authenticator.h"
#include "connection_grpc/client_connection.h"
#include "connection_grpc/connection_base.h"
#include "connection_grpc/connection_manager.h"
#include "connection_grpc/operation_state_machine.h"
#include "connection_grpc/read_data.h"
#include "connection_grpc/server_connection.h"

#include <android-base/file.h>
#include <android-base/logging.h>
#include <arpa/inet.h>
#include <gtest/gtest.h>

#include <chrono>
#include <fstream>
#include <future>
#include <iostream>
#include <iterator>
#include <string>
namespace android::sdv::dt::agent::test {

const char* kTestFqins[2] = {"vm0:google.sdv.dt.IDataTunnelAgent/default",
                             "vm1:google.sdv.dt.IDataTunnelAgent/default"};

SocketAddressV4 GetListenAddress(const ConnectionManager& cm) {
    return SocketAddressV4(cm.GetLocalIp(), cm.GetListenPort());
}

using ConnectResult = ::sdv::SdvResult<IConnectionHolder>;

struct TestEventHandler : ConnectionManager::EventHandler {
    TestEventHandler() : result_future_(result_promise_.get_future()) {}

    virtual void OnConnectionManagerEvent(ConnectResult result) override {
        result_promise_.set_value(std::move(result));
    }

    bool HasConnectResult() {
        return result_future_.wait_for(std::chrono::seconds(0)) == std::future_status::ready;
    }

    ConnectResult GetConnectResult() {
        CHECK(result_future_.wait_for(std::chrono::seconds(5)) == std::future_status::ready);
        auto result = result_future_.get();
        result_promise_ = std::promise<ConnectResult>();
        result_future_ = result_promise_.get_future();
        return result;
    }

    // Barfs if there was an error.
    IConnectionHolder GetConnection() { return GetConnectResult().value(); }

private:
    std::promise<ConnectResult> result_promise_;
    std::future<ConnectResult> result_future_;
};

using CmInstance = std::unique_ptr<ConnectionManager>;
using CmFactory =
        std::function<std::unique_ptr<ConnectionManager>(ConnectionManager::EventHandler&)>;

// Represents a configuration to test; really just a pair of functions to create connection
// managers that can talk to each other.
using CmFactoryPair = std::pair<CmFactory, CmFactory>;

class DtConnectionManagerTestBase {
protected:
    auto SetupConnectedPair(const CmFactory& factory_0, const CmFactory& factory_1) {
        managers_[0] = factory_0(event_handlers_[0]);
        managers_[1] = factory_1(event_handlers_[1]);

        managers_[1]->Connect(GetListenAddress(*managers_[0]), kTestFqins[0], event_handlers_[1]);

        return std::make_pair(event_handlers_[0].GetConnection(),
                              event_handlers_[1].GetConnection());
    }

private:
    TestEventHandler event_handlers_[2];
    std::unique_ptr<ConnectionManager> managers_[2];
};

class DtConnectionManagerTest :
      public DtConnectionManagerTestBase,
      public testing::TestWithParam<CmFactoryPair> {
protected:
    auto SetupConnectedPair() {
        return DtConnectionManagerTestBase::SetupConnectedPair(GetParam().first, GetParam().second);
    }
};

TEST_P(DtConnectionManagerTest, BasicFlow) {
    auto [conn0, conn1] = SetupConnectedPair();

    // Send 0 -> 1
    uint8_t send_buf[8] = {0, 1, 2, 3, 4, 5, 6, 7};
    ASSERT_TRUE(conn0->SendData(send_buf, sizeof(send_buf)));

    uint8_t recv_buf[8] = {0, 0, 0, 0, 0, 0, 0, 0};
    ASSERT_EQ(conn1->ReceiveData(recv_buf, sizeof(recv_buf)), 8);

    ASSERT_EQ(memcmp(send_buf, recv_buf, sizeof(send_buf)), 0);

    // Send 1 -> 0
    uint8_t send_buf_2[8] = {8, 9, 10, 11, 12, 13, 14, 15};
    ASSERT_TRUE(conn1->SendData(send_buf, sizeof(send_buf_2)));

    uint8_t recv_buf_2[8] = {0, 0, 0, 0, 0, 0, 0, 0};
    ASSERT_EQ(conn0->ReceiveData(recv_buf_2, sizeof(recv_buf_2)), 8);

    ASSERT_EQ(memcmp(send_buf, recv_buf, sizeof(send_buf)), 0);
}

TEST_P(DtConnectionManagerTest, SplitRead) {
    auto [conn0, conn1] = SetupConnectedPair();

    // Send 0 -> 1
    uint8_t send_buf[16] = {15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0};
    ASSERT_TRUE(conn0->SendData(send_buf, sizeof(send_buf)));

    uint8_t recv_buf_a[6] = {0, 0, 0, 0, 0, 0};
    ASSERT_EQ(conn1->ReceiveData(recv_buf_a, 6), 6);
    ASSERT_EQ(memcmp(send_buf, recv_buf_a, 6), 0);

    uint8_t recv_buf_b[6] = {0, 0, 0, 0, 0, 0};
    ASSERT_EQ(conn1->ReceiveData(recv_buf_b, 6), 6);
    ASSERT_EQ(memcmp(send_buf + 6, recv_buf_b, 6), 0);

    uint8_t recv_buf_c[6] = {0, 0, 0, 0, 0, 0};
    ASSERT_EQ(conn1->ReceiveData(recv_buf_c, 6), 4);
    ASSERT_EQ(memcmp(send_buf + 12, recv_buf_c, 4), 0);

    // Send 1 -> 0
    uint8_t send_buf_2[16] = {0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30};
    ASSERT_TRUE(conn1->SendData(send_buf_2, sizeof(send_buf_2)));

    uint8_t recv_buf_2a[7] = {0, 0, 0, 0, 0, 0, 0};
    ASSERT_EQ(conn0->ReceiveData(recv_buf_2a, 7), 7);
    ASSERT_EQ(memcmp(send_buf_2, recv_buf_2a, 7), 0);

    uint8_t recv_buf_2b[7] = {0, 0, 0, 0, 0, 0, 0};
    ASSERT_EQ(conn0->ReceiveData(recv_buf_2b, 7), 7);
    ASSERT_EQ(memcmp(send_buf_2 + 7, recv_buf_2b, 7), 0);

    uint8_t recv_buf_2c[7] = {0, 0, 0, 0, 0, 0, 0};
    ASSERT_EQ(conn0->ReceiveData(recv_buf_2c, 7), 2);
    ASSERT_EQ(memcmp(send_buf_2 + 14, recv_buf_2c, 2), 0);
}

TEST_P(DtConnectionManagerTest, PartialRead) {
    auto [conn0, conn1] = SetupConnectedPair();

    // Send 0 -> 1
    uint8_t send_buf[8] = {0xd, 0xe, 0xa, 0xd, 0xb, 0xe, 0xe, 0xf};
    ASSERT_TRUE(conn0->SendData(send_buf, sizeof(send_buf)));

    uint8_t recv_buf[16] = {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
    ASSERT_EQ(conn1->ReceiveData(recv_buf, sizeof(recv_buf)), sizeof(send_buf));

    ASSERT_EQ(memcmp(send_buf, recv_buf, sizeof(send_buf)), 0);

    // Send 1 -> 0
    uint8_t send_buf_2[8] = {0xa, 0xb, 0xc, 0xd, 0xa, 0xb, 0xc, 0xd};
    ASSERT_TRUE(conn0->SendData(send_buf_2, sizeof(send_buf_2)));

    uint8_t recv_buf_2[16] = {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0};
    ASSERT_EQ(conn1->ReceiveData(recv_buf_2, sizeof(recv_buf_2)), sizeof(send_buf_2));

    ASSERT_EQ(memcmp(send_buf, recv_buf, sizeof(send_buf_2)), 0);
}

TEST_P(DtConnectionManagerTest, GetLocalIpAddress) {
    TestEventHandler eh;
    auto cm = GetParam().first(eh);

    IPAddressV4 local_ip = cm->GetLocalIp();
    IPAddressV4 expected_ip = IPAddressV4::GetLocal().value();

    ASSERT_EQ(local_ip.ToString(), expected_ip.ToString());
    ASSERT_NE(cm->GetListenPort(), 0) << "Listen port should be non-zero";
}

TEST_P(DtConnectionManagerTest, ConcurrentSenders) {
    IConnectionHolder conn[2];
    std::tie(conn[0], conn[1]) = SetupConnectedPair();

    static const size_t SENDER_COUNT = 16;
    static const size_t MESSAGE_SIZE = 128;
    static const size_t MESSAGE_COUNT_PER_SENDER = 4096;
    static const size_t MESSAGE_COUNT_TOTAL = MESSAGE_COUNT_PER_SENDER * SENDER_COUNT;

    // This test starts two threads to receive messages (one for each connection endpoint),
    // and SENDER_COUNT threads to send messages on the connections.  (Each connection will
    // have SENDER_COUNT/2 threads sending messages through it.)
    //
    // Each sender thread sends MESSAGE_COUNT_PER_SENDER messages of size MESSAGE_SIZE,
    // with the message content being the thread's index repeated MESSAGE_SIZE times.
    //
    // Receivers will keep counts of how many messages with each value they receive and at
    // the end we'll make sure the receivers got the expected #s of messages.

    // Start up 2 receiver threads.
    std::unordered_map<uint8_t, int> recv_counts[2];
    std::thread recv_threads[2];
    for (int recv_idx = 0; recv_idx < 2; recv_idx++) {
        recv_threads[recv_idx] = std::thread([&, recv_idx]() {
            uint8_t recv_buf[MESSAGE_SIZE];
            for (int i = 0; i < (MESSAGE_COUNT_TOTAL / 2); i++) {
                if (conn[recv_idx]->ReceiveData(recv_buf, MESSAGE_SIZE) != MESSAGE_SIZE) {
                    break;
                }
                recv_counts[recv_idx][recv_buf[0]]++;
            }
        });
    }

    // Start up SENDER_COUNT sender threads.
    std::thread send_threads[SENDER_COUNT];
    for (int thread_idx = 0; thread_idx < SENDER_COUNT; thread_idx++) {
        send_threads[thread_idx] = std::thread([&, thread_idx]() {
            // Determine which connection this thread will send on.
            int send_idx = thread_idx % 2;
            std::vector<uint8_t> buffer(MESSAGE_SIZE, (uint8_t)thread_idx);
            for (int i = 0; i < MESSAGE_COUNT_PER_SENDER; i++) {
                if (!conn[send_idx]->SendData(buffer.data(), MESSAGE_SIZE)) {
                    break;
                }
            }
        });
    }

    // Wait for all senders to complete.
    for (int idx = 0; idx < SENDER_COUNT; idx++) {
        send_threads[idx].join();
    }

    // Wait for receiver threads to exit.
    recv_threads[0].join();
    recv_threads[1].join();

    // Verify that we got the expected messages.
    for (uint8_t idx = 0; idx < SENDER_COUNT; idx++) {
        uint8_t recv_idx = (idx + 1) % 2;
        ASSERT_TRUE(recv_counts[recv_idx].contains(idx));
        ASSERT_EQ(recv_counts[recv_idx][idx], MESSAGE_COUNT_PER_SENDER);
    }
}

TEST_P(DtConnectionManagerTest, OutboundConnectFails) {
    TestEventHandler eh;
    auto cm = GetParam().first(eh);

    SocketAddressV4 addr(cm->GetLocalIp(), 42);

    cm->Connect(addr, kTestFqins[1], eh);
    auto conn_result = eh.GetConnectResult();
    ASSERT_FALSE(conn_result.has_value());
}

TEST_P(DtConnectionManagerTest, MultipleClients) {
    TestEventHandler listen_eh[4];
    std::unique_ptr<ConnectionManager> cm[4];
    IConnectionHolder s_conn[3];
    IConnectionHolder c_conn[3];

    for (int i = 0; i < 4; i++) {
        cm[i] = GetParam().first(listen_eh[i]);
    }

    TestEventHandler connect_eh[3];
    for (int i = 0; i < 3; i++) {
        cm[i]->Connect(GetListenAddress(*cm[3]), kTestFqins[0], connect_eh[i]);
        c_conn[i] = connect_eh[i].GetConnection();
        s_conn[i] = listen_eh[3].GetConnection();
    }

    ASSERT_FALSE(listen_eh[0].HasConnectResult());
    ASSERT_FALSE(listen_eh[1].HasConnectResult());
    ASSERT_FALSE(listen_eh[2].HasConnectResult());

    for (uint8_t i = 0; i < 3; i++) {
        // Send client -> server
        uint8_t send_buf[8] = {i, 1, 2, 3, 4, 5, 6, 7};
        ASSERT_TRUE(c_conn[i]->SendData(send_buf, sizeof(send_buf)));

        uint8_t recv_buf[8] = {0, 0, 0, 0, 0, 0, 0, 0};
        ASSERT_EQ(s_conn[i]->ReceiveData(recv_buf, sizeof(recv_buf)), 8);

        ASSERT_EQ(memcmp(send_buf, recv_buf, sizeof(send_buf)), 0);

        // Send server -> client
        uint8_t send_buf_2[8] = {8, 9, 10, 11, 12, 13, 14, i};
        ASSERT_TRUE(s_conn[i]->SendData(send_buf, sizeof(send_buf_2)));

        uint8_t recv_buf_2[8] = {0, 0, 0, 0, 0, 0, 0, 0};
        ASSERT_EQ(c_conn[i]->ReceiveData(recv_buf_2, sizeof(recv_buf_2)), 8);

        ASSERT_EQ(memcmp(send_buf, recv_buf, sizeof(send_buf)), 0);
    }
}

TEST_P(DtConnectionManagerTest, MultipleServers) {
    TestEventHandler listen_eh[4];
    std::unique_ptr<ConnectionManager> cm[4];
    IConnectionHolder s_conn[3];
    IConnectionHolder c_conn[3];

    for (int i = 0; i < 4; i++) {
        cm[i] = GetParam().first(listen_eh[i]);
    }

    TestEventHandler connect_eh;
    for (int i = 0; i < 3; i++) {
        cm[3]->Connect(GetListenAddress(*cm[i]), kTestFqins[0], connect_eh);
        c_conn[i] = connect_eh.GetConnection();
        s_conn[i] = listen_eh[i].GetConnection();
    }

    // Client's listen hander would have gotten no event
    ASSERT_FALSE(listen_eh[3].HasConnectResult());

    for (uint8_t i = 0; i < 3; i++) {
        // Send client -> server
        uint8_t send_buf[8] = {i, 1, 2, 3, 4, 5, 6, 7};
        ASSERT_TRUE(c_conn[i]->SendData(send_buf, sizeof(send_buf)));

        uint8_t recv_buf[8] = {0, 0, 0, 0, 0, 0, 0, 0};
        ASSERT_EQ(s_conn[i]->ReceiveData(recv_buf, sizeof(recv_buf)), 8);

        ASSERT_EQ(memcmp(send_buf, recv_buf, sizeof(send_buf)), 0);

        // Send server -> client
        uint8_t send_buf_2[8] = {8, 9, 10, 11, 12, 13, 14, i};
        ASSERT_TRUE(s_conn[i]->SendData(send_buf, sizeof(send_buf_2)));

        uint8_t recv_buf_2[8] = {0, 0, 0, 0, 0, 0, 0, 0};
        ASSERT_EQ(c_conn[i]->ReceiveData(recv_buf_2, sizeof(recv_buf_2)), 8);

        ASSERT_EQ(memcmp(send_buf, recv_buf, sizeof(send_buf)), 0);
    }
}

CmInstance InsecureGrpcCmFactory(ConnectionManager::EventHandler& eh) {
    auto result = CreateGrpcConnectionManager(IPAddressV4::GetLocal().value(),
                                              /* std_config= */ std::nullopt);
    result->Listen(eh);
    return result;
}

INSTANTIATE_TEST_SUITE_P(GrpcInsecure, DtConnectionManagerTest,
                         testing::Values(CmFactoryPair(InsecureGrpcCmFactory,
                                                       InsecureGrpcCmFactory)));

std::string ReadSslTestDataFile(const std::string& filename) {
    std::string full_path =
            android::base::GetExecutableDirectory() + "/tests/ssl_test_data/" + filename;
    std::ifstream file(full_path);
    std::string contents;
    copy(std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>(),
         std::back_inserter(contents));
    CHECK(!contents.empty()) << "Could not read file " << full_path;
    return contents;
}

struct TestSslPemConfig {
    const char* pem_cert_chain_file;
    const char* pem_pkey_file;
};

static const TestSslPemConfig test_ssl_configs[2] = {
        {
                "service_0_cert_chain.pem",
                "service_0_pkey.pem",
        },
        {
                "service_1_cert_chain.pem",
                "service_1_pkey.pem",
        },
};

class TestTrustedRootsAccessor : public TrustedRootsAccessor {
public:
    explicit TestTrustedRootsAccessor(std::string trusted_roots_pem) :
          trusted_roots_pem_(std::move(trusted_roots_pem)),
          trusted_root_set_(TrustedRootSet::BuildFromPEM(trusted_roots_pem_).value()) {}

    virtual ~TestTrustedRootsAccessor() = default;

    std::string GetAllTrustedRootsAsPEM() const override { return trusted_roots_pem_; }

    bool IsSignedByTrustedRoot(const std::string_view leaf_cert_pem) const override {
        return trusted_root_set_.IsSignedByAny(leaf_cert_pem);
    }

private:
    std::string trusted_roots_pem_;
    TrustedRootSet trusted_root_set_;
};

CmFactory MakeSslGrpcCmFactory(int idx) {
    static auto trusted_roots_accessor = [&]() {
        auto pem_trusted_root_certs = ReadSslTestDataFile("root_certs.pem");
        return new TestTrustedRootsAccessor(pem_trusted_root_certs);
    }();

    ConnectionManager::SslConfig ssl_cfg{
            .trusted_roots_accessor = *trusted_roots_accessor,
            .pem_cert_chain = ReadSslTestDataFile(test_ssl_configs[idx].pem_cert_chain_file),
            .pem_private_key = ReadSslTestDataFile(test_ssl_configs[idx].pem_pkey_file),
    };

    return [ssl_cfg](ConnectionManager::EventHandler& eh) -> CmInstance {
        auto result = CreateGrpcConnectionManager(IPAddressV4::GetLocal().value(), ssl_cfg);
        result->Listen(eh);
        return result;
    };
}

INSTANTIATE_TEST_SUITE_P(GrpcSsl, DtConnectionManagerTest,
                         testing::Values(CmFactoryPair(MakeSslGrpcCmFactory(0),
                                                       MakeSslGrpcCmFactory(1))));

class DtConnectionManagerCertIdentitiesTest :
      public DtConnectionManagerTestBase,
      public ::testing::Test {};

TEST_F(DtConnectionManagerCertIdentitiesTest, NoAuthenticatedIdentitiesWhenInsecure) {
    auto [conn0, conn1] = SetupConnectedPair(InsecureGrpcCmFactory, InsecureGrpcCmFactory);
    ASSERT_FALSE(conn0->AuthenticatedIdentity());
    ASSERT_FALSE(conn1->AuthenticatedIdentity());
    ASSERT_FALSE(conn0->AuthenticatedVmName());
    ASSERT_FALSE(conn1->AuthenticatedVmName());
}

TEST_F(DtConnectionManagerCertIdentitiesTest, CorrectAuthenticatedIdentitiesWhenSecure) {
    auto [conn0, conn1] = SetupConnectedPair(MakeSslGrpcCmFactory(0), MakeSslGrpcCmFactory(1));

    auto conn0_id = conn0->AuthenticatedIdentity();
    ASSERT_TRUE(conn0_id);
    // Expect SAN from cert 0.
    EXPECT_EQ(*conn0_id, "khde30k2v4fql0jgurskknhputhm0ak8nhtg4n7a72691hq5q900");

    auto conn1_id = conn1->AuthenticatedIdentity();
    ASSERT_TRUE(conn1_id);
    // Expect SAN from cert 1.
    EXPECT_EQ(*conn1_id, "vc1i0snu5m9is48r82bdflu2iqg4g73r8uo98bfluhip1u103gjg");

    auto conn0_vm = conn0->AuthenticatedVmName();
    ASSERT_TRUE(conn0_vm);
    EXPECT_EQ(*conn0_vm, "vm1:CertAuthority");

    auto conn1_vm = conn1->AuthenticatedVmName();
    ASSERT_TRUE(conn1_vm);
    EXPECT_EQ(*conn1_vm, "vm0:CertAuthority");
}

TEST(GrpcConnectionManagerTest, Cleanly_Destroy_Without_Listen) {
    // No real asserts here; just not crashing is good enough.
    auto conn_mgr = CreateGrpcConnectionManager(IPAddressV4::GetLocal().value(), std::nullopt);
    conn_mgr.reset();
}

TEST(ConnectionSerialization, IPAddressV4Serialization) {
    // Test with a known IP address
    in_addr test_addr;
    inet_pton(AF_INET, "192.168.1.100", &test_addr);
    IPAddressV4 original_ip(test_addr);

    std::vector<uint8_t> buffer;
    original_ip.Serialize(buffer);

    // Verify serialized size
    ASSERT_EQ(buffer.size(), sizeof(in_addr));

    // Deserialize back
    IPAddressV4 deserialized_ip;
    size_t bytes_read = deserialized_ip.Deserialize(buffer.data(), buffer.size());

    ASSERT_EQ(bytes_read, sizeof(in_addr));
    ASSERT_EQ(original_ip.ToString(), deserialized_ip.ToString());
    ASSERT_EQ(static_cast<const in_addr&>(original_ip).s_addr,
              static_cast<const in_addr&>(deserialized_ip).s_addr);

    // Test with another IP
    inet_pton(AF_INET, "10.0.0.1", &test_addr);
    original_ip = IPAddressV4(test_addr);
    buffer.clear();
    original_ip.Serialize(buffer);
    deserialized_ip = IPAddressV4();  // Reset
    bytes_read = deserialized_ip.Deserialize(buffer.data(), buffer.size());
    ASSERT_EQ(bytes_read, sizeof(in_addr));
    ASSERT_EQ(original_ip.ToString(), deserialized_ip.ToString());

    // Test with insufficient buffer size
    IPAddressV4 partial_ip;
    std::vector<uint8_t> small_buffer = {0x01, 0x02, 0x03};  // 3 bytes, needs 4
    bytes_read = partial_ip.Deserialize(small_buffer.data(), small_buffer.size());
    ASSERT_EQ(bytes_read, 0);
}

TEST(ConnectionSerialization, SocketAddressV4Serialization) {
    // Test with a known IP address and port
    in_addr test_addr;
    inet_pton(AF_INET, "192.168.1.100", &test_addr);
    uint16_t test_port = 8080;
    SocketAddressV4 original_sock_addr(test_addr, test_port);

    std::vector<uint8_t> buffer;
    original_sock_addr.Serialize(buffer);

    // Verify serialized size (4 bytes for IP + 2 bytes for port)
    ASSERT_EQ(buffer.size(), sizeof(in_addr) + sizeof(uint16_t));

    // Deserialize back
    SocketAddressV4 deserialized_sock_addr;
    size_t bytes_read = deserialized_sock_addr.Deserialize(buffer.data(), buffer.size());

    ASSERT_EQ(bytes_read, sizeof(in_addr) + sizeof(uint16_t));
    ASSERT_EQ(original_sock_addr.ToString(), deserialized_sock_addr.ToString());
    ASSERT_EQ(static_cast<const sockaddr_in&>(original_sock_addr).sin_addr.s_addr,
              static_cast<const sockaddr_in&>(deserialized_sock_addr).sin_addr.s_addr);
    ASSERT_EQ(static_cast<const sockaddr_in&>(original_sock_addr).sin_port,
              static_cast<const sockaddr_in&>(deserialized_sock_addr).sin_port);

    // Test with another address and port
    inet_pton(AF_INET, "10.0.0.5", &test_addr);
    test_port = 12345;
    original_sock_addr = SocketAddressV4(test_addr, test_port);
    buffer.clear();
    original_sock_addr.Serialize(buffer);
    deserialized_sock_addr = SocketAddressV4();  // Reset
    bytes_read = deserialized_sock_addr.Deserialize(buffer.data(), buffer.size());
    ASSERT_EQ(bytes_read, sizeof(in_addr) + sizeof(uint16_t));
    ASSERT_EQ(original_sock_addr.ToString(), deserialized_sock_addr.ToString());

    // Test with insufficient buffer size for IP
    SocketAddressV4 partial_sock_addr;
    std::vector<uint8_t> small_buffer_ip = {0x01, 0x02, 0x03};  // 3 bytes for IP
    bytes_read = partial_sock_addr.Deserialize(small_buffer_ip.data(), small_buffer_ip.size());
    ASSERT_EQ(bytes_read, 0);

    // Test with insufficient buffer size for port
    std::vector<uint8_t> small_buffer_port = {0x01, 0x02, 0x03, 0x04,
                                              0x05};  // 4 bytes for IP + 1 for port
    bytes_read = partial_sock_addr.Deserialize(small_buffer_port.data(), small_buffer_port.size());
    ASSERT_EQ(bytes_read, 0);
}
}  // namespace android::sdv::dt::agent::test

int main(int argc, char** argv) {
    android::base::SetDefaultTag("DtConnectionManagerTest");
    android::base::SetMinimumLogSeverity(android::base::VERBOSE);
    ::testing::InitGoogleTest(&argc, argv);
    return RUN_ALL_TESTS();
}
