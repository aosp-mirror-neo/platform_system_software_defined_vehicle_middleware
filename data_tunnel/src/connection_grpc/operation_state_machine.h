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

#include <future>

namespace android::sdv::dt::agent {

// GrpcOperationStateMachine
//
// Grpc read and write operations are asynchronous from our perspective:  Grpc runs
// its own internal threads that execute read or write operations and that will
// execute a callback on one of our connection objects when that operation completes.
//
// This class serves several purposes:
//
// - Turns the async Grpc read / write processes synchronous to support our synchronous
//   SendData() / ReceiveData() methods.
//
// - Ensures we don't issue multiple simultaneous operations of a given type to Grpc
//   (which it doesn't support), by having threads that want to start an operation wait
//   for any in-progress operation to complete.
class GrpcOperationStateMachine {
public:
    // Constructor for GrpcOperationStateMachine.
    GrpcOperationStateMachine(std::string op_type) : op_type_(op_type) {}

    // Get a lock on the state, needed to start an operation.
    //
    // This is exposed so that we can also use this lock to control access to other state in
    // the connection class that's related to a read or write operation, and to handle cases
    // where the lock needs to be held across multiple calls into this class.
    std::unique_lock<std::mutex> Lock() { return std::unique_lock(mutex_); }

    // Runs a GRPC operation that is initiated by the start_grpc_operation.
    //
    // The caller must hold a lock provided by Lock() to call this.
    // The caller must forward any OnReadDone() or OnWriteDone() call to ReportGrpcResult().
    template <typename StartFn>
    std::optional<bool> RunOperation(std::unique_lock<std::mutex>& guard, StartFn start_op) {
        CHECK(guard.owns_lock() && guard.mutex() == &mutex_);
        if (terminated_) {
            ALOGE("Grpc %s requested but terminate already happened.  Aborting.", op_type_.c_str());
            return std::nullopt;
        }

        grpc_result_ = std::promise<bool>();
        start_op();
        return grpc_result_.get_future().get();
    }

    // Reports the result of a pending operation.
    // Must be called from the reactor's OnWriteDone() or OnReadDone().
    void ReportGrpcResult(bool success);

    // Terminate state machine.  Caller must already hold lock.
    void Terminate(std::unique_lock<std::mutex>& guard);

    // Try to immediately terminate state machine.
    // Returns true if state machine was terminated or was already in that state.
    bool TryTerminate();

    // Check whether state machine is terminated.
    // Only used for testing, does not lock and tests must ensure no concurrent modification.
    bool IsTerminated();

private:
    mutable std::mutex mutex_;

    const std::string op_type_;

    bool terminated_ = false;

    // Storage for Grpc result populated by ReportGrpcResult.
    std::promise<bool> grpc_result_;
};

}  // namespace android::sdv::dt::agent
