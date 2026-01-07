// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#include "RpcAgentServer.h"
#include "fakeservicemanager/FakeServiceManager.h"

#include <aidl/google/sdv/service_discovery/common/RegistrationToken.h>
#include <aidl/google/sdv/service_discovery/discovery/BnTransportSupportAgent.h>
#include <android-base/unique_fd.h>
#include <android/binder_manager.h>
#include <android/binder_process.h>
#include <arpa/inet.h>
#include <binder/IServiceManager.h>
#include <gmock/gmock.h>
#include <gtest/gtest.h>
#include <netinet/in.h>
#include <utils/RefBase.h>

#include <sys/socket.h>
#include <unistd.h>

// Mock implementations of C functions
// This is not an ODR violation with the ones available in libc:
// The key is that symbol interposition allows the linker to select one definition among several
// available, based on its resolution rules. Since only one definition is ultimately chosen and used
// by the program, the One Definition Rule is upheld for that specific program. The mocked C
// functions below appear earlier in the linker's symbol resolution path and thus the functions
// within libc aren't involved.
// TODO(b/426428571): Nevertheless, the RPC agent's code should be refactored in a way that allows a
// better way to inject those dependencies and a more robust way to test.
extern "C" {

struct prop_info {
    int i;
};  // Dummy definition for mocking.
static prop_info g_dummy_prop_info;
static const prop_info* g_mock_prop_info = nullptr;
static std::string g_mock_prop_value = "";

const prop_info* __system_property_find(const char* /*name*/) {
    return g_mock_prop_info;
}

void __system_property_read_callback(const prop_info* pi,
                                     void (*callback)(void* cookie, const char* name,
                                                      const char* value, unsigned int serial),
                                     void* cookie) {
    if (pi && callback) {
        callback(cookie, "", g_mock_prop_value.c_str(), 0);
    }
}

}  // extern "C"

namespace android::sdv::rpc {
namespace {

// Mock variables for binder services
std::shared_ptr<::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent>
        g_mock_transport_agent = nullptr;

class MockTransportSupportAgent :
      public ::aidl::google::sdv::service_discovery::discovery::BnTransportSupportAgent,
      public virtual android::RefBase {
public:
    MockTransportSupportAgent() = default;
    virtual ~MockTransportSupportAgent() = default;

    MOCK_METHOD(::ndk::ScopedAStatus, getServiceUnitForToken,
                (const ::aidl::google::sdv::service_discovery::common::RegistrationToken& in_token,
                 ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition*
                         _aidl_return),
                (override));

    MOCK_METHOD(::ndk::ScopedAStatus, setAgentMetadata,
                (const ::aidl::google::sdv::service_discovery::common::RegistrationToken& in_token,
                 const ::aidl::google::sdv::service_discovery::common::metadata::TransportMetadata&
                         in_agentMetadata),
                (override));

    MOCK_METHOD(::ndk::ScopedAStatus, getAgentMetadata,
                (int64_t in_sid, const std::string& in_unitName,
                 ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition* out_sud,
                 ::aidl::google::sdv::service_discovery::common::metadata::TransportMetadata*
                         out_agentMetadata),
                (override));
};

static sp<FakeServiceManager> gFakeServiceManager = nullptr;
static std::once_flag gSmOnce;

class RpcAgentServerTest : public ::testing::Test {
protected:
    void SetUp() override {
        std::call_once(gSmOnce, [&]() {
            gFakeServiceManager = sp<FakeServiceManager>::make();
            android::setDefaultServiceManager(gFakeServiceManager);

            // Initialize binder thread pool
            ABinderProcess_setThreadPoolMaxThreadCount(1);
            ABinderProcess_startThreadPool();
        });

        // Reset RpcAgentServer's static instance
        RpcAgentServer::terminateService();
        gFakeServiceManager->clear();

        // Setup mocks for startService
        g_mock_transport_agent = ndk::SharedRefBase::make<MockTransportSupportAgent>();

        auto addServiceStatus =
                AServiceManager_addService(g_mock_transport_agent->asBinder().get(),
                                           (std::string(
                                                    MockTransportSupportAgent::
                                                            ITransportSupportAgent::descriptor) +
                                            "/default")
                                                   .c_str());
        ASSERT_EQ(addServiceStatus, EX_NONE);
    }

