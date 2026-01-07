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

#define LOG_TAG "RpcAgent"

#include "RpcAgentServer.h"

#include <aidl/google/sdv/service_discovery/common/RegistrationToken.h>
#include <aidl/google/sdv/service_discovery/common/metadata/TransportMetadata.h>
#include <android-base/file.h>
#include <android-base/logging.h>
#include <android/binder_ibinder.h>
#include <android/binder_manager.h>
#include <android/binder_status.h>
#include <arpa/inet.h>
#include <net/if.h>
#include <netinet/in.h>
#include <netinet/tcp.h>

#include <sys/poll.h>
#include <sys/socket.h>
#include <sys/system_properties.h>

namespace {

using ::aidl::google::sdv::rpc::IRpcAgent;
using ::aidl::google::sdv::service_discovery::common::RegistrationToken;
using ::aidl::google::sdv::service_discovery::common::metadata::TransportMetadata;
using ::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent;
using ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition;
using ::android::base::unique_fd;
using ::ndk::ScopedAStatus;
using ::ndk::ScopedFileDescriptor;
using ::ndk::SpAIBinder;

struct ServiceUnitMetadataRequest {
    int64_t sid;
    std::string unitName;
};

const char* NETWORK_INTERFACE_PROPERTY_NAME = "ro.boot.sdv.rpc.interface";

// Registers the RPC server address in Service Discovery.
bool registerService(const std::string& address, ITransportSupportAgent& transportSupportAgent,
                     const RegistrationToken& token) {
    TransportMetadata transport_metadata;
    transport_metadata.value_holder.assign(address.begin(), address.end());

    return transportSupportAgent.setAgentMetadata(token, transport_metadata).isOk();
}

struct Address {
    std::string ip;
    int port;

    std::string to_string() { return ip + ":" + std::to_string(port); }
};

android::base::expected<Address, ndk::ScopedAStatus> extractAddress(char* data, size_t size) {
    std::stringstream ss;
    ss.write(data, size);
    std::string str = ss.str();

    std::size_t pos = str.find(":");
    if (pos == std::string::npos || pos == 0 || pos == str.length() - 1) {
        LOG(ERROR) << "Invalid rpcagent metadata format";
        return android::base::unexpected(
                ndk::ScopedAStatus::
                        fromServiceSpecificErrorWithMessage(EINVAL,
                                                            "Incorrect rpcmetadata format"));
    }

    const std::string ip_address = str.substr(0, pos);
    int port;
    port = atoi(str.c_str() + pos + 1);
    if (port == 0) {
        LOG(ERROR) << "Invalid rpcagent metadata port number";
        return android::base::unexpected(
                ndk::ScopedAStatus::fromServiceSpecificErrorWithMessage(EINVAL,
                                                                        "Incorrect port number "
                                                                        "in rpcmetadata"));
    }

    return Address{ip_address, port};
}

// Retrieves the address of the RPC server from Service Discovery.
android::base::expected<Address, ndk::ScopedAStatus> getServiceAddress(
        ITransportSupportAgent& transportSupportAgent,
        const ServiceUnitMetadataRequest& metadataRequest) {
    ServiceUnitDefinition sud;
    TransportMetadata tm;

    const auto& sid = metadataRequest.sid;
    const auto& unitName = metadataRequest.unitName;

    if (auto status = transportSupportAgent.getAgentMetadata(sid, unitName, &sud, &tm);
        !status.isOk()) {
        LOG(ERROR) << status.getDescription();
        return android::base::unexpected(std::move(status));
    };

    return extractAddress((char*)tm.value_holder.data(), tm.value_holder.size());
}

}  // namespace

