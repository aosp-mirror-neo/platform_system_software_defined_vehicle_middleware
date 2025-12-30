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

#include "connection_grpc/operation_state_machine.h"

#include <gtest/gtest.h>

#include <atomic>
#include <chrono>
#include <future>
#include <thread>

namespace android::sdv::dt::agent {

using namespace std::chrono_literals;

class GrpcOperationStateMachineTest : public ::testing::Test {
protected:
    GrpcOperationStateMachine state_machine_{"TestOp"};
};

TEST_F(GrpcOperationStateMachineTest, RunOperationSuccess) {
    std::atomic<bool> op_started = false;
    std::thread grpc_callback_thread;
    auto start_op = [&]() {
        op_started = true;
        grpc_callback_thread = std::thread([&]() {
            // Simulate gRPC callback after some time
            std::this_thread::sleep_for(10ms);
            state_machine_.ReportGrpcResult(true);
        });
    };

    auto guard = state_machine_.Lock();
    auto result = state_machine_.RunOperation(guard, start_op);

    ASSERT_TRUE(result.has_value());
    EXPECT_TRUE(*result);
    EXPECT_TRUE(op_started);
    grpc_callback_thread.join();
}

TEST_F(GrpcOperationStateMachineTest, RunOperationFailure) {
    std::atomic<bool> op_started = false;
    std::thread grpc_callback_thread;
    auto start_op = [&]() {
        op_started = true;
        grpc_callback_thread = std::thread([&]() {
            // Simulate gRPC callback after some time
            std::this_thread::sleep_for(10ms);
            state_machine_.ReportGrpcResult(false);
        });
    };

    auto guard = state_machine_.Lock();
    auto result = state_machine_.RunOperation(guard, start_op);

    ASSERT_TRUE(result.has_value());
    EXPECT_FALSE(*result);
    EXPECT_TRUE(op_started);

    grpc_callback_thread.join();
}

TEST_F(GrpcOperationStateMachineTest, ConcurrentOperations) {
    std::promise<void> op1_started_promise;
    std::future<void> op1_started_future = op1_started_promise.get_future();
    std::atomic<bool> op1_finished = false;

    std::promise<void> op2_started_promise;
    std::future<void> op2_started_future = op2_started_promise.get_future();
    std::atomic<bool> op2_finished = false;

    // Thread for the first operation.
    std::thread op1_thread([&]() {
        auto guard = state_machine_.Lock();
        state_machine_.RunOperation(guard, [&]() { op1_started_promise.set_value(); });
        op1_finished = true;
    });

    // Wait for op1 to start.
    op1_started_future.wait_for(20ms);

    // Thread for the second operation. It should block.
    std::thread op2_thread([&]() {
        auto guard = state_machine_.Lock();
        state_machine_.RunOperation(guard, [&]() { op2_started_promise.set_value(); });
        op2_finished = true;
    });

    // Complete op1.
    state_machine_.ReportGrpcResult(true);
    op1_thread.join();
    EXPECT_TRUE(op1_finished);

    // Now op2 should start.
    op2_started_future.wait_for(20ms);

    // Complete op2.
    state_machine_.ReportGrpcResult(true);
    op2_thread.join();
    EXPECT_TRUE(op2_finished);
}

TEST_F(GrpcOperationStateMachineTest, WaitToTerminateWhenInactive) {
    state_machine_.WaitToTerminate();
    // It should just return without blocking.
}

TEST_F(GrpcOperationStateMachineTest, WaitToTerminateWithActiveOperation) {
    std::promise<void> op_started;
    std::future<void> op_started_future = op_started.get_future();

    std::thread op_thread([&]() {
        auto guard = state_machine_.Lock();
        state_machine_.RunOperation(guard, [&]() { op_started.set_value(); });
    });

    op_started_future.wait_for(100ms);

    std::thread terminate_thread([&]() { state_machine_.WaitToTerminate(); });

    // Give terminate_thread a chance to run and block.
    std::this_thread::sleep_for(20ms);

    // Now complete the operation.
    state_machine_.ReportGrpcResult(true);

    // The terminate thread should now unblock and finish.
    terminate_thread.join();
    op_thread.join();
}

TEST_F(GrpcOperationStateMachineTest, WaitToTerminateWithWaitingOperation) {
    std::promise<void> op1_started_promise;
    std::future<void> op1_started_future = op1_started_promise.get_future();
    std::atomic<bool> op1_finished = false;

    std::thread op1_thread([&]() {
        auto guard = state_machine_.Lock();
        state_machine_.RunOperation(guard, [&]() { op1_started_promise.set_value(); });
        op1_finished = true;
    });

    op1_started_future.wait_for(100ms);

    std::atomic<bool> op2_started = false;
    std::atomic<bool> op2_returned_nullopt = false;
    std::thread op2_thread([&]() {
        auto guard = state_machine_.Lock();
        auto result = state_machine_.RunOperation(guard, [&]() { op2_started = true; });
        if (!result.has_value()) {
            op2_returned_nullopt = true;
        }
    });

    // Give op2 a chance to block.
    std::this_thread::sleep_for(20ms);

    std::thread terminate_thread([&]() { state_machine_.WaitToTerminate(); });

    // Give terminate a chance to block.
    std::this_thread::sleep_for(20ms);

    // op1 is running, op2 is waiting, terminate is waiting.
    // Now, complete op1.
    state_machine_.ReportGrpcResult(true);

    op1_thread.join();
    EXPECT_TRUE(op1_finished);

    // After op1 completes, op2's `RunOperation` should unblock, see the state is terminating,
    // and return nullopt without running its `start_op`.
    op2_thread.join();
    EXPECT_FALSE(op2_started);
    EXPECT_TRUE(op2_returned_nullopt);

    // And terminate thread should complete.
    terminate_thread.join();
}

TEST_F(GrpcOperationStateMachineTest, RunOperationWhileTerminating) {
    state_machine_.WaitToTerminate();

    auto guard = state_machine_.Lock();
    auto result = state_machine_.RunOperation(guard, [] {});
    EXPECT_FALSE(result.has_value());
}

TEST_F(GrpcOperationStateMachineTest, ForceTerminate) {
    auto guard = state_machine_.Lock();
    state_machine_.ForceTerminate(guard);
    // Should not crash.

    // After ForceTerminate, RunOperation should fail.
    auto result = state_machine_.RunOperation(guard, [] {});
    EXPECT_FALSE(result.has_value());
}

}  // namespace android::sdv::dt::agent
