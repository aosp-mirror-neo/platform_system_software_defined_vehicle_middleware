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
#include <sdv/sd_registration.hpp>
#include <sdv/status.hpp>

struct ASDV_PublicationHandle;

namespace sdv_comms::dt {

/**
 *  NOTE: All public methods of Publisher are thread-safe and can be
 *  called from different threads without risk of data races.
 */
class Publisher {
public:
    // Moveable, not copyable.
    Publisher(Publisher&&);
    Publisher(const Publisher&) = delete;

    // Destructor automatically unsubscribes and cleans up underlying record.
    ~Publisher();

    // Test whether an instance contains a valid reference.
    // An instance will become invalid when moved-from.
    bool IsValid() const;

    using CreateResult = sdv::SdvResult<Publisher>;

    /**
     * Non-blocking.
     * Registers and creates a new DataTunnel publication with the provided
     * UnitRegistrationArgs.
     *
     * NOTE: The callback must not block or engage in long-running computation.
     *
     * Parameters:
     *    [IN] context: Reference to the local SDV Comm Stack Context object
     *    [IN] unit_reg_args: Unit registration arguments of the new publication;
     *                        Ownership of this object is passed to the Publisher.
     *    [IN] callback: Callback to receive the async create result.
     *
     * Return value:
     *    None; Callback will be invoked exactly once with SdvResult containing
     *    either a valid Publisher or a non-OK SdvStatus.
     */
    static void CreateAsync(const ctx::Context& context, sd::UnitRegistrationArgs unit_reg_args,
                            std::function<void(CreateResult)> callback);

    /**
     * Non-blocking.
     * Registers and creates a new DataTunnel publication with the provided
     * UnitRegistrationArgs.
     *
     * Parameters:
     *    [IN] context: Reference to the local SDV Comm Stack Context object
     *    [IN] unit_reg_args: Unit registration arguments of the new publication;
     *                        Ownership of this object is passed to the Publisher.
     *
     * Return value:
     *    Future for SdvResult that will contain either a valid Publisher or a
     *    non-OK SdvStatus.
     */
    static std::future<CreateResult> CreateFuture(const ctx::Context& context,
                                                  sd::UnitRegistrationArgs unit_reg_args);

    /**
     * Remote blocking.
     * Registers and creates a new DataTunnel publication with the provided
     * UnitRegistrationArgs.
     *
     * Parameters:
     *    [IN] context: Reference to the local SDV Comm Stack Context object
     *    [IN] unit_reg_args: Unit registration arguments of the new publication;
     *                        Ownership of this object is passed to the Publisher.
     *
     * Return value:
     *    SdvResult containing either a valid Publisher or a non-OK SdvStatus.
     */
    static CreateResult Create(const ctx::Context& context, sd::UnitRegistrationArgs unit_reg_args);

    /**
     * Non blocking.
     * Returns the publication metadata that contains message size in
     * bytes along with other publication related metadata.
     *
     * Parameters:
     *
     * Return value:
     *    Copy of publisher's PublisherUnitMetadata, !IsValid() on error.
     */
    sd::PublisherUnitMetadata GetMetadata() const;

    /**
     * Non blocking.
     * Writes messages to the publication's buffer.
     *
     * Parameters:
     *    [IN] messages: Pointer to the buffer from which the data will be copied
     *    [IN] buffer_size: Size of buffer (in bytes) pointed to by messages
     *
     * Return value:
     *    SdvResult that contains SdvStatus of the method call and, if method
     *    succeeded, the number of messages that been written to the publication.
     */
    SdvResult<uint32_t> WriteMessages(const void* _Nonnull messages, uint32_t buffer_size);

private:
    explicit Publisher(ASDV_PublicationHandle* _Nullable);
    Publisher() = delete;

    ASDV_PublicationHandle* _Nullable handle_ = nullptr;
};

}  // namespace sdv_comms::dt