    void TearDown() override {
        // Clean up RpcAgentServer's static instance
        RpcAgentServer::terminateService();
        g_mock_transport_agent = nullptr;
        g_mock_prop_info = nullptr;
        g_mock_prop_value.clear();
    }
};

TEST_F(RpcAgentServerTest, StartService_Success) {
    auto result = RpcAgentServer::startService();

    ASSERT_TRUE(result.ok());
    ASSERT_NE(result.value(), nullptr);
}

TEST_F(RpcAgentServerTest, StartService_FailsWhenAlreadyStarted) {
    // Start the service once successfully.
    auto first_result = RpcAgentServer::startService();
    ASSERT_TRUE(first_result.ok());

    // Try to start it again.
    auto second_result = RpcAgentServer::startService();

    ASSERT_FALSE(second_result.ok());
    ASSERT_EQ(second_result.error().code(), INVALID_OPERATION);
    ASSERT_THAT(second_result.error().message(),
                ::testing::HasSubstr("Cannot start service more than once"));
}

TEST_F(RpcAgentServerTest, StartService_FailsWhenInitFails) {
    gFakeServiceManager->clear();
    g_mock_transport_agent = nullptr;

    auto result = RpcAgentServer::startService();

    ASSERT_FALSE(result.ok());
    ASSERT_EQ(result.error().code(), INVALID_OPERATION) << result.error();
    ASSERT_THAT(result.error().message(), ::testing::HasSubstr("Failed to start rpcagent server"));
}

TEST_F(RpcAgentServerTest, GetNetworkInterfaceName_Success) {
    // Simulate __system_property_find finding the property
    // prop_info is an opaque type. We only need a non-NULL pointer for the mock,
    // as the actual content is not dereferenced by the mocked functions.
    g_mock_prop_info = &g_dummy_prop_info;
    g_mock_prop_value = "eth0";

    std::string iface_name;
    // Call the public method that internally calls getConfiguredNetworkInterfaceName
    auto server = RpcAgentServer::startService();
    ASSERT_TRUE(server.ok());

    auto status = server.value()->getNetworkInterfaceName(&iface_name);
    ASSERT_TRUE(status.isOk()) << status.getMessage();
    ASSERT_EQ(iface_name, "eth0");
}

TEST_F(RpcAgentServerTest, GetNetworkInterfaceName_PropertyNotFound) {
    // Simulate __system_property_find not finding the property
    g_mock_prop_info = nullptr;

    auto server = RpcAgentServer::startService();
    ASSERT_TRUE(server.ok());

    std::string iface_name;
    auto status = server.value()->getNetworkInterfaceName(&iface_name);

    ASSERT_FALSE(status.isOk());
    ASSERT_TRUE(iface_name.empty());
}

TEST_F(RpcAgentServerTest, GetNetworkInterfaceName_PropertyEmpty) {
    g_mock_prop_info = &g_dummy_prop_info;
    g_mock_prop_value = "";  // Empty value

    auto server = RpcAgentServer::startService();
    ASSERT_TRUE(server.ok());

    std::string iface_name;
    auto status = server.value()->getNetworkInterfaceName(&iface_name);

    ASSERT_FALSE(status.isOk());
    ASSERT_TRUE(iface_name.empty());
}

TEST_F(RpcAgentServerTest, RegisterServer_Success) {
    g_mock_prop_info = &g_dummy_prop_info;
    g_mock_prop_value =
            "sdv_rpc";  // Valid interface name, ensure this interface exists or ioctl will fail

    auto server_result = RpcAgentServer::startService();
    ASSERT_TRUE(server_result.ok());
    auto server = server_result.value();

    MockTransportSupportAgent* mock_tsa =
            static_cast<MockTransportSupportAgent*>(g_mock_transport_agent.get());
    EXPECT_CALL(*mock_tsa, setAgentMetadata(::testing::_, ::testing::_))
            .WillOnce(::testing::Return(::ndk::ScopedAStatus::ok()));

    ::aidl::google::sdv::service_discovery::common::RegistrationToken token;
    token.token = g_mock_transport_agent->asBinder();  // Dummy binder
    ::ndk::ScopedFileDescriptor fd;

    ::ndk::ScopedAStatus status = server->registerServer(token, &fd);

    // Assert
    ASSERT_TRUE(status.isOk()) << status.getMessage();
    ASSERT_NE(fd.get(), -1) << "Socket FD should be valid";
}

TEST_F(RpcAgentServerTest, RegisterServer_GetNetworkInterfaceFails) {
    g_mock_prop_info = nullptr;  // This will cause getConfiguredNetworkInterfaceName to fail

    auto server_result = RpcAgentServer::startService();
    ASSERT_TRUE(server_result.ok());
    auto server = server_result.value();

    MockTransportSupportAgent* mock_tsa =
            static_cast<MockTransportSupportAgent*>(g_mock_transport_agent.get());
    EXPECT_CALL(*mock_tsa, setAgentMetadata(::testing::_, ::testing::_)).Times(0);

    ::aidl::google::sdv::service_discovery::common::RegistrationToken token;
    token.token = g_mock_transport_agent->asBinder();
    ::ndk::ScopedFileDescriptor fd;

    ::ndk::ScopedAStatus status = server->registerServer(token, &fd);

    ASSERT_FALSE(status.isOk());
    ASSERT_EQ(status.getServiceSpecificError(), 10002);  // CANT_GET_NETWORK_INTERFACE_PROPERTY
    ASSERT_THAT(status.getMessage(), ::testing::HasSubstr("Could not determine local IP"));
}

TEST_F(RpcAgentServerTest, RegisterServer_SetAgentMetadataFails) {
    g_mock_prop_info = &g_dummy_prop_info;
    g_mock_prop_value = "sdv_rpc";

    auto server_result = RpcAgentServer::startService();
    ASSERT_TRUE(server_result.ok());
    auto server = server_result.value();

    ::aidl::google::sdv::service_discovery::common::RegistrationToken token;
    token.token = g_mock_transport_agent->asBinder();

    MockTransportSupportAgent* mock_tsa =
            static_cast<MockTransportSupportAgent*>(g_mock_transport_agent.get());
    EXPECT_CALL(*mock_tsa, setAgentMetadata(::testing::_, ::testing::_))
            .WillOnce(::testing::Return(
                    ::ndk::ScopedAStatus::
                            fromExceptionCodeWithMessage(EACCES, "mock setAgentMetadata failed")));

    ::ndk::ScopedFileDescriptor fd;
    ::ndk::ScopedAStatus status = server->registerServer(token, &fd);

    ASSERT_FALSE(status.isOk());
    // The SUT currently uses the last `errno` which might not be EACCES.
    // It's safer to check the message.
    ASSERT_THAT(status.getMessage(), ::testing::HasSubstr("Failed to set metadata"));
}

TEST_F(RpcAgentServerTest, RegisterServerPort_AutoAssignSuccess) {
    g_mock_prop_info = &g_dummy_prop_info;
    g_mock_prop_value = "sdv_rpc";

    auto server_result = RpcAgentServer::startService();
    ASSERT_TRUE(server_result.ok());
    auto server = server_result.value();

    MockTransportSupportAgent* mock_tsa =
            static_cast<MockTransportSupportAgent*>(g_mock_transport_agent.get());
    EXPECT_CALL(*mock_tsa, setAgentMetadata(::testing::_, ::testing::_))
            .WillOnce(::testing::Return(::ndk::ScopedAStatus::ok()));

    ::aidl::google::sdv::service_discovery::common::RegistrationToken token;
    token.token = g_mock_transport_agent->asBinder();
    int32_t port = 0;

    ::ndk::ScopedAStatus status = server->registerServerPort(token, port);

    ASSERT_TRUE(status.isOk()) << status.getMessage();
}

TEST_F(RpcAgentServerTest, RegisterServerPort_SpecificPortSuccess) {
    g_mock_prop_info = &g_dummy_prop_info;
    g_mock_prop_value = "sdv_rpc";

    auto server_result = RpcAgentServer::startService();
    ASSERT_TRUE(server_result.ok());
    auto server = server_result.value();

    MockTransportSupportAgent* mock_tsa =
            static_cast<MockTransportSupportAgent*>(g_mock_transport_agent.get());
    EXPECT_CALL(*mock_tsa, setAgentMetadata(::testing::_, ::testing::_))
            .WillOnce(::testing::Return(::ndk::ScopedAStatus::ok()));

    ::aidl::google::sdv::service_discovery::common::RegistrationToken token;
    token.token = g_mock_transport_agent->asBinder();
    int32_t port = 12345;

    ::ndk::ScopedAStatus status = server->registerServerPort(token, port);

    ASSERT_TRUE(status.isOk()) << status.getMessage();
}

// Helper to create a listening socket for testing connect()
android::base::unique_fd create_listening_socket_for_test(uint16_t* port) {
    android::base::unique_fd listen_fd(socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0));
    if (listen_fd.get() == -1) {
        fprintf(stderr, "Failed to create listening socket in test helper\n");
        return android::base::unique_fd();
    }