namespace android::sdv::rpc {

// Returns the configured interface name. If the system property cannot be read,
// an error is returned.
android::base::expected<std::string, RpcAgentServer::GET_IP_ERROR>
RpcAgentServer::getConfiguredNetworkInterfaceName() {
    const prop_info* property = __system_property_find(NETWORK_INTERFACE_PROPERTY_NAME);
    if (property == nullptr) {
        LOG(ERROR) << "Unable to retrieve system property, " << NETWORK_INTERFACE_PROPERTY_NAME;
        return android::base::unexpected(GET_IP_ERROR::CANT_GET_NETWORK_INTERFACE_PROPERTY);
    }

    std::string property_value;
    __system_property_read_callback(
            property,
            [](void* cookie, const char*, const char* value, unsigned) {
                auto property_value = reinterpret_cast<std::string*>(cookie);
                *property_value = value;
            },
            &property_value);

    if (property_value.empty()) {
        LOG(ERROR) << NETWORK_INTERFACE_PROPERTY_NAME << " system property is not set";
        return android::base::unexpected(GET_IP_ERROR::NETWORK_INTERFACE_PROPERTY_NOT_SET);
    }

    LOG(INFO) << "Configured interface name: " << property_value;

    return property_value;
}

std::shared_ptr<RpcAgentServer> RpcAgentServer::sRpcAgentServer = nullptr;

base::Result<std::shared_ptr<RpcAgentServer>> RpcAgentServer::startService() {
    if (sRpcAgentServer != nullptr) {
        return ::android::base::Error(INVALID_OPERATION) << "Cannot start service more than once";
    }
    std::shared_ptr<RpcAgentServer> server = SharedRefBase::make<RpcAgentServer>();
    const auto& ret = server->init();
    if (!ret.ok()) {
        return ::android::base::Error(ret.error().code())
                << "Failed to start rpcagent server: " << ret.error();
    }
    sRpcAgentServer = server;

    return sRpcAgentServer;
}

void RpcAgentServer::terminateService() {
    if (sRpcAgentServer != nullptr) {
        sRpcAgentServer->terminate();
        sRpcAgentServer = nullptr;
    }
}

void RpcAgentServer::terminate() {}

base::Result<void> RpcAgentServer::init() {
    {
        const auto descriptor = std::string(ITransportSupportAgent::descriptor) + "/default";
        AIBinder* service = AServiceManager_waitForService(descriptor.data());
        if (service == nullptr) {
            return ::android::base::Error(INVALID_OPERATION)
                    << "Unable to waitForService " << descriptor.data();
        }
        mTransportSupportAgent = ITransportSupportAgent::fromBinder(ndk::SpAIBinder(service));
        CHECK(mTransportSupportAgent != nullptr) << "Can't fetch ITransportSupportAgent";
    }

    const auto instanceName = std::string(IRpcAgent::descriptor) + "/default";
    binder_exception_t err =
            AServiceManager_addService(this->asBinder().get(), instanceName.data());
    if (err != EX_NONE) {
        return ::android::base::Error(err) << "Failed to add rpcagent to ServiceManager";
    }

    return {};
}

::ndk::ScopedAStatus RpcAgentServer::registerServer(const RegistrationToken& registrationToken,
                                                    ::ndk::ScopedFileDescriptor* fd) {
    unique_fd socket_fd(
            TEMP_FAILURE_RETRY(socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC | SOCK_NONBLOCK, 0)));
    if (!socket_fd.ok()) {
        LOG(ERROR) << "Could not create socket";
        return ScopedAStatus::fromServiceSpecificErrorWithMessage(errno, "Could not create socket");
    }

    android::base::expected<NetworkIfaceDescriptor, GET_IP_ERROR> network_iface_result =
            getNetworkInterface();
    if (!network_iface_result.has_value()) {
        return ScopedAStatus::fromServiceSpecificErrorWithMessage(static_cast<int32_t>(
                                                                          network_iface_result
                                                                                  .error()),
                                                                  "Could not determine local IP");
    }
    const std::string localipaddr = network_iface_result.value().ipAddress;
    LOG(INFO) << "Local Ip: " << localipaddr;

    struct sockaddr_in addr;
    addr.sin_family = AF_INET;
    addr.sin_port = 0;  // auto assign port
    addr.sin_addr.s_addr = inet_addr(localipaddr.c_str());
    if (TEMP_FAILURE_RETRY(bind(socket_fd.get(), (struct sockaddr*)&addr, sizeof(addr))) < 0) {
        LOG(ERROR) << "Could not bind socket";
        return ScopedAStatus::fromServiceSpecificErrorWithMessage(errno, "Could not bind socket");
    }

