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

#include <log/log.h>

#include <string>
#include <vector>

namespace android::sdv::dt::agent::proto {

// Protocol version number
static const uint32_t kVersionNumber = 1;

struct Payload {
    enum Type {
        Unknown = -1,
        // The first part of inter-agent hand shake and initial check
        // This message is sent to the counterparty when we have just established a connection.
        Ping = 1,
        // The second part of inter-agent hand shake and initial check
        // This message is sent to the counterparty when we are ready to receive messages.
        Pong = 2,
        // Requests a remote agent to subscribe to its local topic
        SubscribeRequest = 3,
        // Response to SubscribeRequest
        SubscribeResult = 4,
        // Export broker sends messages to subscribers' agents
        TopicRawData = 5,
        // Export broker informs subscribers' agents if their topic is destroyed
        TopicDestroyed = 6,
        // Requests a remote agent to send the last message of the specified topic
        LastMessageRequest = 7,
        // Response to LastMessageRequest. Contains the result of the request and, if successful,
        // a buffer with the last message.
        LastMessageResult = 8,
    };

    const Type type;

    Payload(Type type) : type(type) {}
    virtual ~Payload() {}

    static uint32_t BaseSize() { return sizeof(int16_t); }

    virtual uint32_t Size() const { return BaseSize(); }
    virtual bool Serialize(BufferWriter& bw) const {
        (void)bw;
        return true;
    }

    // Overrides should fail if deserialization is not possible, typically due to insufficient
    // data available.
    virtual bool Deserialize(BufferReader& br) {
        (void)br;
        return true;
    }

    template <typename T>
    const T* As() const {
        if (type != T::payload_type) {
            return nullptr;
        }
        return static_cast<const T*>(this);
    }
};

struct Payload_Ping : public Payload {
    static constexpr Type payload_type = Type::Ping;

    uint32_t version_number = kVersionNumber;
    std::string vm_name;
    Payload_Ping() : Payload(Ping) {}
    Payload_Ping(const std::string& vm_name) : Payload(Ping), vm_name(vm_name) {}

    uint32_t Size() const override {
        return Payload::Size() +
                static_cast<uint32_t>(sizeof(version_number) +
                                      /* vm_name length*/ sizeof(uint16_t) + vm_name.length());
    }

    bool Serialize(BufferWriter& bw) const override {
        return bw.Write(version_number) && bw.Write(vm_name);
    }

    bool Deserialize(BufferReader& br) override {
        return br.Read(version_number) && br.Read(vm_name);
    }
};

struct Payload_Pong : public Payload {
    static constexpr Type payload_type = Type::Pong;

    Payload_Pong() : Payload(Pong) {}

    uint32_t Size() const override { return Payload::Size(); }

    bool Serialize(BufferWriter&) const override { return true; }

    bool Deserialize(BufferReader&) override { return true; }
};

struct Payload_SubscribeRequest : public Payload {
    static constexpr Type payload_type = Type::SubscribeRequest;

    std::string topic_name;
    int32_t subscriber_pub_id;

    Payload_SubscribeRequest() : Payload(SubscribeRequest) {}

    Payload_SubscribeRequest(const std::string& topic_name, int32_t subscriber_pub_id) :
          Payload(SubscribeRequest), topic_name(topic_name), subscriber_pub_id(subscriber_pub_id) {}

    uint32_t Size() const override {
        return Payload::Size() +
                static_cast<uint32_t>(sizeof(uint16_t) + topic_name.length() +
                                      sizeof(subscriber_pub_id));
    }

    bool Serialize(BufferWriter& bw) const override {
        // TODO: don't modify write pointer in case of failure
        return bw.Write(topic_name) && bw.Write(subscriber_pub_id);
    }

    bool Deserialize(BufferReader& br) override {
        return br.Read(topic_name) && br.Read(subscriber_pub_id);
    }
};

struct Payload_SubscribeResult : public Payload {
    static constexpr Type payload_type = Type::SubscribeResult;

    static constexpr uint16_t Result_Ok = 0;
    static constexpr uint16_t Result_Failure = 1;
    uint16_t result;
    uint32_t message_size;
    uint32_t message_count;
    int32_t subscriber_pub_id;
    uint8_t has_last_message = 0;
    RawBuffer last_message_content;

    Payload_SubscribeResult() : Payload(SubscribeResult) {}

