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
//
// - Enables clean connection shutdown by waiting for any outstanding Grpc operation to
//   complete and for any other threads waiting to start an operation to depart.
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
        // Wait for any current op to resolve (or terminate to start).
        wait_count_++;
        cv_.wait(guard, [this]() { return state_ != State::GRPC_WAIT; });
        wait_count_--;

        if (state_ != State::INACTIVE) {
            ALOGE("Grpc %s requested but terminate in progress.  Aborting.", op_type_.c_str());
            cv_.notify_all();
            return std::nullopt;
        }
        state_ = State::GRPC_WAIT;
        grpc_result_.reset();

        start_op();

        cv_.wait(guard, [this]() { return grpc_result_.has_value(); });
        bool success = *grpc_result_;

        // Should still be in GRPC_WAIT unless WaitToTerminate() moved us to TERMINAL_GRPC_WAIT.
        CHECK(state_ == State::GRPC_WAIT || state_ == State::TERMINAL_GRPC_WAIT);

        // Go back to inactive unless op failed or terminate was requested (indicated by
        // non-GRPC_WAIT state).
        state_ = (success && state_ == State::GRPC_WAIT) ? State::INACTIVE
                                                         : State::WAITING_THREADS_EXIT_WAIT;

        // Wake up anyone who was waiting to start or to complete Terminate().
        cv_.notify_all();

        return success;
    }

    // Reports the result of a pending operation.
    // Must be called from the reactor's OnWriteDone() or OnReadDone().
    void ReportGrpcResult(bool success);

    // Terminate the state machine, waiting for any outstanding Grpc operation to complete
    // and for any threads waiting to start an operation to exit.
    void WaitToTerminate();

    // Immediately terminate operation.  For use when there's an error that didn't manifest
    // as a failed GRPC operation.
    //
    // The caller must hold a lock provided by Lock() to call this.
    void ForceTerminate(std::unique_lock<std::mutex>& /*guard*/);

private:
    std::condition_variable cv_;
    mutable std::mutex mutex_;

    // State diagram: http://shortn/_v3QIplkcvg
    enum class State {
        // No operation currently active.
        INACTIVE,
        // Grpc operation is in-progress, waiting for result.
        GRPC_WAIT,
        // Terminate was requested but there's an in-progress Grpc op that needs to complete.
        TERMINAL_GRPC_WAIT,
        // Terminate requested or Grpc op failed but there may be threads waiting to start.
        WAITING_THREADS_EXIT_WAIT,
        // Writing has terminated.
        TERMINATED,
    };
    State state_ = State::INACTIVE;

    std::string op_type_;

    // Storage for Grpc result populated by ReportGrpcResult.
    std::optional<bool> grpc_result_;

    // Count of threads in RunOperation() waiting for a turn.
    size_t wait_count_ = 0;
};

}  // namespace android::sdv::dt::agent