    socklen_t len = sizeof(addr);
    if (0 != getsockname(socket_fd.get(), (struct sockaddr*)&addr, &len)) {
        LOG(ERROR) << "Could not getsockname";
        return ScopedAStatus::fromServiceSpecificErrorWithMessage(errno, "Could not getsockname");
    }

    // Start listening before registration in SD, because after registration in SD server
    // becomes visible and there can be incoming connections from clients.
    if (0 != TEMP_FAILURE_RETRY(listen(socket_fd.get(), 50))) {
        int savedErrno = errno;
        LOG(ERROR) << "Could not listen initialized Unix socket: " << strerror(savedErrno);
        return ScopedAStatus::fromServiceSpecificErrorWithMessage(errno, "Could not listen");
    }

    unsigned int realPort = ntohs(addr.sin_port);

    std::string fullAddr = localipaddr;
    fullAddr += ":";
    fullAddr += std::to_string(realPort);

    bool isRegistrationOk = registerService(fullAddr, *mTransportSupportAgent, registrationToken);
    if (!isRegistrationOk) {
        LOG(ERROR) << "Failed to set metadata";
        return ScopedAStatus::fromServiceSpecificErrorWithMessage(errno, "Failed to set metadata");
    }

    fd->set(socket_fd.release());

    return ScopedAStatus::ok();
}

::ndk::ScopedAStatus RpcAgentServer::getAddressConnection(int64_t sid, const std::string& unitName,
                                                          std::string* address) {
    auto obtainedAddress =
            getServiceAddress(*mTransportSupportAgent, ServiceUnitMetadataRequest{sid, unitName});
    if (!obtainedAddress.has_value()) {
        LOG(WARNING) << "Cannot obtain address of the server";
        return std::move(obtainedAddress.error());
    }
    if (!address) {
        return ScopedAStatus::fromServiceSpecificErrorWithMessage(-1, "Address pointer is null");
    }
    *address = obtainedAddress.value().to_string();
    return ScopedAStatus::ok();
}

::ndk::ScopedAStatus RpcAgentServer::registerServerPort(const RegistrationToken& registrationToken,
                                                        int32_t port) {
    android::base::expected<NetworkIfaceDescriptor, GET_IP_ERROR> network_iface_result =
            getNetworkInterface();
    if (!network_iface_result.has_value()) {
        return ScopedAStatus::fromServiceSpecificErrorWithMessage(static_cast<int32_t>(
                                                                          network_iface_result
                                                                                  .error()),
                                                                  "Could not determine IP");
    }
    const std::string localipaddr = network_iface_result.value().ipAddress;
    LOG(INFO) << "Local Ip: " << localipaddr;

    std::string fullAddr = localipaddr;
    fullAddr += ":";
    fullAddr += std::to_string(port);

    bool isRegistrationOk = registerService(fullAddr, *mTransportSupportAgent, registrationToken);
    if (!isRegistrationOk) {
        LOG(ERROR) << "Failed to set metadata";
        return ScopedAStatus::fromServiceSpecificErrorWithMessage(errno, "Failed to set metadata");
    }

    return ScopedAStatus::ok();
}

