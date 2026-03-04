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

#include "../certs.h"
#include "../connection.h"
#include "operation_state_machine.h"
#include "read_data.h"

#include <android-base/logging.h>
#include <grpc/grpc.h>
#include <grpc/grpc_security_constants.h>
#include <grpcpp/channel.h>
#include <grpcpp/client_context.h>
#include <grpcpp/create_channel.h>
#include <grpcpp/generic/async_generic_service.h>
#include <grpcpp/generic/generic_stub.h>
#include <grpcpp/server_builder.h>
#include <grpcpp/support/client_callback.h>
#include <log/log.h>

#include <algorithm>
#include <chrono>
#include <future>
#include <memory>

namespace android::sdv::dt::agent {

// GrpcConnectionBase
//
// Grpc has different reactor interfaces for "client" vs "server" connections.
// (grpc::ClientBidiReactor vs grpc::ServerBidiReactor).
//
// These two reactor interfaces have identical methods for initiating reads and writes and receiving
// notice of completion of a read or write, and our logic for reading and writing a connection works
// the same for either the "Server" or "Client" end of a connection.
//
// So this class inherits from IConnection and either the client or server reactor interface,
// depending on the template argument, and partially implements each, using the reactor's methods
// and On*Done handlers to implement IConnection's SendData / ReceiveData.
//
// It also implements IConnection's AuthenticatedVmName and AuthenticatedIdentity methods, since
// those are mostly the same between client and server connections.
//
// Subclasses of this class are responsible for connection lifecycle operations, since those differ
// much more between client and server.
template <typename ReactorType>
class GrpcConnectionBase : public ReactorType, public IConnection {
public:
    GrpcConnectionBase() : write_state_("Write"), read_state_("Read") {}

    std::optional<std::string> AuthenticatedVmName() const {
        auto auth_context = GetAuthContext();
        if (!auth_context->IsPeerAuthenticated()) {
            return std::nullopt;
        }
        auto peer_chain_pems =
                auth_context->FindPropertyValues(GRPC_X509_PEM_CERT_CHAIN_PROPERTY_NAME);
        if (peer_chain_pems.size() != 1) {
            ALOGW("Got %zu peer root certs instead of expected 1.", peer_chain_pems.size());
            return std::nullopt;
        }
        return DecodedRootCNFromChainPEM(
                std::string_view(peer_chain_pems[0].data(), peer_chain_pems[0].length()));
    }

    // Returns the authenticated identity of the peer, if available.
    std::optional<std::string> AuthenticatedIdentity() const {
        auto auth_context = GetAuthContext();
        if (!auth_context->IsPeerAuthenticated()) {
            return std::nullopt;
        }
        auto peer_ids = auth_context->GetPeerIdentity();
        if (peer_ids.size() != 1) {
            ALOGW("Got %zu peer identities instead of expected 1!", peer_ids.size());
            return std::nullopt;
        }
        return std::string(peer_ids[0].data(), peer_ids[0].length());
    }

    // Implementation of IConnection::SendData
    virtual bool SendData(const void* buffer, size_t bytes_to_send) override {
        // Create a "static" (non-owning) slice from the provided data and use that
        // to create the buffer that Grpc will send from, then start the Grpc write.
        grpc::Slice slice(buffer, bytes_to_send, grpc::Slice::StaticSlice());
        grpc::ByteBuffer grpc_buffer(&slice, 1);

        auto guard = write_state_.Lock();
        auto grpc_result =
                write_state_.RunOperation(guard, [&]() { ReactorType::StartWrite(&grpc_buffer); });

        if (!grpc_result.has_value()) {
            ALOGE("%s.SendData(): In terminating state.  Returning failure.", ToString().c_str());
            return false;
        }

        if (*grpc_result) {
            bytes_written_ += bytes_to_send;
        } else {
            ALOGE("%s.OnWriteDone() got failure.  Terminating.", ToString().c_str());
            write_state_.Terminate(guard);
        }

        return *grpc_result;
    }

    virtual size_t BytesSent() const { return bytes_written_.load(); }

    // Implementation of IConnection::ReceiveData
    //
    // Grpc is message-oriented and doesn't let us limit how many bytes are
    // returned from a read operation -- each read will give us the block of
    // data from a single SendData() by our peer.
    //
    // So there's a possibility that buffer will not be large enough to return
    // all of the data from a single Grpc read.  So this method will only
    // initiate a new Grpc read if there's no leftover data from a previous
    // ReceiveData() to be returned.
    virtual size_t ReceiveData(void* buffer, size_t buffer_size) override {
        if (buffer_size == 0) {
            ALOGE("%s.ReceiveData: Illegal buffer size 0", ToString().c_str());
            return 0;
        }

        // We need to hold the lock for exclusive access to read_data_.
        auto guard = read_state_.Lock();

        if (!read_data_) {
            // Prepare buffer and start read.
            grpc::ByteBuffer grpc_buffer;

            auto grpc_result = read_state_.RunOperation(guard, [&]() {
                ReactorType::StartRead(&grpc_buffer);
            });

            if (!grpc_result.has_value()) {
                ALOGE("%s.ReceiveData(): In terminating state.  Returning failure.",
                      ToString().c_str());
                return 0;
            }

            if (!*grpc_result) {
                ALOGE("%s.OnReadDone() got failure.  Terminating.", ToString().c_str());
                read_state_.Terminate(guard);
                return 0;
            }

            // Try to extract data from the buffer in a usable form.
            read_data_ = ReadData::Extract(grpc_buffer);
            if (!read_data_) {
                // We got unusable data and are in a bad state; can't safely process further reads.
                read_state_.Terminate(guard);
                return 0;
            }
        }

        // At this point read_data_ must be populated.
        size_t read_len = read_data_->Consume(buffer, buffer_size);

        // If we exhausted read_data_ then clear it so that we Grpc read on next ReceiveData().
        if (read_data_->Exhausted()) {
            read_data_.reset();
        }

        bytes_read_ += read_len;
        return read_len;
    }

    virtual size_t BytesReceived() const { return bytes_read_.load(); }

protected:
    // Let subclasses check that all read/write operations are terminated before finalizing.
    bool TryTerminateAllOperations() {
        return read_state_.TryTerminate() && write_state_.TryTerminate();
    }

private:
    // Needed to get certs for AuthenticatedVmName and AuthenticatedIdentity.
    virtual std::shared_ptr<const grpc::AuthContext> GetAuthContext() const = 0;

    // Reactor method called after a write initiated by StartWrite has completed.
    void OnWriteDone(bool success) { write_state_.ReportGrpcResult(success); }

    // Reactor method called after a read requested by StartRead terminates.
    void OnReadDone(bool success) { read_state_.ReportGrpcResult(success); }

    GrpcOperationStateMachine write_state_;
    std::atomic<size_t> bytes_written_;

    GrpcOperationStateMachine read_state_;
    std::optional<ReadData> read_data_;
    std::atomic<size_t> bytes_read_;
};

}  // namespace android::sdv::dt::agent