    int opt = 1;
    if (setsockopt(listen_fd.get(), SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt)) < 0) {
        return android::base::unique_fd();  // listen_fd will be closed by unique_fd destructor
    }

    struct sockaddr_in serv_addr;
    memset(&serv_addr, 0, sizeof(serv_addr));
    serv_addr.sin_family = AF_INET;
    serv_addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);  // Listen on 127.0.0.1
    serv_addr.sin_port = 0;                              // Auto-assign port

    if (bind(listen_fd.get(), (struct sockaddr*)&serv_addr, sizeof(serv_addr)) < 0) {
        return android::base::unique_fd();
    }

    if (listen(listen_fd.get(), 1) < 0) {
        return android::base::unique_fd();
    }

    socklen_t len = sizeof(serv_addr);
    if (getsockname(listen_fd.get(), (struct sockaddr*)&serv_addr, &len) == -1) {
        return android::base::unique_fd();
    }
    *port = ntohs(serv_addr.sin_port);
    return listen_fd;
}

TEST_F(RpcAgentServerTest, GetFdConnection_Success) {
    auto server_result = RpcAgentServer::startService();
    ASSERT_TRUE(server_result.ok());
    auto server = server_result.value();

    uint16_t listening_port = 0;
    android::base::unique_fd listen_fd(create_listening_socket_for_test(&listening_port));
    ASSERT_NE(listen_fd.get(), -1)
            << "Test setup: Failed to create listening socket: " << strerror(errno);
    ASSERT_NE(listening_port, 0) << "Test setup: Listening port is 0";

    MockTransportSupportAgent* mock_tsa =
            static_cast<MockTransportSupportAgent*>(g_mock_transport_agent.get());
    std::string server_address_str = "127.0.0.1:" + std::to_string(listening_port);
    std::vector<uint8_t> metadata_value_vec(server_address_str.begin(), server_address_str.end());

    EXPECT_CALL(*mock_tsa, getAgentMetadata(::testing::_, ::testing::_, ::testing::_, ::testing::_))
            .WillOnce(::testing::DoAll(::testing::SetArgPointee<3>(
                                               ::aidl::google::sdv::service_discovery::common::
                                                       metadata::TransportMetadata{
                                                               .value_holder = metadata_value_vec}),
                                       ::testing::Return(::ndk::ScopedAStatus::ok())));

    int64_t sid = 123;
    std::string unitName = "testUnit";
    ::ndk::ScopedFileDescriptor connected_fd;

    ::ndk::ScopedAStatus status = server->getFdConnection(sid, unitName, &connected_fd);

    ASSERT_TRUE(status.isOk()) << status.getMessage();
    ASSERT_NE(connected_fd.get(), -1);

    // To be more robust, accept the connection to ensure it was fully established.
    // This is important because the SUT's connect logic for non-blocking sockets
    // might incorrectly treat EINPROGRESS as immediate success.
    struct sockaddr_in client_addr;
    socklen_t client_len = sizeof(client_addr);
    android::base::unique_fd accepted_fd(
            accept(listen_fd.get(), (struct sockaddr*)&client_addr, &client_len));
    ASSERT_NE(accepted_fd.get(), -1)
            << "Test validation: Failed to accept connection on listening socket: "
            << strerror(errno);

    // Teardown: listen_fd, connected_fd, accepted_fd are closed by unique_fd destructors
}

