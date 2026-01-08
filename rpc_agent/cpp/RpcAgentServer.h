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

#pragma once

#include <aidl/google/sdv/rpc/BnRpcAgent.h>
#include <aidl/google/sdv/service_discovery/discovery/ITransportSupportAgent.h>
#include <android-base/expected.h>
#include <android-base/result.h>

namespace android::sdv::rpc {

class RpcAgentServer final : public ::aidl::google::sdv::rpc::BnRpcAgent {
public:
    static base::Result<std::shared_ptr<RpcAgentServer>> startService();
    static void terminateService();

    // Implements IRpcAgent.aidl.
    ::ndk::ScopedAStatus registerServer(
            const ::aidl::google::sdv::service_discovery::common::RegistrationToken& token,
            ::ndk::ScopedFileDescriptor* fd) override;
    ::ndk::ScopedAStatus registerServerPort(
            const ::aidl::google::sdv::service_discovery::common::RegistrationToken& token,
            int32_t port) override;
    ::ndk::ScopedAStatus getFdConnection(int64_t sid, const std::string& unitName,
                                         ::ndk::ScopedFileDescriptor* fd) override;
    ::ndk::ScopedAStatus getAddressConnection(int64_t sid, const std::string& unitName,
                                              std::string* address) override;
    ::ndk::ScopedAStatus getNetworkInterfaceName(std::string* iface) override;

private:
    enum class GET_IP_ERROR : int32_t {
        // The large number to avoid collision with `errno` that has max value of 133
        CANT_DETERMINE_IP = 10001,
        CANT_GET_NETWORK_INTERFACE_PROPERTY = 10002,
        NETWORK_INTERFACE_PROPERTY_NOT_SET = 10003,
    };

    // A struct to represents a network interface made up of a name, and an IP address assigned to
    // this interface. It is used to identify an available local interface on the device. Interfaces
    // are normally known by names such as "le0" and can be assigned multiple IP addresses. For
    // RpcAgent's purposes, we assume that the first IP address returned by inet_ntoa() is the
    // default address on which to open/accept socket connections. These values are configured at
    // compile time for RpcAgent and not expected to change during runtime.
    struct NetworkIfaceDescriptor {
        std::string iFaceName;
        std::string ipAddress;

        // The network interface is valid if its name is not empty and it has an assigned IP
        // address. These values are configured at compile time for RpcAgent and not expected to
        // change during runtime, but could be invalid ephemerally if the network is brought down.
        bool valid() { return (!iFaceName.empty() && !ipAddress.empty()); }
    };

    friend class ::ndk::SharedRefBase;

    RpcAgentServer();
    android::base::Result<void> init();
    void terminate();

    android::base::expected<NetworkIfaceDescriptor, GET_IP_ERROR> getNetworkInterface();
    static android::base::expected<std::string, GET_IP_ERROR> getConfiguredNetworkInterfaceName();

    static std::shared_ptr<RpcAgentServer> sRpcAgentServer;
    std::shared_ptr<::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent>
            mTransportSupportAgent;
    NetworkIfaceDescriptor mNetworkInterface;
};

}  // namespace android::sdv::rpc