    uint32_t Size() const override {
        return Payload::Size() +
                static_cast<uint32_t>(
                        sizeof(result) +                         // result
                        sizeof(message_size) +                   // message_size
                        sizeof(message_count) +                  // message_count
                        sizeof(subscriber_pub_id) +              // subscriber_pub_id
                        sizeof(has_last_message) +               // has_last_message
                        (has_last_message ? message_size : 0));  // last_message_content
    }

    bool Serialize(BufferWriter& bw) const override {
        // TODO: don't modify write pointer in case of failure
        if (has_last_message && last_message_content.Size() != message_size) {
            ALOGE("Error serializing SubscribeResult for publication %d:  last_message_content "
                  "size %zu does not match message_size %u.",
                  subscriber_pub_id, last_message_content.Size(), message_size);
            return false;
        }
        return bw.Write(result) && bw.Write(message_size) && bw.Write(message_count) &&
                bw.Write(subscriber_pub_id) && bw.Write(has_last_message) &&
                (has_last_message ? bw.Write(last_message_content.Ptr(), message_size) : true);
    }

    bool Deserialize(BufferReader& br) override {
        if (!(br.Read(result) && br.Read(message_size) && br.Read(message_count) &&
              br.Read(subscriber_pub_id) && br.Read(has_last_message))) {
            return false;
        }
        if (!has_last_message) {
            return true;
        }
        if (br.AvailableToRead() < message_size) {
            return false;
        }
        if (last_message_content.Size() < message_size) {
            last_message_content.Resize(message_size, false);
        }
        return br.Read(last_message_content.Ptr(), message_size);
    }
};

struct Payload_TopicRawData : public Payload {
    static constexpr Type payload_type = Type::TopicRawData;

    Payload_TopicRawData(uint32_t data_size) :
          Payload(TopicRawData), data_size(data_size), buffer(data_size) {}

    int32_t subscriber_pub_id;
    uint32_t data_size;  // to be able to reuse the buffer with different buffers
    RawBuffer buffer;

    static uint32_t PayloadSize(uint32_t message_size) {
        return Payload::BaseSize() +
                static_cast<uint32_t>(sizeof(subscriber_pub_id) + sizeof(data_size) + message_size);
    }

    uint32_t Size() const override { return PayloadSize(data_size); }

    bool Serialize(BufferWriter& bw) const override {
        // TODO: don't modify write pointer in case of failure
        return bw.Write(subscriber_pub_id) && bw.Write(data_size) &&
                bw.Write(buffer.Ptr(), data_size);
    }

    bool Deserialize(BufferReader& br) override {
        if (!br.Read(subscriber_pub_id) || !br.Read(data_size)) {
            return false;
        }
        if (br.AvailableToRead() < data_size) {
            return false;
        }
        if (buffer.Size() < data_size) {
            buffer.Resize(data_size, false);
        }
        return br.Read(buffer.Ptr(), data_size);
    }
};

struct Payload_TopicDestroyed : public Payload {
    static constexpr Type payload_type = Type::TopicDestroyed;

    int32_t subscriber_pub_id;

    Payload_TopicDestroyed() : Payload(TopicDestroyed) {}

    Payload_TopicDestroyed(int32_t subscriber_pub_id) :
          Payload(TopicDestroyed), subscriber_pub_id(subscriber_pub_id) {}

    uint32_t Size() const override { return Payload::Size() + sizeof(subscriber_pub_id); }

    bool Serialize(BufferWriter& bw) const override {
        // TODO: don't modify write pointer in case of failure
        return bw.Write(subscriber_pub_id);
    }

    bool Deserialize(BufferReader& br) override { return br.Read(subscriber_pub_id); }
};

struct Payload_LastMessageRequest : public Payload {
    static constexpr Type payload_type = Type::LastMessageRequest;

    std::string topic_name;

    Payload_LastMessageRequest() : Payload(LastMessageRequest) {}

    Payload_LastMessageRequest(const std::string& topic_name) :
          Payload(LastMessageRequest), topic_name(topic_name) {}

    uint32_t Size() const override {
        return Payload::Size() + static_cast<uint32_t>(sizeof(uint16_t) + topic_name.length());
    }

    bool Serialize(BufferWriter& bw) const override {
        // TODO: don't modify write pointer in case of failure
        return bw.Write(topic_name);
    }

