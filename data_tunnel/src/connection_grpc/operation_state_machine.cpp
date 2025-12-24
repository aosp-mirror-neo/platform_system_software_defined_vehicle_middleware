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

#define LOG_TAG "sdv_dt_agent:connection_grpc:operation_state_machine"

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

namespace android::sdv::dt::agent {

void GrpcOperationStateMachine::ReportGrpcResult(bool success) {
    auto guard = Lock();
    CHECK(state_ == State::GRPC_WAIT || state_ == State::TERMINAL_GRPC_WAIT);
    grpc_result_ = success;
    cv_.notify_all();
}

void GrpcOperationStateMachine::WaitToTerminate() {
    auto guard = Lock();

    CHECK(state_ != State::TERMINAL_GRPC_WAIT && state_ != State::TERMINATED);

    state_ = (state_ == State::GRPC_WAIT) ? State::TERMINAL_GRPC_WAIT
                                          : State::WAITING_THREADS_EXIT_WAIT;

    cv_.wait(guard,
             [this]() { return state_ == State::WAITING_THREADS_EXIT_WAIT && wait_count_ == 0; });
    state_ = State::TERMINATED;
}

void GrpcOperationStateMachine::ForceTerminate(std::unique_lock<std::mutex>& /*guard*/) {
    // Make sure there's no outstanding activity that we would need to wait on.
    CHECK(state_ != State::GRPC_WAIT && state_ != State::TERMINAL_GRPC_WAIT && wait_count_ == 0);
    state_ = State::TERMINATED;
}

}  // namespace android::sdv::dt::agent
