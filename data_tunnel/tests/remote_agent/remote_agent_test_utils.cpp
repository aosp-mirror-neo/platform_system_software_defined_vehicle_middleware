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

#include "remote_agent/remote_agent_test_utils.h"

#include <android-base/logging.h>

namespace android::sdv::dt::agent::test {

// Fake class for IConnection

// Fake IConnection::ReceiveData implementation.
// Retrieves the next block from receive_queue_, or blocks until one is available.
ReceivedData FakeConnection::ReceiveData() {
    receive_call_count_++;

    std::unique_lock guard(lock_);

    // Potentially block until data is available.
    // Times out after 10 seconds to keep badly-written tests from running "forever".
    CHECK(cv_.wait_for(guard, std::chrono::seconds(10), [&]() { return !receive_queue_.empty(); }));

    // Moving ownership into a shared so that our std::function below can capture it.
    auto payload_data = std::make_shared<RawBuffer>(std::move(receive_queue_.front()));
    receive_queue_.pop_front();

    if (payload_data->Size() == 0) {
        return ReceivedData();
    }

    return ReceivedData(std::span<const uint8_t>{payload_data->Ptr(), payload_data->Size()},
                        [payload_data](auto) mutable { payload_data.reset(); });
}

// Enqueues a block of raw data from reader to be returned by a call to ReceiveData.  Consumes
// length bytes from the reader.
void FakeConnection::EnqueueDataForReceive(BufferReader& reader, std::optional<size_t> length) {
    if (!length) {
        length = reader.AvailableToRead();
    }

    // Copy the data into an owned buffer that we'll push onto the queue.
    RawBuffer buffer(*length);
    memcpy(buffer.Ptr(), reader.CurPtr(), *length);
    reader.MovePos(*length);

    // Push buffer onto queue then notify any waiting reader.
    std::unique_lock guard(lock_);
    receive_queue_.emplace_back(std::move(buffer));
    guard.unlock();
    cv_.notify_all();
}

// Serializes the given payload and enqueues it to be received.
void FakeConnection::EnqueueParcelForReceive(const proto::Payload& payload) {
    // Calculate required buffer size for the given payload.
    proto::Parcel parcel;
    auto parcel_size = parcel.ParcelSize(payload);

    // Allocate buffer and serialize payload into it.
    RawBuffer buffer(parcel_size);
    BufferWriter writer(buffer);
    CHECK(parcel.Serialize(writer, payload));

    // Delegate actual enqueue to raw data method.
    BufferReader reader({buffer.Ptr(), writer.GetPos()});
    EnqueueDataForReceive(reader);
}

// Enqueue empty buffer to simulate ReceiveData failure.
void FakeConnection::EnqueueReceiveFailure() {
    std::unique_lock guard(lock_);
    receive_queue_.emplace_back(RawBuffer(0));
    guard.unlock();
    cv_.notify_all();
}

// Fake IConnection::ReceiveData implementation.
// Copies the provided data into an entry at the back of send_queue_, then
// returns a return value that can be set with SetSendDataReturnValue.
bool FakeConnection::SendData(const void* buffer, size_t bytes_to_send) {
    // Copy data into an owned buffer.
    RawBuffer entry(bytes_to_send);
    memcpy(entry.Ptr(), buffer, bytes_to_send);

    // Push data onto queue and notify any thread waiting in GetNextSendData*.
    std::unique_lock guard(lock_);
    sent_queue_.emplace_back(std::move(entry));
    guard.unlock();
    cv_.notify_all();

    return send_data_return_value_;
}

std::optional<RawBuffer> FakeConnection::GetNextSentData(int timeout_seconds) {
    // Wait for data to become available, if needed.
    std::unique_lock guard(lock_);
    if (!cv_.wait_for(guard, std::chrono::seconds(timeout_seconds),
                      [&]() { return !sent_queue_.empty(); })) {
        // Return nothing on timeout.
        return std::nullopt;
    }

    // Pop the front entry of the queue and return it.
    auto result = std::move(sent_queue_.front());
    sent_queue_.pop_front();
    return result;
}

std::unique_ptr<proto::Payload> FakeConnection::GetNextSentDataIfPayload(
        std::function<bool(const proto::Payload&)> predicate, int timeout_seconds) {
    // Wait for data to become available, if needed.
    std::unique_lock guard(lock_);
    if (!cv_.wait_for(guard, std::chrono::seconds(timeout_seconds),
                      [&]() { return !sent_queue_.empty(); })) {
        // Return nothing on timeout.
        return nullptr;
    }

    // Try to deserialize front entry without removing from queue.
    auto& buffer = sent_queue_.front();
    BufferReader reader({buffer.Ptr(), buffer.Size()});
    auto [parse_result, payload] = proto::Parcel::Deserialize(reader);

    // Return nothing if parse fails or result doesn't match predicate.
    if (parse_result != proto::Parcel::ParseResult::Success || !predicate(*payload)) {
        return nullptr;
    }

    // Pop from queue only once we know we're succeeding.
    sent_queue_.pop_front();
    return std::move(payload);
}

size_t FakeConnection::RemainingSentDataCount() {
    std::unique_lock guard(lock_);
    return sent_queue_.size();
}

void FakeConnection::SetSendDataReturnValue(bool return_value) {
    send_data_return_value_ = return_value;
}

MakeFakeConnectionResult MakeFakeConnection() {
    auto releaser = std::make_shared<testing::MockFunction<void(IConnection*)>>();
    IConnectionHolder connection(new FakeConnection(), [releaser](IConnection* conn) {
        // Call the mock releaser so tests can verify deletion.
        releaser->AsStdFunction()(conn);
        // Actually delete
        delete conn;
    });
    return MakeFakeConnectionResult{std::move(connection), std::move(releaser)};
}

}  // namespace android::sdv::dt::agent::test