    bool Deserialize(BufferReader& br) override { return br.Read(topic_name); }
};

struct Payload_LastMessageResult : public Payload {
    static constexpr Type payload_type = Type::LastMessageResult;

    Payload_LastMessageResult() : Payload(LastMessageResult) {}

    static constexpr uint16_t Result_Ok = 0;
    static constexpr uint16_t Result_Failure = 1;

    std::string topic_name;
    uint16_t result;
    RawBuffer buffer;

    static uint32_t PayloadSizeForTopic(const std::string& topic_name, uint32_t message_size) {
        return Payload::BaseSize() +
                static_cast<uint32_t>(sizeof(uint16_t) + topic_name.length() + sizeof(uint16_t) +
                                      sizeof(uint32_t) + message_size);
    }

    uint32_t Size() const override {
        return PayloadSizeForTopic(topic_name, static_cast<uint32_t>(buffer.Size()));
    }

    bool Serialize(BufferWriter& bw) const override {
        // TODO: don't modify write pointer in case of failure
        return bw.Write(topic_name) && bw.Write(result) &&
                bw.Write(static_cast<uint32_t>(buffer.Size())) &&
                bw.Write(buffer.Ptr(), buffer.Size());
    }

    bool Deserialize(BufferReader& br) override {
        uint32_t buffersize;
        if (!br.Read(topic_name) || !br.Read(result) || !br.Read(buffersize)) {
            return false;
        }
        if (br.AvailableToRead() < buffersize) {
            return false;
        }
        if (buffer.Size() < buffersize) {
            buffer.Resize(buffersize, false);
        }
        return br.Read(buffer.Ptr(), buffersize);
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
            ALOGE("DeserializePayloadFromBuffer: Error deserializing payload of type %d", type);
            delete payload;
            payload = nullptr;
        }
    } else {
        ALOGE("DeserializePayloadFromBuffer: Unknown payload type %d", type);
    }

    return std::unique_ptr<Payload>(payload);
}

struct Parcel {
    enum class ParseResult { Success, UnknownPayload, Failure };

    // Magic number to avoid inadvertently interpreting arbitrary data as a Parcel.
    static constexpr uint32_t Header = 0xC00010FF;
    static constexpr uint32_t HeaderSize = sizeof(Header);

    Parcel() = default;

    static uint32_t ParcelSize(uint32_t payload_size) { return HeaderSize + payload_size; }

    static uint32_t ParcelSize(const Payload& payload) { return ParcelSize(payload.Size()); }

    static ParseResult CanReadParcel(const BufferReader& br) {
        uint32_t header = 0;
        BufferReader temp_reader(br);
        if (!temp_reader.Read(header) || header != Header) {
            return ParseResult::Failure;
        }
        return ParseResult::Success;
    }

    bool Serialize(BufferWriter& bw, const Payload& payload) const {
        return (ParcelSize(payload) <= bw.Available()) && bw.Write(Header) &&
                bw.Write(static_cast<int16_t>(payload.type)) && payload.Serialize(bw);
    }

    static std::pair<ParseResult, std::unique_ptr<Payload>> Deserialize(BufferReader& br) {
        const auto* data = br.CurPtr();
        auto available = br.AvailableToRead();

        uint32_t payload_size;
        // Ensure the parcel isn't corrupted and it's enough data to deserialize
        auto res = CanReadParcel(br);
        if (res != ParseResult::Success) {
            ALOGE("Parcel::Deserialize got invalid parcel.  First of %zu bytes: %s", available,
                  BufferDebugString({data, available}, 24).c_str());
            return {res, std::unique_ptr<Payload>(nullptr)};
        }

        if (!br.MovePos(sizeof(uint32_t))) {
            return {ParseResult::Failure, std::unique_ptr<Payload>(nullptr)};
        }

        auto payload = DeserializePayloadFromBuffer(br);
        if (!payload) {
            ALOGE("Parcel::Deserialize failed to deserialize payload.  First of %zu bytes: %s",
                  available, BufferDebugString({data, available}, 24).c_str());

            // Returning UnknownPayload instead of Failure because the parcel isn't corrupted, but
            // the payload is unknown and can't be parsed, probably, because the counteragent has a
            // newer protocol version.
            return {ParseResult::UnknownPayload, std::unique_ptr<Payload>()};
        }

        return {ParseResult::Success, std::move(payload)};
    }
};

}  // namespace android::sdv::dt::agent::proto
