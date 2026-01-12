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

#include <android-base/logging.h>
#include <sdv/sd_callbacks.h>
#include <sdv/unit_discovery.h>
#include <sdv/unit_metadata.h>
#include <sdv/unit_registration.h>
#include <sdv/unit_type.h>

#include <atomic>

#include <callback_helpers.hpp>
#include <ffi_helpers.hpp>
#include <sdv/sd_lookup.hpp>

using android::base::unexpected;
using sdv::SdvResult;
using sdv::SdvStatus;
using sdv::SdvStatusCode;

namespace sdv_comms::sd {

// ServiceDiscoveryManager implementation

ServiceDiscoveryManager::ServiceDiscoveryManager(const ctx::Context& context) : context_(context) {}

namespace {

// ServiceUnitListAccumulator is a helper for the ListUnits*Async methods.
//
// The listUnitsBy* ABI functions return their results using a repeated callback,
// but we give C++ SDK clients APIs taking a once callback that delivers all results
// in a single call, since C++ does not provide a convenient way to "iterate" over a
// finite list of asynchronously-arriving results.
//
// This class does two things to enable that:
//
// - Collects ServiceUnitDefinition instances provided by the repeated callback the
//   underlying ABI functions fire, then delivers all SDUs to the client in a single
//   execution of a client-provided callback when the ABI callback gets called with
//   nullopt (indicating clear() received).
//
// - Enables routing of failures returned by ABI function to the client callback (as
//   happens with other C++ APIs that take once callbacks).
//
//   If we get the final ABI callback from the ABI with no SDUs reported, it could
//   be because of an error, or because there were no matching service units.  To
//   distinguish those two situations we need the status returned by the ABI function.
//
//   So to send a result to the client callback, we reach one of two states;
//     - (success) Got final callback from ABI *and* ABI function returned OK status.
//     - (failure) ABI function returned non-OK status.
//
//   This class tracks which of these conditions have been met and only sends the client
//   callback when we transition to one of those states.
//
//   The alternative here would be to return the status from the ABI function as the
//   return value of the ListUnits*Async methods, and always fire the client callback
//   with a non-error result, but then the client would have to deal with the race
//   between the callback and the function return.  Better to take care of it for them.
class ServiceUnitListAccumulator {
public:
    explicit ServiceUnitListAccumulator(std::function<void(ServiceUnitListResult)> client_cb) :
          client_callback_(std::move(client_cb)) {}

    // Called on each firing of the ABI Callback.
    SdvStatusCode HandleAbiCallback(std::optional<ServiceUnitDefinition> sud) {
        if (sud) {
            sud_results_.emplace_back(std::move(*sud));
            return sdv::SdvStatusCode::kOk;
        }

        // Got nullopt signalling end of results.  Need to check if we already got an OK
        // status reported for the ABI func before passing result list back to client.

        // Atomically set flag indicating that we got the terminal callback, and get the
        // immediately preceding state.
        auto previous_state = completion_state_.fetch_or(kGotTerminalCallback);

        // Check whether we got multiple terminal callbacks, indicating programming error.
        if (previous_state & kGotTerminalCallback) {
            LOG(ERROR) << "ServiceUnitListAccumulator got multiple clears.";
        }

        // If we already had OK status then we just met all requirements to pass success result to
        // the client.
        //
        // We check that the previous state was exactly kGotOkStatus (instead of using bitwise-and
        // to see if it was set without considering other bits), so that we don't fire the client
        // callback multiple times in the event of a programming error that causes us to get
        // multiple clear() calls.
        if (previous_state == kGotOkStatus) {
            client_callback_(std::move(sud_results_));
        }

        return sdv::SdvStatusCode::kOk;
    }

    // Called after the ABI function returns.
    void ReportAbiFuncStatus(sdv_status_t status) {
        if (status != ASDV_STATUS_OK) {
            // Don't need to check for the terminal callback on bad status.
            client_callback_(unexpected(SdvStatus(status)));
            return;
        }

        // Atomically set flag indicating that we got an OK return from the ABI function, and
        // get the immediately preceding state.
        auto previous_state = completion_state_.fetch_or(kGotOkStatus);

        // Check whether we got multiple OK status reports, indicating programming error.
        if (previous_state & kGotOkStatus) {
            LOG(ERROR) << "ServiceUnitListAccumulator::ReportAbiFuncStatus called multiple times.";
        }

        // If we already got the terminal callback then we just met all requirements to pass success
        // result to the client.
        //
        // We check that the previous state was exactly kGotTerminalCallback (instead of using
        // bitwise-and to see if it was set without considering other bits), so that we don't fire
        // the client callback multiple times in the event ReportAbiFuncStatus is mistakenly
        // called multiple times..
        if (previous_state == kGotTerminalCallback) {
            client_callback_(std::move(sud_results_));
        }
    }

    ~ServiceUnitListAccumulator() {
        if (!sud_results_.empty()) {
            // If we get to this state, it means that we somehow got Func callbacks
            // providing SDUs, but the ABI function returned an error so we sent that
            // error to the client callback instead of the results.
            LOG(ERROR) << "ServiceUnitListAccumulator destroyed with unreported results.";
        }
    }

private:
    std::vector<ServiceUnitDefinition> sud_results_;  // Build result here.
    std::function<void(ServiceUnitListResult)> client_callback_;

