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

#include "dt_buffer.h"

#include <string>
#include <vector>

namespace android::sdv::dt::agent::proto {

struct Payload {
    enum Type {
        Unknown = -1,
        // The first part of inter-agent hand shake and initial check
        // This message is sent to the counterparty when we have just established a connection.
        Ping,
        // The second part of inter-agent hand shake and initial check
        // This message is sent to the counterparty when we are ready to receive messages.
        Pong,
        // Requests a remote agent to subscribe to its local topic
        SubscribeRequest,
        // Response to SubscribeRequest
        SubscribeResult,
        // Export broker sends messages to subscribers' agents
        TopicRawData,
        // Export broker informs subscribers' agents if their topic is destroyed
        TopicDestroyed,
        // Requests a remote agent to send the last message of the specified topic
        LastMessageRequest,
        // Response to LastMessageRequest. Contains the result of the request and, if successful,
        // a buffer with the last message.
        LastMessageResult,
    };

    const Type type_;

    Payload(Type type) : type_(type) {}
    virtual ~Payload() {}

    static uint32_t BaseSize() { return sizeof(int16_t); }

    virtual uint32_t Size() const { return BaseSize(); }
    virtual bool Serialize(BufferWriter& bw) const {
        (void)bw;
        return true;
    }

    virtual bool Deserialize(BufferReader& br) {
        (void)br;
        return true;
    }

    bool operator==(const Payload& other) const {
        return type_ == other.type_ && Size() == other.Size();
    }

    template <typename T>
    const T* As() const {
        if (type_ != T::payload_type) {
            return nullptr;
        }
        return static_cast<const T*>(this);
    }
};

struct Payload_Ping : public Payload {
    static constexpr Type payload_type = Type::Ping;

    std::string vm_name_;
    Payload_Ping() : Payload(Ping) {}
    Payload_Ping(const std::string& vm_name) : Payload(Ping), vm_name_(vm_name) {}

    uint32_t Size() const override {
        return Payload::Size() + static_cast<uint32_t>(sizeof(uint16_t) + vm_name_.length());
    }

    bool Serialize(BufferWriter& bw) const override { return bw.Write(vm_name_); }

    bool Deserialize(BufferReader& br) override { return br.Read(vm_name_); }
};

struct Payload_Pong : public Payload {
    static constexpr Type payload_type = Type::Pong;

    uint32_t ping_id_;

    Payload_Pong(uint32_t ping_id = 0) : Payload(Pong), ping_id_(ping_id) {}

    uint32_t Size() const override {
        return Payload::Size() + static_cast<uint32_t>(sizeof(ping_id_));
    }

    bool Serialize(BufferWriter& bw) const override { return bw.Write(ping_id_); }

    bool Deserialize(BufferReader& br) override { return br.Read(ping_id_); }
};

struct Payload_SubscribeRequest : public Payload {
    static constexpr Type payload_type = Type::SubscribeRequest;

    std::string topic_name_;

    Payload_SubscribeRequest() : Payload(SubscribeRequest) {}

    Payload_SubscribeRequest(const std::string& topic_name) :
          Payload(SubscribeRequest), topic_name_(topic_name) {}

    uint32_t Size() const override {
        return Payload::Size() + static_cast<uint32_t>(sizeof(uint16_t) + topic_name_.length());
    }

    bool Serialize(BufferWriter& bw) const override {
        // TODO: don't modify write pointer in case of failure
        return bw.Write(topic_name_);
    }

    bool Deserialize(BufferReader& br) override { return br.Read(topic_name_); }
};

struct Payload_SubscribeResult : public Payload {
    static constexpr Type payload_type = Type::SubscribeResult;

    static constexpr uint16_t Result_Ok = 0;
    static constexpr uint16_t Result_Failure = 1;
    uint16_t result_;
    uint32_t message_size_;
    uint32_t message_count_;
    std::string topic_name_;

    Payload_SubscribeResult() : Payload(SubscribeResult) {}

    uint32_t Size() const override {
        return Payload::Size() +
                static_cast<uint32_t>(sizeof(uint16_t) +  // result_
                                      sizeof(uint32_t) +  // message_size_
                                      sizeof(uint32_t) +  // message_count_
                                      sizeof(uint16_t) +  // topic_name_
                                      topic_name_.length());
    }

    bool Serialize(BufferWriter& bw) const override {
        // TODO: don't modify write pointer in case of failure
        return bw.Write(result_) && bw.Write(message_size_) && bw.Write(message_count_) &&
                bw.Write(topic_name_);
    }

