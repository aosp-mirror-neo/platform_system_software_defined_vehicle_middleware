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

#pragma once

#include "common.h"

#include <netinet/in.h>

#include <functional>

#include <sdv/status.hpp>

namespace android::sdv::dt::agent {

class IPAddressV4 {
public:
    // TODO: Add an extra argument to specify acceptable network interface(s)
    static ::sdv::SdvResult<IPAddressV4> GetLocal();

    IPAddressV4() {}
    IPAddressV4(const in_addr& addr) { memcpy(&addr_, &addr, sizeof(in_addr)); }

    void Serialize(std::vector<uint8_t>& buffer) const;
    size_t Deserialize(const uint8_t* buffer, size_t buffer_size);

    operator const in_addr&() const { return addr_; }
    operator const in_addr_t&() const { return addr_.s_addr; }

    std::string ToString() const;

private:
    in_addr addr_;
};

class SocketAddressV4 {
public:
    SocketAddressV4() {
        memset(&sock_addr_, 0, sizeof(sock_addr_));
        sock_addr_.sin_family = AF_INET;
    }
    SocketAddressV4(const in_addr& addr, uint16_t port) {
        memset(&sock_addr_, 0, sizeof(sock_addr_));
        sock_addr_.sin_family = AF_INET;
        sock_addr_.sin_addr = addr;
        sock_addr_.sin_port = htons(port);
    }

    SocketAddressV4(const sockaddr_in& sock_addr) {
        sock_addr_.sin_family = AF_INET;
        sock_addr_.sin_addr = sock_addr.sin_addr;
        sock_addr_.sin_port = sock_addr.sin_port;
    }

    void Serialize(std::vector<uint8_t>& buffer) const;
    size_t Deserialize(const uint8_t* buffer, size_t buffer_size);

    bool operator==(const SocketAddressV4& other) const {
        return sock_addr_.sin_family == other.sock_addr_.sin_family &&
                sock_addr_.sin_addr.s_addr == other.sock_addr_.sin_addr.s_addr;
    }
    bool operator!=(const SocketAddressV4& other) const { return !(*this == other); }

    operator const sockaddr_in&() const { return sock_addr_; }

    std::string ToString() const;

private:
    sockaddr_in sock_addr_;
};

class IConnection {
public:
    virtual ~IConnection() = default;

    // Disconnect terminates the connection.  It may block and will not return until the disconnect
    // is complete.
    //
    // It is not required to call this function before destroying the connection, but doing so will
    // guarantee that destruction does not block.
    virtual void Disconnect() = 0;

    // Non-blocking request to initiate disconnection.
    //
    // Calling this will cause any in-progress (and subsequent) SendData and ReceiveData operations
    // to fail.
    //
    // Calling this does not guarantee that the destructor won't block.
    virtual void RequestDisconnect() = 0;

    virtual bool SendData(const void* buffer, size_t bytes_to_send) = 0;
    virtual size_t ReceiveData(void* buffer, size_t buffer_size) = 0;
    virtual std::string ToString() const = 0;
    // For dumpsys stats
    virtual size_t BytesSent() const = 0;
    virtual size_t BytesReceived() const = 0;
    virtual std::optional<std::string> AuthenticatedVmName() const = 0;
    virtual std::optional<std::string> AuthenticatedIdentity() const = 0;
};

// IConnectionHolder serves as a RAII holder for any IConnection returned from ConnectionManager to
// that class's clients.
//
// It provides mostly the same interface as std::unique_ptr (minus release()), and by default
// behaves the same, deleting the managed IConnection when the IConnectionHolder is destroyed, or on
// a call to reset().
//
// But it allows an IConnection implementation to override this default behavior and instead
// retake ownership of the connection once the client code is done with it, without the client
// application needing to make any explicit calls to make that happen.
//
// This is useful when there are blocking operations needed to shut down a connection that have to
// happen before the IConnection can be destroyed: Passing ownership back to the IConnection
// implementation to handle those operations means the client application need not wait on them.
class IConnectionHolder : public std::unique_ptr<IConnection, std::function<void(IConnection*)>> {
public:
    using Releaser = std::function<void(IConnection*)>;
    IConnectionHolder(IConnection* conn = nullptr,
                      Releaser releaser = std::default_delete<IConnection>()) :
          std::unique_ptr<IConnection, Releaser>(conn, std::move(releaser)) {}
    IConnection* release() = delete;
};

// Interface to access trusted root certificates to check peer certs.
struct TrustedRootsAccessor {
    virtual ~TrustedRootsAccessor() = default;
    virtual std::string GetAllTrustedRootsAsPEM() const = 0;
    virtual bool IsSignedByTrustedRoot(std::string_view leaf_cert_pem) const = 0;
};

class ConnectionManager {
public:
    // TODO: Replace EventHandler interface with std::function<> callback
    struct EventHandler {
        virtual ~EventHandler() = default;
        virtual void OnConnectionManagerEvent(::sdv::SdvResult<IConnectionHolder> connection) = 0;
    };

    // Ssl Configuration
    struct SslConfig {
        // Trusted root certificate accessor for validating peer certificates.
        // This instance must outlive any ConnectionManager created with the SslConfig.
        const TrustedRootsAccessor& trusted_roots_accessor;

        // PEM-formatted certificate to be used to authenticate this agent to peers.
        std::string pem_cert_chain;

        // PEM-formatted private key to be used to authenticate this agent to peers.
        std::string pem_private_key;
    };

    uint16_t GetListenPort() const { return listen_port_; }
    const IPAddressV4& GetLocalIp() const { return local_ip_; }
    virtual ~ConnectionManager() = default;
    virtual void Shutdown() = 0;

    virtual bool Listen(EventHandler& event_handler) = 0;

    // addr: The raw IP / socket to connect to.
    // target_name: Expected name of the target we're connecting to, required for peer
    // authentication when running with ssl enabled.
    virtual void Connect(const SocketAddressV4& addr, const std::string& target_name,
                         EventHandler& event_handler) = 0;

protected:
    ConnectionManager(const IPAddressV4& local_ip) : local_ip_(local_ip), listen_port_(0) {}

    // TODO: get rid of non-const protected members
protected:
    IPAddressV4 local_ip_;
    uint16_t listen_port_;
};

using ConnectionManagerFactory = std::function<std::unique_ptr<
        ConnectionManager>(IPAddressV4, std::optional<ConnectionManager::SslConfig> ssl_config)>;

}  // namespace android::sdv::dt::agent