TEST_F(RpcAgentServerTest, GetFdConnection_GetAgentMetadataFails) {
    auto server_result = RpcAgentServer::startService();
    ASSERT_TRUE(server_result.ok());
    auto server = server_result.value();

    MockTransportSupportAgent* mock_tsa =
            static_cast<MockTransportSupportAgent*>(g_mock_transport_agent.get());

    EXPECT_CALL(*mock_tsa, getAgentMetadata(::testing::_, ::testing::_, ::testing::_, ::testing::_))
            .WillOnce(::testing::Return(
                    ::ndk::ScopedAStatus::fromExceptionCodeWithMessage(-1, "mock metadata error")));

    int64_t sid = 123;
    std::string unitName = "testUnit";
    ::ndk::ScopedFileDescriptor connected_fd;

    ::ndk::ScopedAStatus status = server->getFdConnection(sid, unitName, &connected_fd);

    ASSERT_FALSE(status.isOk());
    ASSERT_THAT(status.getExceptionCode(), -1);
    ASSERT_THAT(status.getMessage(), ::testing::HasSubstr("mock metadata error"));
    ASSERT_EQ(connected_fd.get(), -1);
}

TEST_F(RpcAgentServerTest, GetFdConnection_InvalidMetadataFormat) {
    auto server_result = RpcAgentServer::startService();
    ASSERT_TRUE(server_result.ok());
    auto server = server_result.value();

    MockTransportSupportAgent* mock_tsa =
            static_cast<MockTransportSupportAgent*>(g_mock_transport_agent.get());

    std::string bad_metadata_str = "127.0.0.1_badport";  // Invalid format
    std::vector<uint8_t> metadata_value_vec(bad_metadata_str.begin(), bad_metadata_str.end());

    EXPECT_CALL(*mock_tsa, getAgentMetadata(::testing::_, ::testing::_, ::testing::_, ::testing::_))
            .WillOnce(::testing::DoAll(::testing::SetArgPointee<3>(
                                               ::aidl::google::sdv::service_discovery::common::
                                                       metadata::TransportMetadata{
                                                               .value_holder = metadata_value_vec}),
                                       ::testing::Return(::ndk::ScopedAStatus::ok())));

    int64_t sid = 123;
    std::string unitName = "testUnit";
    ::ndk::ScopedFileDescriptor connected_fd;

    ::ndk::ScopedAStatus status = server->getFdConnection(sid, unitName, &connected_fd);

    ASSERT_FALSE(status.isOk());
    ASSERT_EQ(status.getServiceSpecificError(), EINVAL);  // From extractAddress
    ASSERT_THAT(status.getMessage(), ::testing::HasSubstr("Incorrect rpcmetadata format"));
    ASSERT_EQ(connected_fd.get(), -1);
}

