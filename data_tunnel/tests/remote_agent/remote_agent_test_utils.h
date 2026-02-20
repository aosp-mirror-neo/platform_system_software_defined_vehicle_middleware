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

#include "connection.h"
#include "proto/dt_buffer.h"
#include "proto/dt_proto.h"
#include "remote_agent/remote_agent_common.h"
#include "remote_agent/remote_agent_handshaker.h"

#include <android-base/scopeguard.h>
#include <gmock/gmock.h>

#include <deque>
#include <future>
#include <optional>
#include <thread>
#include <vector>

namespace android::sdv::dt::agent::test {

// Fake IConnection implementation
//
// The SendData implementation records each message sent as a blob of bytes and allows them
// to be retrieved and inspected using the GetNextSentData* methods.
//
// ReceiveData returns data that is provided by the test code using the Enqueue methods; if
// nothing is in the queue when ReceiveData is called, it blocks until something is enqueued; this
// is meant to support testing of code that runs a dedicated thread to do blocking reads from
// ReceiveData.
class FakeConnection : public IConnection {
public:
    FakeConnection() {
        // We want to mock RequestDisconnect so we can check for it being called, but we also want
        // a default behavior that will generate SendData / ReceiveData failures as will happen in
        // the real impl.
        ON_CALL((*this), RequestDisconnect()).WillByDefault([this]() {
            EnqueueReceiveFailure();
            SetSendDataReturnValue(false);
        });
    }

    MOCK_METHOD(void, RequestDisconnect, (), (override));
    MOCK_METHOD(size_t, BytesSent, (), (const, override));
    MOCK_METHOD(size_t, BytesReceived, (), (const, override));
    MOCK_METHOD(std::optional<std::string>, AuthenticatedVmName, (), (const, override));
    MOCK_METHOD(std::optional<std::string>, AuthenticatedIdentity, (), (const, override));

    virtual std::string ToString() const override {
        return "FakeConnection(" + std::to_string(reinterpret_cast<unsigned long>(this)) + ")";
    }

    // Fake implementation of IConnection::ReceiveData.  Returns data (or a failure event) enqueued
    // by the "Enqueue*" methods below.  If no data or failure event has been enqueued
    virtual ReceivedData ReceiveData() override;

    // Enqueue the given buffer to be returned ReceiveData; if length is not provided the entire
    // remaining contents of the buffer are read.  Consumes length bytes from the reader.
    // Meant for testing of code that can receive a payload in chunks across multiple ReceiveDatas.
    void EnqueueDataForReceive(BufferReader& reader, std::optional<size_t> length = std::nullopt);

    // Serialize a payload in a parcel with the given ID and enqueue it for ReceiveData
    void EnqueueParcelForReceive(int32_t parcel_id, const proto::Payload& payload);

    // Enqueue a receive failure to be returned by ReceiveData.
    void EnqueueReceiveFailure();

    // Count of calls to ReceiveData needed by some tests.
    size_t ReceiveCallCount() { return receive_call_count_; }

    // Fake implementation of IConnection::SendData that records the data passed to each call
    // in a queue that can then be consumed by test code via the "GetNextSent*" methods.  Can
    // be made to return failure on the next call using SetSendDataReturnValue.
    virtual bool SendData(const void* buffer, size_t bytes_to_send) override;

    // Retrieve the next item in the sent data queue if available.  If the queue is empty, waits up
    // to timeout_seconds before returning nullopt.
    std::optional<RawBuffer> GetNextSentData(int timeout_seconds = 5);

    // Retrieves the next item in the sent data queue only if it successfully parses as a payload
    // that matches the given predicate.  Otherwise leaves the entry in place and returns nullopt.
    std::unique_ptr<proto::Payload> GetNextSentDataIfPayload(
            std::function<bool(const proto::Payload&)> predicate, int timeout_seconds = 5);

    // Retrieves the next item in the sent data queue only if it successfully parses as a payload
    // of type PayloadType.  Otherwise leaves the entry in place and returns nullopt.
    template <typename PayloadType>
    std::unique_ptr<PayloadType> GetNextSentDataIfPayload(const int timeout_seconds = 5) {
        auto payload = GetNextSentDataIfPayload(
                [](const proto::Payload& payload) {
                    return payload.type == PayloadType::payload_type;
                },
                timeout_seconds);
        if (!payload) {
            return nullptr;
        }
        return std::unique_ptr<PayloadType>(static_cast<PayloadType*>(payload.release()));
    }

    // Get the # of parcels sent by SendData that haven't been consumed by a GetNextSentData* call.
    size_t RemainingSentDataCount();

    // Set the value to be returned by the next call to SendData.
    void SetSendDataReturnValue(bool);

private:
    std::atomic<size_t> receive_call_count_ = 0;

    std::mutex lock_;
    std::condition_variable cv_;

    // Queue of buffers to be returned from ReceiveData calls.  ReceiveData() blocks when this is
    // empty and waits until something is enqueued; an empty buffer can be queued to simulate a
    // receive failure.
    std::deque<RawBuffer> receive_queue_;

    // Queue of parcels that have been sent.  Entries are pushed on the back when SendData()
    // is called and popped from the front when matched by an ExpectSentPayload call.
    std::deque<RawBuffer> sent_queue_;

    bool send_data_return_value_ = true;
};

// We store the releaser mock function in a shared so that both the test and the callable
// that we attach to the connection holder can own it.
using FakeConnectionReleaser = std::shared_ptr<testing::MockFunction<void(IConnection*)>>;
struct MakeFakeConnectionResult {
    // Owned FakeConnection implementation of IConnection.
    IConnectionHolder connection;

    // Releaser that will be called when connection is reset() or destroyed.
    // Provided to allow tests to EXPECT_CALL deletion.
    FakeConnectionReleaser releaser;
};
MakeFakeConnectionResult MakeFakeConnection();

class MockConnectionManager : public ConnectionManager {
public:
    virtual void Shutdown() override {}

    MOCK_METHOD(bool, Listen, (ConnectionManager::EventHandler&), (override));
    MOCK_METHOD(void, Connect,
                (const SocketAddressV4&, const std::string& target_name,
                 ConnectionManager::EventHandler&),
                (override));

    MockConnectionManager(
            const IPAddressV4& local_ip = IPAddressV4(static_cast<in_addr>(0x01020304)),
            uint16_t listen_port = 31337) :
          ConnectionManager(local_ip) {
        listen_port_ = listen_port;
    }
};

// Mock version of handshaker for use in unit tests.
class MockHandshaker : public RemoteAgentHandshaker {
public:
    MOCK_METHOD(std::optional<std::string>, PerformHandshake,
                (IConnection & connection, std::optional<std::string> expected_vm_name), (const));
};

// Wait up to a defined timeout for a condition that depends on some other thread's operation.
//
// Provided to avoid having to use std::this_thread::sleep_for with a fixed constant that slows
// down test execution when made large enough to avoid flakes.
template <typename Duration>
bool WaitForConditionOrTimeout(std::function<bool()> condition, Duration duration) {
    auto deadline = std::chrono::steady_clock::now() + duration;
    while (std::chrono::steady_clock::now() < deadline) {
        if (condition()) {
            return true;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }
    return false;
}

inline bool WaitForConditionOrTimeout(std::function<bool()> condition) {
    return WaitForConditionOrTimeout(std::move(condition), std::chrono::seconds(2));
}

}  // namespace android::sdv::dt::agent::test
