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

#include <memory>
#include <string>
#include <thread>

namespace android::sdv::dt::core {

using TopicId = int32_t;
using ClientId = int32_t;

const TopicId InvalidTopicId = -1;
const ClientId InvalidClientId = -1;

class CommChannel {
public:
    struct Message {
        static constexpr uint16_t HeaderV1 = 0xF0F1;
        enum ExceptionType {
            Exception_ClientDisconnected = 0,
            Exception_TopicDestroyed,
            Exception_Unknown
        };

        enum Type {
            Undefined = 0,
            DataAvailable = 1,         // args[0]: topic_id, args[1]: message count
            Exception = 2,             // args[0]: topic_id, args[1]: exception_type
            SetNotificationState = 3,  // args[0]: topic_id, args[1]: (int)enabled
        };

        Message(Type type, int32_t arg1 = 0, int32_t arg2 = 0, int32_t arg3 = 0) :
              hdr_(HeaderV1), type_(type), args_{arg1, arg2, arg3} {}
        Message() : Message(Undefined) {}

        bool IsValid() const { return hdr_ == HeaderV1 && type_ != Undefined; }

        Type type() const { return static_cast<Type>(type_); }

        static Message DataAvailableMessage(TopicId topic_id, int32_t count) {
            return Message(DataAvailable, topic_id, count);
        }

        static Message ExceptionMessage(TopicId topic_id, ExceptionType code) {
            return Message(Exception, topic_id, code);
        }

        static Message SetNotificationStateMessage(TopicId topic_id, bool enabled) {
            return Message(SetNotificationState, topic_id, enabled ? 1 : 0);
        }

        bool operator==(const Message&) const = default;

        uint16_t hdr_;
        uint16_t type_;
        int32_t args_[3];
    };

    virtual ~CommChannel() = default;
    virtual bool SendMessage(const Message& message) = 0;
    virtual bool IsConnected() const = 0;

    using ReadCallback = void (*)(const Message& message, void*, int32_t);
    static std::unique_ptr<CommChannel> FromPipeFds(int read_pipe_fd, int write_pipe_fd,
                                                    ReadCallback callback, void* ptr_arg,
                                                    int32_t int_arg);
};

}  // namespace android::sdv::dt::core