TEST_F(RpcAgentServerTest, GetAddressConnection_Success) {
    auto server_result = RpcAgentServer::startService();
    ASSERT_TRUE(server_result.ok());
    auto server = server_result.value();

    MockTransportSupportAgent* mock_tsa =
            static_cast<MockTransportSupportAgent*>(g_mock_transport_agent.get());

    std::string expected_ip = "192.168.1.100";
    int expected_port = 8080;
    std::string metadata_value_str = expected_ip + ":" + std::to_string(expected_port);
    std::vector<uint8_t> metadata_value_vec(metadata_value_str.begin(), metadata_value_str.end());

    EXPECT_CALL(*mock_tsa, getAgentMetadata(::testing::_, ::testing::_, ::testing::_, ::testing::_))
            .WillOnce(::testing::DoAll(::testing::SetArgPointee<3>(
                                               ::aidl::google::sdv::service_discovery::common::
                                                       metadata::TransportMetadata{
                                                               .value_holder = metadata_value_vec}),
                                       ::testing::Return(::ndk::ScopedAStatus::ok())));

    int64_t sid = 123;
    std::string unitName = "testUnit";
    std::string actual_address;

    ::ndk::ScopedAStatus status = server->getAddressConnection(sid, unitName, &actual_address);

    ASSERT_TRUE(status.isOk()) << status.getMessage();
    ASSERT_EQ(actual_address, metadata_value_str);
}