    // Completion state tracking.
    //
    // Think of this as a 4-state FSM:
    //
    // 0: No bits set (initial)
    // 1: Only kGotOkStatus set
    // 2: Only kGotTerminalCallback set
    // 3: Both kGotOkStatus and kGotTerminalStatus set
    //
    // On any state change we want to check whether we have just transitioned to state 3
    // from some other state *for the first time*.
    //
    // So we use std::atomic which allows us to atomically update completion_state_ and retrieve its
    // previous value.  Then we check if the previous value consisted of state 3 minus the bit we
    // just set.  If so, we just moved into state 3 and should fire the callback.
    //
    // NOTE: These constants are flags that will be represented as bits in completion_state;
    // any new values must be distinct powers of two.
    static const uint32_t kGotOkStatus = 0x1;
    static const uint32_t kGotTerminalCallback = 0x2;
    std::atomic<uint32_t> completion_state_ = 0;
};

// Construct an accumulator that will fire the given client_cb on termination, and
// also construct a callback suitable for passing to the ABI as a vtable that
// will forward to the accumulator.
auto MakeAccumulatorAndCallback(std::function<void(ServiceUnitListResult)> client_cb) {
    // shared_ptr so we can capture accumulator in std::function and share it between
    // the callback and the calling code.
    auto acc = std::make_shared<ServiceUnitListAccumulator>(std::move(client_cb));

    // Callback to be passed to ABI func via vtable.
    std::function<sdv::SdvStatusCode(std::optional<ServiceUnitDefinition>)> acc_cb =
            [acc](auto sud) { return acc->HandleAbiCallback(std::move(sud)); };

    return std::make_pair(std::move(acc), std::move(acc_cb));
}

}  // namespace

void ServiceDiscoveryManager::ListUnitsByTypeAsync(
        const UnitType& unit_type,
        std::function<void(ServiceUnitListResult)> client_callback) const {
    auto [acc, acc_cb] = MakeAccumulatorAndCallback(std::move(client_callback));
    auto abi_cb = MakeRepeatedABICallback<ServiceUnitDefinition::FromABIPtr>(std::move(acc_cb));
    auto vtable =
            MakeRepeatedVTable<ASDV_ServiceDiscoveryManager_listUnitsByType, 2>(std::move(abi_cb));
    auto status = ASDV_ServiceDiscoveryManager_listUnitsByType(context_.ToABIPtr(),
                                                               ToFFI(unit_type), vtable);
    acc->ReportAbiFuncStatus(status);
}

std::future<ServiceUnitListResult> ServiceDiscoveryManager::ListUnitsByTypeFuture(
        const UnitType& unit_type) const {
    auto [result_future, callback] = MakeFutureSatisfyingCallback<ServiceUnitListResult>();
    ListUnitsByTypeAsync(unit_type, std::move(callback));
    return std::move(result_future);
}

ServiceUnitListResult ServiceDiscoveryManager::ListUnitsByType(const UnitType& unit_type) const {
    return ListUnitsByTypeFuture(unit_type).get();
}

void ServiceDiscoveryManager::ListUnitsByNameAsync(
        const UnitNameDiscoveryArgs& unit_name_args,
        std::function<void(ServiceUnitListResult)> client_callback) const {
    auto [acc, acc_cb] = MakeAccumulatorAndCallback(std::move(client_callback));
    auto abi_cb = MakeRepeatedABICallback<ServiceUnitDefinition::FromABIPtr>(std::move(acc_cb));
    auto vtable =
            MakeRepeatedVTable<ASDV_ServiceDiscoveryManager_listUnitsByName, 2>(std::move(abi_cb));
    auto status = ASDV_ServiceDiscoveryManager_listUnitsByName(context_.ToABIPtr(),
                                                               ToFFI(unit_name_args), vtable);
    acc->ReportAbiFuncStatus(status);
}

std::future<ServiceUnitListResult> ServiceDiscoveryManager::ListUnitsByNameFuture(
        const UnitNameDiscoveryArgs& unit_name_args) const {
    auto [result_future, callback] = MakeFutureSatisfyingCallback<ServiceUnitListResult>();
    ListUnitsByNameAsync(unit_name_args, std::move(callback));
    return std::move(result_future);
}

ServiceUnitListResult ServiceDiscoveryManager::ListUnitsByName(
        const UnitNameDiscoveryArgs& unit_name_args) const {
    return ListUnitsByNameFuture(unit_name_args).get();
}

void ServiceDiscoveryManager::GetUnitAsync(const id::ServiceFqin& fqin,
                                           const std::string& unit_name,
                                           std::function<void(ServiceUnitResult)> callback) const {
    // Note copy rather than move callback since we may call it below on error.
    auto abi_cb = MakeOnceABICallback<ServiceUnitDefinition::FromABIPtr>(callback);
    auto vtable = MakeOnceVTable<ASDV_ServiceDiscoveryManager_getUnit, 3>(std::move(abi_cb));
    auto status =
            ASDV_ServiceDiscoveryManager_getUnit(context_.ToABIPtr(), ToFFI(fqin),
                                                 (const unsigned char*)unit_name.c_str(), vtable);
    if (status != ASDV_STATUS_OK) {
        // ABI won't call func() on failure, so fire the callback ourselves with the error.
        callback(unexpected(SdvStatus(status)));
    }
}

std::future<ServiceUnitResult> ServiceDiscoveryManager::GetUnitFuture(
        const id::ServiceFqin& fqin, const std::string& unit_name) const {
    auto [result_future, callback] = MakeFutureSatisfyingCallback<ServiceUnitResult>();
    GetUnitAsync(fqin, unit_name, std::move(callback));
    return std::move(result_future);
}

ServiceUnitResult ServiceDiscoveryManager::GetUnit(const id::ServiceFqin& fqin,
                                                   const std::string& unit_name) const {
    return GetUnitFuture(fqin, unit_name).get();
}

}  // namespace sdv_comms::sd
