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

    virtual ReceivedData ReceiveData() override {
        // Prepare buffer and start read.
        grpc::ByteBuffer grpc_buffer;

        auto guard = read_state_.Lock();
        // Start GRPC read operation and wait for callback indicating completion.
        auto grpc_result =
                read_state_.RunOperation(guard, [&]() { ReactorType::StartRead(&grpc_buffer); });

        if (!grpc_result.has_value()) {
            ALOGE("%s.ReceiveData(): In terminating state.  Returning failure.",
                  ToString().c_str());
            return ReceivedData();
        }

        if (!*grpc_result) {
            ALOGE("%s.OnReadDone() got failure.  Terminating.", ToString().c_str());
            read_state_.Terminate(guard);
            return ReceivedData();
        }

        grpc::Slice read_slice;

        // We need to return data to the application in a single contiguous block.  A grpc::Slice
        // references a contiguous region, but the grpc::ByteBuffer into which GRPC reads data
        // may contain multiple non-contiguous slices.
        //
        // If grpc_buffer consists of a single slice, TrySingleSlice can populate read_slice with
        // a new reference to that slice (which can persist after grpc_buffer is destroyed since
        // slices are refcounted).
        //
        // If grpc_buffer has multiple slices, TrySingleSlice can't work, so we fall back to
        // DumpToSingleSlice, which creates a new slice and copies all the data into it.
        //
        // TODO(b/491392921): Try collecting some stats on how often we're having to copy here and
        // consider changing ReceiveData() to be able to return multiple spans.
        if (!grpc_buffer.TrySingleSlice(&read_slice).ok() &&
            !grpc_buffer.DumpToSingleSlice(&read_slice).ok()) {
            ALOGE("ReceiveData() unable to dump buffer to single slice.  Forcing terminate.");
            // Move immediately to terminated state since we got unusable data.
            read_state_.Terminate(guard);
            return ReceivedData();
        } else if (read_slice.size() == 0) {
            ALOGE("ReceiveData() got GRPC success but zero bytes.  Forcing terminate.");
            // Move immediately to terminated state since we got unusable data.
            read_state_.Terminate(guard);
            return ReceivedData();
        }

        bytes_read_ += read_slice.size();

        // grpc::Slice objects are refcounted, so the captured read_slice will remain valid even
        // after the grpc_buffer from which it was created is destroyed.
        return ReceivedData({read_slice.begin(), read_slice.size()}, [read_slice](auto) {
            // Don't actually need to do anything here; the captured read_slice will be destroyed
            // (with the rest of the std::function) right after this call, which is enough to
            // free the storage.
        });
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
    std::atomic<size_t> bytes_read_;
};

}  // namespace android::sdv::dt::agent
