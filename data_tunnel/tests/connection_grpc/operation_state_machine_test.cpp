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

TEST_F(GrpcOperationStateMachineTest, TerminateWhenInactive) {
    {
        auto guard = state_machine_.Lock();
        state_machine_.Terminate(guard);
    }
    EXPECT_TRUE(state_machine_.IsTerminated());
}

TEST_F(GrpcOperationStateMachineTest, TryTerminateWhenInactive) {
    EXPECT_TRUE(state_machine_.TryTerminate());
}

TEST_F(GrpcOperationStateMachineTest, TerminateWithActiveOperation) {
    std::promise<void> op_started;
    std::future<void> op_started_future = op_started.get_future();
    std::optional<bool> op_result;

    std::jthread op_thread([&]() {
        auto guard = state_machine_.Lock();
        op_result = state_machine_.RunOperation(guard, [&]() { op_started.set_value(); });
    });

    ASSERT_EQ(op_started_future.wait_for(100ms), std::future_status::ready);

    std::promise<void> terminate_done_promise;
    auto terminate_done_future = terminate_done_promise.get_future();
    std::jthread term_thread([&]() {
        auto guard = state_machine_.Lock();
        state_machine_.Terminate(guard);
        terminate_done_promise.set_value();
    });

    ASSERT_EQ(terminate_done_future.wait_for(1s), std::future_status::timeout);
    ASSERT_FALSE(state_machine_.IsTerminated());

    // Now complete the operation and wait briefly for terminate to complete.
    state_machine_.ReportGrpcResult(true);

    ASSERT_EQ(terminate_done_future.wait_for(200ms), std::future_status::ready);
    ASSERT_TRUE(state_machine_.IsTerminated());

    // Operation should have returned success despite termination
    ASSERT_TRUE(op_result && *op_result);
}

TEST_F(GrpcOperationStateMachineTest, TryTerminateWithActiveOperation) {
    std::promise<void> op_started;
    std::future<void> op_started_future = op_started.get_future();
    std::promise<void> op_done;

    std::jthread op_thread([&]() {
        {
            auto guard = state_machine_.Lock();
            state_machine_.RunOperation(guard, [&]() { op_started.set_value(); });
        }
        op_done.set_value();
    });

    ASSERT_EQ(op_started_future.wait_for(100ms), std::future_status::ready);

    ASSERT_FALSE(state_machine_.TryTerminate());

    // Now complete the operation and wait for RunOperation to complete.
    state_machine_.ReportGrpcResult(true);
    ASSERT_EQ(op_done.get_future().wait_for(200ms), std::future_status::ready);

    ASSERT_TRUE(state_machine_.TryTerminate());
}

TEST_F(GrpcOperationStateMachineTest, TerminateWithWaitingOperation) {
    std::promise<void> op1_started;
    std::promise<std::optional<bool>> op1_result;
    auto op1_result_future = op1_result.get_future();
    std::promise<void> op1_terminate_ready;

    std::jthread op1_thread([&]() {
        auto guard = state_machine_.Lock();
        op1_result.set_value(
                state_machine_.RunOperation(guard, [&]() { op1_started.set_value(); }));

        // Wait for main test to signal we can terminate.
        op1_terminate_ready.get_future().get();

        // op1 calls terminate without ever releasing guard.
        state_machine_.Terminate(guard);
    });

    ASSERT_EQ(op1_started.get_future().wait_for(100ms), std::future_status::ready);

    std::promise<void> op2_started;
    std::promise<std::optional<bool>> op2_result;
    auto op2_result_future = op2_result.get_future();
    std::jthread op2_thread([&]() {
        op2_started.set_value();
        auto guard = state_machine_.Lock();
        op2_result.set_value(state_machine_.RunOperation(guard, [&]() {}));
    });

    // Wait for op2 thread to start.
    ASSERT_EQ(op2_started.get_future().wait_for(100ms), std::future_status::ready);

    // Now, report a result.
    state_machine_.ReportGrpcResult(true);

    // Now op1 completes.
    ASSERT_EQ(op1_result_future.wait_for(100ms), std::future_status::ready);
    ASSERT_EQ(*op1_result_future.get(), true);

    // op2 still waiting for op1 thread to terminate and release lock
    ASSERT_EQ(op2_result_future.wait_for(200ms), std::future_status::timeout);

    op1_terminate_ready.set_value();

    // Now op2 completes and returns no value due to termination.
    ASSERT_EQ(op2_result_future.wait_for(200ms), std::future_status::ready);
    EXPECT_FALSE(op2_result_future.get().has_value());
}

TEST_F(GrpcOperationStateMachineTest, RunOperationWhileTerminated) {
    ASSERT_TRUE(state_machine_.TryTerminate());

    auto guard = state_machine_.Lock();
    auto result = state_machine_.RunOperation(guard, [] {});
    EXPECT_FALSE(result.has_value());
}

}  // namespace android::sdv::dt::agent