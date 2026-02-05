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
    grpc_result_.set_value(success);
}

void GrpcOperationStateMachine::Terminate(std::unique_lock<std::mutex>& guard) {
    CHECK(guard.owns_lock() && guard.mutex() == &mutex_);
    terminated_ = true;
}

bool GrpcOperationStateMachine::TryTerminate() {
    std::unique_lock guard(mutex_, std::try_to_lock);
    if (guard.owns_lock()) {
        terminated_ = true;
    }
    return terminated_;
}

bool GrpcOperationStateMachine::IsTerminated() {
    return terminated_;
}

}  // namespace android::sdv::dt::agent