TEST_F(RpcAgentServerTest, GetAddressConnection_FailsOnNullptr) {
    auto server_result = RpcAgentServer::startService();
    ASSERT_TRUE(server_result.ok());
    auto server = server_result.value();

    MockTransportSupportAgent* mock_tsa =
            static_cast<MockTransportSupportAgent*>(g_mock_transport_agent.get());

    std::string expected_ip = "192.168.1.100";
    int expected_port = 8080;
    std::string metadata_value_str = expected_ip + ":" + std::to_string(expected_port);
    std::vector<uint8_t> metadata_value_vec(metadata_value_str.begin(), metadata_value_str.end());

    EXPECT_CALL(*mock_tsa, getAgentMetadata(::testing::_, ::testing::_, ::testing::_, ::testing::_))
            .WillOnce(::testing::DoAll(::testing::SetArgPointee<3>(
                                               ::aidl::google::sdv::service_discovery::common::
                                                       metadata::TransportMetadata{
                                                               .value_holder = metadata_value_vec}),
                                       ::testing::Return(::ndk::ScopedAStatus::ok())));

    int64_t sid = 123;
    std::string unitName = "testUnit";

    ::ndk::ScopedAStatus status = server->getAddressConnection(sid, unitName, nullptr);

    ASSERT_FALSE(status.isOk());
    ASSERT_THAT(status.getMessage(), ::testing::HasSubstr("Address pointer is null"));
}

// Currently disabled as the following provokes a `stack-protector` issue.
// Has to be investigated.
TEST_F(RpcAgentServerTest, GetAddressConnection_GetAgentMetadataFails) {
    auto server_result = RpcAgentServer::startService();
    ASSERT_TRUE(server_result.ok());
    auto server = server_result.value();

    MockTransportSupportAgent* mock_tsa =
            static_cast<MockTransportSupportAgent*>(g_mock_transport_agent.get());

    ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition out_sud;
    ::aidl::google::sdv::service_discovery::common::metadata::TransportMetadata out_agentMetadata;
    EXPECT_CALL(*mock_tsa, getAgentMetadata(::testing::_, ::testing::_, ::testing::_, ::testing::_))
            .WillOnce(::testing::Return(
                    ::ndk::ScopedAStatus::fromExceptionCodeWithMessage(-1, "mock metadata error")));

    int64_t sid = 123;
    std::string unitName = "testUnit";
    std::string actual_address;

    ::ndk::ScopedAStatus status = server->getAddressConnection(sid, unitName, &actual_address);

    ASSERT_FALSE(status.isOk());
    ASSERT_THAT(status.getMessage(), ::testing::HasSubstr("mock metadata error"));
    ASSERT_EQ(actual_address, "");
}

}  // namespace
}  // namespace android::sdv::rpc
