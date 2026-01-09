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
#include <vector>

#include <sdv/context.hpp>
#include <sdv/identity.hpp>
#include <sdv/status.hpp>

namespace sdv_comms::dt {

class InstantReader {
public:
    using Message = std::vector<uint8_t>;
    using Result = sdv::SdvResult<Message>;

    /**
     * Non-blocking.
     * Retrieves the last published message in the specified publication without
     * subscribing to it, or returns an error if the publication is empty.
     *
     * Parameters:
     *    [IN] context: Reference to the local SDV Comm Stack Context object
     *    [IN] unit_id: Unit Identifier of the publication to read data from
     *    [IN] callback: Callback to receive the async read result.
     *
     * Return value:
     *    None; callback will be invoked exactly once with an SdvResult that
     *    contains SdvStatus of the method call and, if method succeeded, the
     *    last Message of the specified publication.
     */
    static void GetLastMessageAsync(const ctx::Context& context, const id::UnitIdentifier& unit_id,
                                    std::function<void(Result)> callback);

    /**
     * Non-blocking.
     * Retrieves the last published message in the specified publication without
     * subscribing to it, or returns an error if the publication is empty.
     *
     * Parameters:
     *    [IN] context: Reference to the local SDV Comm Stack Context object
     *    [IN] unit_id: Unit Identifier of the publication to read data from
     *
     * Return value:
     *    Future for SdvResult that contains SdvStatus of the method call and,
     *    if method succeeded, the last Message of the specified publication.
     */
    static std::future<Result> GetLastMessageFuture(const ctx::Context& context,
                                                    const id::UnitIdentifier& unit_id);

    /**
     * Remote blocking.
     * Retrieves the last published message in the specified publication without
     * subscribing to it, or returns an error if the publication is empty.
     *
     * Parameters:
     *    [IN] context: Reference to the local SDV Comm Stack Context object
     *    [IN] unit_id: Unit Identifier of the publication to read data from
     *
     * Return value:
     *    SdvResult that contains SdvStatus of the method call and, if method
     *    succeeded, the last Message of the specified publication.
     */
    static Result GetLastMessage(const ctx::Context& context, const id::UnitIdentifier& unit_id);
};

}  // namespace sdv_comms::dt