    bool Deserialize(BufferReader& br) override {
        return br.Read(result_) && br.Read(message_size_) && br.Read(message_count_) &&
                br.Read(topic_name_);
    }
};

struct Payload_TopicRawData : public Payload {
    static constexpr Type payload_type = Type::TopicRawData;

    Payload_TopicRawData(uint32_t data_size) :
          Payload(TopicRawData), data_size_(data_size), buffer_(data_size) {}

    std::string topic_name_;
    uint32_t data_size_;  // to be able to reuse the buffer with different buffers
    RawBuffer buffer_;

    static uint32_t PayloadSizeForTopic(const std::string& topic_name, uint32_t message_size) {
        return Payload::BaseSize() +
                static_cast<uint32_t>(sizeof(uint16_t) + topic_name.length() + sizeof(uint32_t) +
                                      message_size);
    }

    uint32_t Size() const override { return PayloadSizeForTopic(topic_name_, data_size_); }

    bool Serialize(BufferWriter& bw) const override {
        // TODO: don't modify write pointer in case of failure
        return bw.Write(topic_name_) && bw.Write(data_size_) && bw.Write(buffer_.Ptr(), data_size_);
    }

    bool Deserialize(BufferReader& br) override {
        if (!br.Read(topic_name_) || !br.Read(data_size_)) {
            return false;
        }
        if (br.AvailableToRead() < data_size_) {
            return false;
        }
        if (buffer_.Size() < data_size_) {
            buffer_.Resize(data_size_, false);
        }
        return br.Read(buffer_.Ptr(), data_size_);
    }
};

struct Payload_TopicDestroyed : public Payload {
    static constexpr Type payload_type = Type::TopicDestroyed;

    std::string topic_name_;

    Payload_TopicDestroyed() : Payload(TopicDestroyed) {}

    Payload_TopicDestroyed(const std::string& topic_name) :
          Payload(TopicDestroyed), topic_name_(topic_name) {}

    uint32_t Size() const override {
        return Payload::Size() + static_cast<uint32_t>(sizeof(uint16_t) + topic_name_.length());
    }

    bool Serialize(BufferWriter& bw) const override {
        // TODO: don't modify write pointer in case of failure
        return bw.Write(topic_name_);
    }

    bool Deserialize(BufferReader& br) override { return br.Read(topic_name_); }
};

struct Payload_LastMessageRequest : public Payload {
    static constexpr Type payload_type = Type::LastMessageRequest;

    std::string topic_name_;

    Payload_LastMessageRequest() : Payload(LastMessageRequest) {}

    Payload_LastMessageRequest(const std::string& topic_name) :
          Payload(LastMessageRequest), topic_name_(topic_name) {}

    uint32_t Size() const override {
        return Payload::Size() + static_cast<uint32_t>(sizeof(uint16_t) + topic_name_.length());
    }

    bool Serialize(BufferWriter& bw) const override {
        // TODO: don't modify write pointer in case of failure
        return bw.Write(topic_name_);
    }

    bool Deserialize(BufferReader& br) override { return br.Read(topic_name_); }
};

struct Payload_LastMessageResult : public Payload {
    static constexpr Type payload_type = Type::LastMessageResult;

    Payload_LastMessageResult() : Payload(LastMessageResult) {}

    static constexpr uint16_t Result_Ok = 0;
    static constexpr uint16_t Result_Failure = 1;

    std::string topic_name_;
    uint16_t result_;
    RawBuffer buffer_;

    static uint32_t PayloadSizeForTopic(const std::string& topic_name, uint32_t message_size) {
        return Payload::BaseSize() +
                static_cast<uint32_t>(sizeof(uint16_t) + topic_name.length() + sizeof(uint16_t) +
                                      sizeof(uint32_t) + message_size);
    }

    uint32_t Size() const override {
        return PayloadSizeForTopic(topic_name_, static_cast<uint32_t>(buffer_.Size()));
    }

    bool Serialize(BufferWriter& bw) const override {
        // TODO: don't modify write pointer in case of failure
        return bw.Write(topic_name_) && bw.Write(result_) &&
                bw.Write(static_cast<uint32_t>(buffer_.Size())) &&
                bw.Write(buffer_.Ptr(), buffer_.Size());
    }

