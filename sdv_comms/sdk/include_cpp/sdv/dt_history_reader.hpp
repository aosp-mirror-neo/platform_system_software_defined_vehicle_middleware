// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
#pragma once

#include <functional>
#include <future>

#include <sdv/context.hpp>
#include <sdv/identity.hpp>
#include <sdv/sd_common.hpp>
#include <sdv/status.hpp>

struct ASDV_HistoryHandle;

namespace sdv_comms::dt {

/**
 *  NOTE: All public methods of HistoryReader are thread-safe and can be
 *  called from different threads without risk of data races.
 */
class HistoryReader {
public:
    // Moveable, not copyable.
    HistoryReader(HistoryReader&&);
    HistoryReader(const HistoryReader&) = delete;

    // Destructor cleans up underlying record.
    ~HistoryReader();

    // Test whether an instance contains a valid reference.
    // An instance will become invalid when moved-from.
    bool IsValid() const;

    using CreateResult = sdv::SdvResult<HistoryReader>;

    /**
     * Non-blocking.
     * Subscribes to a publication specified in unit_id and creates a "history_size"
     * aggregator for the publication.
     * Creating more than one HistoryReader instances for the same ServiceUnitIdentity
     * will create independent objects with separate buffers and a subscription related
     * resources.
     *
     * NOTE: The callback must not block or engage in long-running computation.
     *
     * Parameters:
     *    [IN] context: Reference to the local SDV Comm Stack Context object
     *    [IN] unit_id: Unit Identifier of the publication to subscribe
     *    [IN] history_size: Depth of the history aggregator in messages
     *    [IN] callback: Callback to receive the async create result.
     *
     * Return value:
     *    None; Callback will be invoked exactly once with SdvResult containing
     *    either a valid HistoryReader or a non-OK SdvStatus.
     */
    static void CreateAsync(const ctx::Context& context, const id::UnitIdentifier& unit_id,
                            const uint32_t history_size,
                            std::function<void(CreateResult)> callback);

    /**
     * Non-blocking.
     * Subscribes to a publication specified in unit_id and creates a "history_size"
     * aggregator for the publication.
     * Creating more than one HistoryReader instances for the same ServiceUnitIdentity
     * will create independent objects with separate buffers and a subscription related
     * resources.
     *
     * Parameters:
     *    [IN] context: Reference to the local SDV Comm Stack Context object
     *    [IN] unit_id: Unit Identifier of the publication to subscribe
     *    [IN] history_size: Depth of the history aggregator in messages
     *
     * Return value:
     *    Future for SdvResult that will contain either a valid HistoryReader or
     *    a non-OK SdvStatus.
     */
    static std::future<CreateResult> CreateFuture(const ctx::Context& context,
                                                  const id::UnitIdentifier& unit_id,
                                                  const uint32_t history_size);

    /**
     * Remote blocking.
     * Subscribes to a publication specified in unit_id and creates a "history_size"
     * aggregator for the publication.
     * Creating more than one HistoryReader instances for the same ServiceUnitIdentity
     * will create independent objects with separate buffers and a subscription related
     * resources.
     *
     * Parameters:
     *    [IN] context: Reference to the local SDV Comm Stack Context object
     *    [IN] unit_id: Unit Identifier of the publication to subscribe
     *    [IN] history_size: Depth of the history aggregator in messages
     *
     * Return value:
     *    SdvResult containing either a valid HistoryReader or a non-OK SdvStatus.
     */
    static CreateResult Create(const ctx::Context& context, const id::UnitIdentifier& unit_id,
                               const uint32_t history_size);

    /**
     * Non blocking.
     * Returns the publication metadata that contains message size in
     * bytes along with other publication related metadata.
     *
     * Parameters:
     *
     * Return value:
     *    A copy of the publisher's metadata, or a !IsValid() instance on error.
     */
    sd::PublisherUnitMetadata GetMetadata() const;

    /**
     * Non blocking.
     * Reports the number of messages available in the history buffer.
     *
     * Parameters:
     *
     * Return value:
     *    SdvResult that contains SdvStatus of the method call and, if
     *    method succeeded, the amount of messages available in the
     *    history buffer.
     */
    SdvResult<uint32_t> AvailableToRead() const;

    /**
     * Non blocking.
     * Reads messages from the publication's history buffer.
     * Reading the last messages is non-destructive, i.e. the same message can
     * be read multiple times.
     *
     * Parameters:
     *    [OUT] messages: Pointer to the buffer to which the data will be copied.
     *                    Must be large enough to store the requested # of messages.
     *    [IN] buffer_size: Size of the messages buffer in bytes
     *
     * Return value:
     *    SdvResult that contains SdvStatus of the method call and, if method
     *    succeeded, the number of messages that were read from the
     *    history_buffer and stored to the "messages" buffer.
     */
    SdvResult<uint32_t> ReadLastMessages(uint8_t* _Nonnull messages,
                                         const uint32_t buffer_size) const;

private:
    explicit HistoryReader(ASDV_HistoryHandle* _Nullable);
    HistoryReader() = delete;

    ASDV_HistoryHandle* _Nullable handle_ = nullptr;
};

}  // namespace sdv_comms::dt
