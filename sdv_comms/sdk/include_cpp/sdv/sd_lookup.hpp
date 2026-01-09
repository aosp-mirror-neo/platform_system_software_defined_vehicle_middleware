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

#include "context.hpp"
#include "identity.hpp"
#include "sd_common.hpp"

#include <functional>
#include <future>
#include <string_view>
#include <vector>

#include <sdv/status.hpp>

// Classes for doing one-time lookup operations via Service Discovery.

namespace sdv_comms::sd {

using ServiceUnitListResult = sdv::SdvResult<std::vector<ServiceUnitDefinition>>;
using ServiceUnitResult = sdv::SdvResult<ServiceUnitDefinition>;

// Primary service discovery class, provides methods to look up Service Units.
class ServiceDiscoveryManager {
public:
    explicit ServiceDiscoveryManager(const ctx::Context&);

    // Copyable.
    ServiceDiscoveryManager(const ServiceDiscoveryManager&) = default;

    /**
     * Non-Blocking
     *
     * Find Service Units matching the given Unit Type.
     *
     * Parameters:
     *    [IN] unit_type: UnitType of desired Service Units.
     *    [IN] callback: Callback to receive result
     *
     * Return value:
     *    None; the provided callback will be invoked exactly once with
     *    A vector of ServiceUnitDefinitions for matching Service Units,
     *    or a non-Ok SdvStatus if an error was encountered.
     */
    void ListUnitsByTypeAsync(const UnitType& unit_type,
                              std::function<void(ServiceUnitListResult)> callback) const;

    /**
     * Non-Blocking
     *
     * Find Service Units matching the given Unit Type.
     *
     * Parameters:
     *    [IN] unit_type: UnitType of desired Service Units.
     *
     * Return value:
     *    A future for a vector of ServiceUnitDefinitions for matching
     *    Service Units, or a non-Ok SdvStatus if an error was encountered.
     */
    std::future<ServiceUnitListResult> ListUnitsByTypeFuture(const UnitType& unit_type) const;

    /**
     * Remote blocking.
     *
     * Find Service Units matching the given Unit Type.
     *
     * Parameters:
     *    [IN] unit_type: UnitType of desired Service Units.
     *
     * Return value:
     *    A vector of ServiceUnitDefinitions for matching Service Units,
     *    or a non-Ok SdvStatus if an error was encountered.
     */
    ServiceUnitListResult ListUnitsByType(const UnitType& unit_type) const;

    /**
     * Non-Blocking
     *
     * Find Service Units matching the given UnitNameDiscoveryArgs.
     *
     * Parameters:
     *    [IN] unit_name_args: UnitNameDiscoveryArgs to match desired
     *                         Service Units.
     *    [IN] callback: Callback to receive result
     *
     * Return value:
     *    None; the provided callback will be invoked exactly once with
     *    A vector of ServiceUnitDefinitions for matching Service Units,
     *    or a non-Ok SdvStatus if an error was encountered.
     */
    void ListUnitsByNameAsync(const UnitNameDiscoveryArgs& unit_name_args,
                              std::function<void(ServiceUnitListResult)> callback) const;
    /**
     * Non-Blocking
     *
     * Find Service Units matching the given UnitNameDiscoveryArgs.
     *
     * Parameters:
     *    [IN] unit_name_args: UnitNameDiscoveryArgs to match desired
     *                         Service Units.
     *
     * Return value:
     *    A future for a vector of ServiceUnitDefinitions for matching Service
     *    Units, or a non-Ok SdvStatus if an error was encountered.
     */
    std::future<ServiceUnitListResult> ListUnitsByNameFuture(
            const UnitNameDiscoveryArgs& unit_name_args) const;

    /**
     * Remote blocking.
     *
     * Find Service Units matching the given UnitNameDiscoveryArgs.
     *
     * Parameters:
     *    [IN] unit_name_args: UnitNameDiscoveryArgs to match desired
     *                         Service Units.
     *
     * Return value:
     *    A vector of ServiceUnitDefinitions for matching Service Units,
     *    or a non-Ok SdvStatus if an error was encountered.
     */
    ServiceUnitListResult ListUnitsByName(const UnitNameDiscoveryArgs& unit_name_args) const;

    /**
     * Non-blocking.
     *
     * Find a specific Service Unit by FQIN and unit name.
     *
     * Parameters:
     *    [IN] fqin: FQIN of the Service Bundle hosting the desired Service Unit.
     *    [IN] unit_name: name of desired unit.
     *    [IN] callback: Callback to receive result.
     *
     * Return value:
     *    None; callback will be invoked exactly once with a ServiceUnitDefinition
     *    for the matching Service Unit, a kNotFound SdvStatus if no matching unit
     *    was found, or another non-OK SdvStatus if some other failure occurred.
     */
    void GetUnitAsync(const id::ServiceFqin& fqin, const std::string& unit_name,
                      std::function<void(ServiceUnitResult)> callback) const;

    /**
     * Non-blocking.
     *
     * Find a specific Service Unit by FQIN and unit name.
     *
     * Parameters:
     *    [IN] fqin: FQIN of the Service Bundle hosting the desired Service Unit.
     *    [IN] unit_name: name of desired unit.
     *
     * Return value:
     *    A future for a ServiceUnitDefinition for the matching Service Unit, a
     *    kNotFound SdvStatus if no matching unit was found, or another non-OK
     *    SdvStatus if some other failure occurred.
     */
    std::future<ServiceUnitResult> GetUnitFuture(const id::ServiceFqin& fqin,
                                                 const std::string& unit_name) const;

    /**
     * Remote blocking.
     *
     * Find a specific Service Unit by FQIN and unit name.
     *
     * Parameters:
     *    [IN] fqin: FQIN of the Service Bundle hosting the desired Service Unit.
     *    [IN] unit_name: name of desired Service Unit.
     *
     * Return value:
     *    A ServiceUnitDefinition for the matching Service Unit, a kNotFound SdvStatus
     *    if no matching unit was found, or another non-OK SdvStatus if some other
     *    failure occurred.
     */
    ServiceUnitResult GetUnit(const id::ServiceFqin& fqin, const std::string& unit_name) const;

private:
    ServiceDiscoveryManager() = delete;

    ctx::Context context_;
};

}  // namespace sdv_comms::sd