    bool Deserialize(BufferReader& br) override {
        uint32_t buffer_size;
        if (!br.Read(topic_name_) || !br.Read(result_) || !br.Read(buffer_size)) {
            return false;
        }
        if (br.AvailableToRead() < buffer_size) {
            return false;
        }
        if (buffer_.Size() < buffer_size) {
            buffer_.Resize(buffer_size, false);
        }
        return br.Read(buffer_.Ptr(), buffer_size);
    }
};

inline std::unique_ptr<Payload> DeserializePayloadFromBuffer(BufferReader& br) {
    int16_t type;
    Payload* payload = nullptr;
    if (!br.Read(type)) return std::unique_ptr<Payload>(nullptr);
    switch (type) {
        case Payload::Ping:
            payload = static_cast<Payload*>(new Payload_Ping());
            break;
        case Payload::Pong:
            payload = static_cast<Payload*>(new Payload_Pong());
            break;
        case Payload::SubscribeRequest:
            payload = static_cast<Payload*>(new Payload_SubscribeRequest());
            break;
        case Payload::SubscribeResult:
            payload = static_cast<Payload*>(new Payload_SubscribeResult());
            break;
        case Payload::TopicRawData:
            // Instantiate a TopicRawData payload with 1KB buffer. The buffer
            // may be increased later during deserialization.
            payload = static_cast<Payload*>(new Payload_TopicRawData(1024));
            break;
        case Payload::TopicDestroyed:
            payload = static_cast<Payload*>(new Payload_TopicDestroyed());
            break;
        case Payload::LastMessageRequest:
            payload = static_cast<Payload*>(new Payload_LastMessageRequest());
            break;
        case Payload::LastMessageResult:
            payload = static_cast<Payload*>(new Payload_LastMessageResult());
            break;
    }

    if (payload) {
        if (!payload->Deserialize(br)) {
            delete payload;
            payload = nullptr;
        }
    }

    return std::unique_ptr<Payload>(payload);
}

struct Parcel {
    enum class ParseResult { Success, NeedMoreData, UnknownPayload, Failure };

    static constexpr uint32_t Header = 0xC00010FF;
    static constexpr uint32_t HeaderSize = sizeof(Header) + /* sequential_id */ sizeof(uint32_t) +
            /*  payload_size */ sizeof(uint32_t);

    uint32_t sequential_id_;

    Parcel(uint32_t sequential_id = 0) : sequential_id_(sequential_id) {}

    static uint32_t ParcelSize(uint32_t payload_size) { return HeaderSize + payload_size; }

    static uint32_t ParcelSize(const Payload& payload) { return ParcelSize(payload.Size()); }

    static ParseResult CanReadParcel(const BufferReader& br) {
        uint32_t header, sequential_id, payload_size;
        if (br.AvailableToRead() < HeaderSize) {
            return ParseResult::NeedMoreData;
        }
        BufferReader temp_reader(br);
        if (!temp_reader.Read(header) || header != Header || !temp_reader.Read(sequential_id) ||
            !temp_reader.Read(payload_size)) {
            return ParseResult::Failure;
        }
        if (temp_reader.AvailableToRead() < payload_size) {
            return ParseResult::NeedMoreData;
        }
        return ParseResult::Success;
    }

    bool Serialize(BufferWriter& bw, const Payload& payload) const {
        return (ParcelSize(payload) <= bw.Available()) && bw.Write(Header) &&
                bw.Write(sequential_id_) && bw.Write(payload.Size()) &&
                bw.Write(static_cast<int16_t>(payload.type_)) && payload.Serialize(bw);
    }

    static std::pair<ParseResult, std::unique_ptr<Payload>> Deserialize(BufferReader& br,
                                                                        uint32_t* parcel_id) {
        uint32_t payload_size;
        // Ensure the parcel isn't corrupted and it's enough data to deserialize
        auto res = CanReadParcel(br);
        if (res != ParseResult::Success) return {res, std::unique_ptr<Payload>(nullptr)};

        if (!br.MovePos(sizeof(uint32_t)) ||
            !((parcel_id != nullptr) ? br.Read(*parcel_id) : (br.MovePos(sizeof(uint32_t)) > 0)) ||
            !br.Read(payload_size)) {
            return {ParseResult::Failure, std::unique_ptr<Payload>(nullptr)};
        }

        auto payload = DeserializePayloadFromBuffer(br);
        // Returning UnknownPayload instead of Failure because the parcel isn't corrupted, but the
        // payload is unknown and can't be parsed, probably, because the counteragent has a newer
        // protocol version.
        return {((payload != nullptr) ? ParseResult::Success : ParseResult::UnknownPayload),
                std::move(payload)};
    }
};

}  // namespace android::sdv::dt::agent::proto
