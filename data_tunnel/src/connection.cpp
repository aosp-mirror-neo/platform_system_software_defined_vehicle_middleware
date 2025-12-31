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

#define LOG_TAG "sdv_dt_agent:conn_grpc"

#include "connection.h"

#include <arpa/inet.h>
#include <log/log.h>
#include <net/if.h>
#include <netinet/in.h>
#include <netinet/tcp.h>

#include <ifaddrs.h>
#include <memory.h>

#include <cstdio>

namespace android::sdv::dt::agent {

static_assert(sizeof(in_addr) == 4, "size of in_addr structure must be 4 bytes");

/**
 * IPAddressV4 impl
 */
::sdv::SdvResult<IPAddressV4> IPAddressV4::GetLocal() {
    struct ifaddrs* ifAddrStruct = NULL;
    if (getifaddrs(&ifAddrStruct) != 0) {
        return base::unexpected<::sdv::SdvStatus>(
                ::sdv::SdvStatus(::sdv::SdvStatusCode::kInternal,
                                 std::format("getifaddrs() failed with errno {}", errno)));
    }

    for (auto ifa = ifAddrStruct; ifa != NULL; ifa = ifa->ifa_next) {
        if (!ifa->ifa_addr) continue;
        if (ifa->ifa_addr->sa_family == AF_INET) {
            auto addr_ptr = &((struct sockaddr_in*)ifa->ifa_addr)->sin_addr;
            IPAddressV4 result(*addr_ptr);
            freeifaddrs(ifAddrStruct);
            return result;
        }
    }

    if (ifAddrStruct != NULL) freeifaddrs(ifAddrStruct);
    return base::unexpected<::sdv::SdvStatus>(::sdv::SdvStatusCode::kNotFound);
}

void Serialize_in_addr(const in_addr& addr, std::vector<uint8_t>& buffer) {
    // in_addr_t (the internal representation of IP address) must be 4-byte size
    // and stores IPv4 address in network (big-endian) byte order
    const uint8_t* ptr = reinterpret_cast<const uint8_t*>(&addr);
    for (auto i = 0; i < 4; ++i) buffer.push_back(ptr[i]);
}

size_t Deserialize_in_addr(in_addr& addr, const uint8_t* buffer, size_t buffer_size) {
    // in_addr_t (the internal representation of IP address) must be 4-byte size
    // and stores IPv4 address in network (big-endian) byte order
    if (buffer_size < 4) return 0;
    memcpy(&addr, buffer, 4);
    return 4;
}

void IPAddressV4::Serialize(std::vector<uint8_t>& buffer) const {
    return Serialize_in_addr(addr_, buffer);
}

size_t IPAddressV4::Deserialize(const uint8_t* buffer, size_t buffer_size) {
    return Deserialize_in_addr(addr_, buffer, buffer_size);
}

std::string IPAddressV4::ToString() const {
    char addr_buffer[INET_ADDRSTRLEN];
    inet_ntop(AF_INET, &addr_, addr_buffer, INET_ADDRSTRLEN);
    return std::string(addr_buffer);
}

/**
 * SocketAddressV4 impl
 */
std::string SocketAddressV4::ToString() const {
    return std::format("{}:{}", IPAddressV4(sock_addr_.sin_addr).ToString(),
                       ntohs(sock_addr_.sin_port));
}

void Serialize_sockaddr_in(const sockaddr_in& addr, std::vector<uint8_t>& buffer) {
    Serialize_in_addr(addr.sin_addr, buffer);
    const uint8_t* ptr = reinterpret_cast<const uint8_t*>(&addr.sin_port);
    for (auto i = 0; i < 2; ++i) buffer.push_back(ptr[i]);
}

size_t Deserialize_sockaddr_in(sockaddr_in& addr, const uint8_t* buffer, size_t buffer_size) {
    auto read = Deserialize_in_addr(addr.sin_addr, buffer, buffer_size);
    if (!read) return 0;
    buffer += read;
    buffer_size -= read;
    if (buffer_size < 2) return 0;
    memcpy(&addr.sin_port, buffer, 2);
    return read + 2;
}

void SocketAddressV4::Serialize(std::vector<uint8_t>& buffer) const {
    return Serialize_sockaddr_in(sock_addr_, buffer);
}

size_t SocketAddressV4::Deserialize(const uint8_t* buffer, size_t buffer_size) {
    return Deserialize_sockaddr_in(sock_addr_, buffer, buffer_size);
}

}  // namespace android::sdv::dt::agent