::ndk::ScopedAStatus RpcAgentServer::getFdConnection(int64_t sid, const std::string& unitName,
                                                     ::ndk::ScopedFileDescriptor* fd) {
    auto obtainedAddress =
            getServiceAddress(*mTransportSupportAgent, ServiceUnitMetadataRequest{sid, unitName});
    if (!obtainedAddress.has_value()) {
        LOG(WARNING) << "Cannot obtain address of the server";
        return std::move(obtainedAddress.error());
    }

    // Get it from connection manager ?!
    unique_fd clientFd(
            TEMP_FAILURE_RETRY(socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC | SOCK_NONBLOCK, 0)));
    if (clientFd == -1) {
        return ScopedAStatus::fromServiceSpecificErrorWithMessage(errno, "Could not create socket");
    }

    int noDelay = 1;
    int result = setsockopt(clientFd.get(), IPPROTO_TCP, TCP_NODELAY, &noDelay, sizeof(noDelay));
    if (result < 0) {
        LOG(ERROR) << "Could not set TCP_NODELAY";
        return ScopedAStatus::fromServiceSpecificErrorWithMessage(errno,
                                                                  "Could not set TCP_NODELAY");
    }

    // Set the server address
    struct sockaddr_in servaddr;
    memset(&servaddr, 0, sizeof(servaddr));
    servaddr.sin_family = AF_INET;
    servaddr.sin_port = htons(obtainedAddress->port);
    servaddr.sin_addr.s_addr =
            inet_addr(obtainedAddress->ip.c_str());  // The IP address of the server

    if (0 !=
        TEMP_FAILURE_RETRY(
                connect(clientFd.get(), (struct sockaddr*)&servaddr, sizeof(servaddr)))) {
        int connErrno = errno;
        if (connErrno == EAGAIN || connErrno == EINPROGRESS) {
            connErrno = 0;
        }
        if (connErrno == ECONNRESET) {
            LOG(ERROR) << "Connection reset on";
            return ScopedAStatus::fromServiceSpecificErrorWithMessage(connErrno,
                                                                      "Connection reset on");
        }
        // connErrno could be zero if getsockopt determines so. Hence zero-check again.
        if (connErrno != 0) {
            LOG(FATAL) << "Could not connect socket at : " << strerror(connErrno);
            return ScopedAStatus::fromServiceSpecificErrorWithMessage(connErrno,
                                                                      "Could not connect socket");
        }
    }
    fd->set(clientFd.release());

    return ScopedAStatus::ok();
}

::ndk::ScopedAStatus RpcAgentServer::getNetworkInterfaceName(std::string* iface) {
    android::base::expected<std::string, GET_IP_ERROR> iface_name_result =
            getConfiguredNetworkInterfaceName();

    if (!iface_name_result.has_value()) {
        return ScopedAStatus::fromServiceSpecificErrorWithMessage(static_cast<int32_t>(
                                                                          iface_name_result
                                                                                  .error()),
                                                                  "Could not get the configured "
                                                                  "network interface name");
    }

    *iface = iface_name_result.value();
    return ScopedAStatus::ok();
}

android::base::expected<RpcAgentServer::NetworkIfaceDescriptor, RpcAgentServer::GET_IP_ERROR>
RpcAgentServer::getNetworkInterface() {
    // Use the cached network interface if already retrieved and valid.
    if (mNetworkInterface.valid()) {
        return mNetworkInterface;
    }

    // Retrieve the configured interface.
    struct ifreq ifr = {
            .ifr_addr.sa_family = AF_INET,
    };

    android::base::expected<std::string, GET_IP_ERROR> iface_name_result =
            getConfiguredNetworkInterfaceName();

    if (!iface_name_result.has_value()) {
        return android::base::unexpected(iface_name_result.error());
    }

    std::string iface = iface_name_result.value();
    int fd = socket(AF_INET, SOCK_DGRAM, 0);
    // IFNAMSIZ includes the null terminator. strncpy might not null-terminate if the source
    // string is as long as the destination. We must ensure null termination to prevent reads
    // past the end of the buffer by ioctl.
    strncpy(ifr.ifr_name, iface.c_str(), IFNAMSIZ);
    ifr.ifr_name[IFNAMSIZ - 1] = '\0';
    ioctl(fd, SIOCGIFADDR, &ifr);
    close(fd);

    std::string ip = inet_ntoa(((struct sockaddr_in*)&ifr.ifr_addr)->sin_addr);
    if (!ip.empty() && ip != "0.0.0.0") {
        LOG(DEBUG) << "Get " << ip << " from " << iface;
        LOG(INFO) << "SDVDEBUG: RpcAgent returns local Ip addr = " << ip;
        mNetworkInterface.iFaceName = iface;
        mNetworkInterface.ipAddress = ip;
        return mNetworkInterface;
    }

    LOG(ERROR) << "Cannot determine local ip";
    return android::base::unexpected(GET_IP_ERROR::CANT_DETERMINE_IP);
}

RpcAgentServer::RpcAgentServer() {}

}  // namespace android::sdv::rpc
