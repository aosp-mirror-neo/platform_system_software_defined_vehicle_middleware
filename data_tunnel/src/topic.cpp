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

#define LOG_TAG "sdv_dt_agent:topic"

#include "topic.h"

#include <android-base/logging.h>

namespace android::sdv::dt::agent {

Topic::Reader::Reader(std::unique_ptr<AidlMQ> mq) : mq_(std::move(mq)) {
    CHECK(mq_) << "TopicReader constructed with empty MQ!";
}

size_t Topic::Reader::AvailableToRead() {
    return mq_->availableToRead();
}

bool Topic::Reader::Read(uint8_t* buffer_pointer, size_t bytes_to_read) {
    return mq_->read(reinterpret_cast<int8_t*>(buffer_pointer), bytes_to_read);
}

Topic::Topic(const Topic::Metadata& metadata, int32_t id, const core::ClientId& client_id,
             bool is_remote, std::unique_ptr<AidlMQ> mq) :
      id_(id),
      client_id_(client_id),
      metadata_(metadata),
      is_remote_(is_remote),
      is_ready_(!is_remote),
      mq_(std::move(mq)) {}

// Returns an empty unique_ptr on error.
std::unique_ptr<AidlMQ> Topic::CloneMQHandle() {
    std::lock_guard<std::mutex> guard(lock_);
    if (!mq_) {
        return nullptr;
    }
    auto mq = std::make_unique<AidlMQ>(mq_->dupeDesc(), /*resetPointers=*/false);
    if (!mq->isValid()) {
        return nullptr;
    }
    return mq;
}

void Topic::Delete() {
    std::unique_lock<std::mutex> guard(lock_);

    // Notify all the subscribers then clear callbacks.
    for (auto& [_, callback_fn] : subscriber_callbacks_) {
        callback_fn(core::CommChannel::Message::Type::Exception, id_);
    }
    subscriber_callbacks_.clear();

    // Change active state to Deleted
    status_ = Status::Deleted;

    // Clear FMQ
    ALOGD("Releasing FMQ handle for publication %d.", id_);
    mq_.reset();

    // Let anyone waiting for readiness know that it's not happening.
    auto callbacks = std::exchange(on_ready_callbacks_, {});
    guard.unlock();
    for (auto& cb : callbacks) {
        cb(::sdv::SdvStatusCode::kUnavailable);
    }
}

void Topic::ReportReady() {
    std::unique_lock<std::mutex> guard(lock_);
    if (is_ready_) {
        // Harmless to get multiple reports for the same topic.
        // This can happen on suspend / resume since we automatically re-subscribe.
        return;
    }
    is_ready_ = true;

    auto callbacks = std::exchange(on_ready_callbacks_, {});
    guard.unlock();
    for (auto& cb : callbacks) {
        cb(::sdv::SdvStatusCode::kOk);
    }
}

std::unique_ptr<Topic> Topic::Create(const Topic::Metadata& metadata, int32_t id,
                                     core::ClientId client_id, bool is_remote,
                                     std::unique_ptr<AidlMQ> mq) {
    return std::unique_ptr<Topic>(new Topic(metadata, id, client_id, is_remote, std::move(mq)));
}

int32_t Topic::GetId() const {
    return id_;
}
core::ClientId Topic::GetClientId() const {
    return client_id_;
}
const Topic::Metadata& Topic::GetMetadata() const {
    return metadata_;
}
bool Topic::GetIsRemote() const {
    return is_remote_;
}

bool Topic::IsActive() const {
    return status_ == Status::Active;
}

bool Topic::MakeDescriptor(PublicationDescriptor* publication_desc) const {
    std::lock_guard<std::mutex> guard(lock_);
    if (!mq_) {
        return false;
    }
    *publication_desc = {id_, static_cast<int32_t>(metadata_.message_size),
                         static_cast<int32_t>(metadata_.message_count), mq_->dupeDesc()};
    return true;
}

// TODO(b/406556288):  Remove this method and have the remote agent hold the writeable AidlMQ.
long Topic::Write(const uint8_t* buffer_pointer, size_t to_read) {
    std::lock_guard<std::mutex> guard(lock_);
    if (!mq_) {
        return 0;
    }
    return mq_->write(reinterpret_cast<const int8_t*>(buffer_pointer), to_read);
}

bool Topic::NotifyWritten(size_t /*written*/) {
    std::lock_guard<std::mutex> guard(lock_);
    if (!IsActive()) return false;
    for (auto& [_, callback_fn] : subscriber_callbacks_) {
        callback_fn(core::CommChannel::Message::DataAvailable, id_);
    }
    return true;
}

std::optional<std::vector<uint8_t>> Topic::GetLastMessage() {
    auto reader = CreateReader();
    if (!reader) {
        ALOGE("Unable to create reader for GetLastMessage() for topic %s!", metadata_.name.c_str());
        return std::nullopt;
    }

    auto available = reader->AvailableToRead();
    if (available == 0) {
        // We don't treat it as an error, just return an empty buffer
        return std::vector<uint8_t>();
    }

    // If CreateReader doesn't return nullopt, the returned reader must have either
    // no data available, or should have the read pointer positioned on the last message.
    // We check >= instead of == in case a new message arrives after CreateReader sets up
    // the read pointer.  This is acceptable since for GetLastMessage there's always a
    // race between positioning the read pointer and doing the actual read.
    CHECK(available >= metadata_.message_size);

    std::vector<uint8_t> result(metadata_.message_size);
    // We try up to 3 times to avoid a spurious failures due to fast writers
    // to a topic with a small message count.
    static const int kMaxAttempts = 2;
    for (int attempt = 0; attempt < kMaxAttempts; attempt++) {
        if (reader->Read(result.data(), metadata_.message_size)) {
            return result;
        }
    }

    return std::nullopt;
}

uint32_t Topic::RegisterSubscriberCallback(Callback callback) {
    std::lock_guard<std::mutex> guard(lock_);
    if (!IsActive()) {
        // Send exception if registration fails.
        callback(core::CommChannel::Message::Exception, id_);
        return 0;
    }
    auto cb_id = subscriber_callbacks_id_counter_++;
    if (!subscriber_callbacks_.try_emplace(cb_id, std::move(callback)).second) {
        // Send exception if registration fails.
        callback(core::CommChannel::Message::Exception, id_);
        return 0;
    }
    // Send initial DataAvailable if there's already data present in the FMQ.
    if (mq_->isValid() && mq_->availableToRead() > 0) {
        callback(core::CommChannel::Message::DataAvailable, id_);
    }
    return cb_id;
}

void Topic::UnregisterSubscriberCallback(uint32_t callback_id) {
    std::lock_guard<std::mutex> guard(lock_);
    if (!IsActive()) return;
    subscriber_callbacks_.erase(callback_id);
}

size_t Topic::GetSubscriberCallbackCount() const {
    std::lock_guard<std::mutex> guard(lock_);
    return subscriber_callbacks_.size();
}

std::optional<Topic::Reader> Topic::CreateReader() {
    auto mq = CloneMQHandle();
    if (!mq) {
        ALOGE("CreateReader (%s): Unable to clone FMQ handle", metadata_.name.c_str());
        return std::nullopt;
    }

    // Try to move the read pointer to the last message if it's not already there.
    static const int kMaxAttempts = 3;
    for (int attempt = 0; attempt < kMaxAttempts; attempt++) {
        // Get the read pointer positioned on the last message, if the queue isn't empty.
        auto available = mq->availableToRead();

        // Verify data alignment.
        if (available % metadata_.message_size != 0) {
            ALOGE("CreateReader (%s): availableToRead value [%zu] misaligned with message size "
                  "[%zu].",
                  metadata_.name.c_str(), available, metadata_.message_size);
            return std::nullopt;
        }

        // If more than one message is available, we need to move read pointer to the last message.
        if (available > metadata_.message_size) {
            auto bytes_to_skip = available - metadata_.message_size;
            AidlMQ::MemTransaction mtx;
            // If either of these failed, we must have been in an overflow state (the only other
            // cause for failure would be insufficient data available, and we compute bytes_to_skip
            // to be smaller than available), and the read pointer will have been reset to
            // bufferSize/2 behind the write pointer, and we should just retry.
            if (!mq->beginRead(bytes_to_skip, &mtx) || !mq->commitRead(bytes_to_skip)) {
                continue;
            }
        }

        return Topic::Reader(std::move(mq));
    }

    ALOGE("CreateReader (%s): failed to set read pointer to last message!", metadata_.name.c_str());

    return std::nullopt;
}

void Topic::RegisterOnReadyCallback(OnReadyCallback on_ready_cb) {
    std::unique_lock<std::mutex> guard(lock_);

    if (mq_ && !is_ready_) {
        on_ready_callbacks_.emplace_back(std::move(on_ready_cb));
        return;
    }
    guard.unlock();

    on_ready_cb(mq_ ? ::sdv::SdvStatusCode::kOk : ::sdv::SdvStatusCode::kUnavailable);
}

}  // namespace android::sdv::dt::agent
