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

#include <callback_helpers.hpp>
#include <ffi_helpers.hpp>
#include <sdv/sd_listener.hpp>

using android::base::unexpected;
using sdv::SdvResult;
using sdv::SdvStatus;
using sdv::SdvStatusCode;

namespace sdv_comms::sd {

// ServiceUnitChangeListener implementation

ServiceUnitChangeListener::ServiceUnitChangeListener(ServiceUnitChangeListener&& o) :
      context_(o.context_), handle_(o.handle_) {
    o.handle_ = nullptr;
}

ServiceUnitChangeListener::ServiceUnitChangeListener(const ctx::Context& c,
                                                     ASDV_ServiceUnitChangeListenerHandle* h) :
      context_(c), handle_(h) {}

void ServiceUnitChangeListener::Stop() {
    if (handle_) {
        ASDV_ServiceDiscoveryManager_unregisterServiceUnitChangeListener(context_.ToABIPtr(),
                                                                         handle_);
        handle_ = nullptr;
    }
}

bool ServiceUnitChangeListener::IsValid() const {
    return handle_ != nullptr;
}

ServiceUnitChangeListener::~ServiceUnitChangeListener() {
    Stop();
}

static ServiceUnitChangeEvent::Type _ConvertEventType(
        ASDV_ServiceUnitChangeEventType abi_event_type) {
    static const auto& type_map = *new std::unordered_map<
            ASDV_ServiceUnitChangeEventType,
            ServiceUnitChangeEvent::Type>{{ASDV_SERVICE_UNIT_REGISTERED,
                                           ServiceUnitChangeEvent::Type::Registered},
                                          {ASDV_SERVICE_UNIT_UNREGISTERED,
                                           ServiceUnitChangeEvent::Type::Unregistered},
                                          {ASDV_SERVICE_UNIT_CHANGED,
                                           ServiceUnitChangeEvent::Type::Changed}};
    if (!type_map.contains(abi_event_type)) {
        return ServiceUnitChangeEvent::Type::Unknown;
    }
    return type_map.at(abi_event_type);
}

// Common implementation for CreateByTypeAsync and CreateByNameAsync.
//
// The two Create...Async methods differ only in the ABI function they call and in the
// parameter passed as unit match criteria; this function implements all the common
// logic and takes the ABI function and the other parameter types as template parameters
//
// An alternate approach would have been to define a UnitCriteria type for clients that
// is effectively a std::variant of UnitType and UnitNameDiscoveryArgs.  But we haven't
// done that in other places in the C++ or Rust SDK, and given the possibility that we
// define such an abstraction in the Comms core and expose it in the ABI, doing it in
// the SDK seems like a mistake.
template <auto AbiFunc, typename... OtherParams>
void ServiceUnitChangeListener::CreateAsync(const ctx::Context& context,
                                            ListenerCallback listener_cb, CreateCallback create_cb,
                                            OtherParams... other_params) {
    static const auto _ConvertEvent = [](ASDV_ServiceUnitChangeEvent* abi_event) {
        ASDV_ServiceUnitChangeEventType abi_event_type;
        auto status = ASDV_ServiceUnitChangeEvent_getEventType(abi_event, &abi_event_type);
        CHECK(status == ASDV_STATUS_OK);
        auto event_type = _ConvertEventType(abi_event_type);
        CHECK(event_type != ServiceUnitChangeEvent::Type::Unknown);

        ASDV_ServiceUnitDefinition* sud_handle =
                ASDV_ServiceUnitChangeEvent_extractDefinition(abi_event);
        CHECK(sud_handle != nullptr);

        return ServiceUnitChangeEvent{event_type, ServiceUnitDefinition(sud_handle)};
    };
    auto listener_abi_cb = MakeRepeatedABICallback<_ConvertEvent>(std::move(listener_cb));
    // Listener VTable is always second to last.
    auto listener_vtable = MakeRepeatedVTable<AbiFunc, -2>(std::move(listener_abi_cb));

    // Can't use MakeOnceABICallback because we need Context to construct ServiceUnitChangeListener,
    // and therefore a static function can't construct it from ASDV_ServiceUnitChangeListener*.
    //
    // Note copy-capture of create_cb since it may be called directly below.
    auto completion_abi_cb = [context, create_cb](ASDV_ServiceUnitChangeListenerHandle* handle,
                                                  sdv_status_t status) {
        if (status != ASDV_STATUS_OK) {
            create_cb(unexpected(SdvStatus(status)));
            return;
        }
        create_cb(ServiceUnitChangeListener(context, handle));
    };
    // Completion vtable is always last.
    auto completion_vtable = MakeOnceVTable<AbiFunc, -1>(std::move(completion_abi_cb));

    auto status = AbiFunc(context.ToABIPtr(), other_params..., listener_vtable, completion_vtable);
    if (status != ASDV_STATUS_OK) {
        create_cb(unexpected(SdvStatus(status)));
    }
}

void ServiceUnitChangeListener::CreateByTypeAsync(
        const ctx::Context& context, const UnitType& unit_type, ListenerCallback listener_cb,
        std::function<void(sdv::SdvResult<ServiceUnitChangeListener>)> completion_cb) {
    CreateAsync<ASDV_ServiceDiscoveryManager_registerServiceUnitChangeListenerByType>(
            context, std::move(listener_cb), std::move(completion_cb), ToFFI(unit_type));
}

std::future<ServiceUnitChangeListener::CreateResult> ServiceUnitChangeListener::CreateByTypeFuture(
        const ctx::Context& context, const UnitType& unit_type, ListenerCallback listener_cb) {
    auto [result_future, result_cb] = MakeFutureSatisfyingCallback<CreateResult>();
    CreateByTypeAsync(context, unit_type, std::move(listener_cb), std::move(result_cb));
    return std::move(result_future);
}

ServiceUnitChangeListener::CreateResult ServiceUnitChangeListener::CreateByType(
        const ctx::Context& context, const UnitType& unit_type, ListenerCallback listener_cb) {
    return CreateByTypeFuture(context, unit_type, std::move(listener_cb)).get();
}

void ServiceUnitChangeListener::CreateByNameAsync(const ctx::Context& context,
                                                  const UnitNameDiscoveryArgs& unit_name_args,
                                                  ListenerCallback listener_cb,
                                                  CreateCallback create_cb) {
    CreateAsync<ASDV_ServiceDiscoveryManager_registerServiceUnitChangeListenerByName>(
            context, std::move(listener_cb), std::move(create_cb), ToFFI(unit_name_args));
}

std::future<ServiceUnitChangeListener::CreateResult> ServiceUnitChangeListener::CreateByNameFuture(
        const ctx::Context& context, const UnitNameDiscoveryArgs& unit_name_args,
        ListenerCallback listener_cb) {
    auto [result_future, result_cb] = MakeFutureSatisfyingCallback<CreateResult>();
    CreateByNameAsync(context, unit_name_args, std::move(listener_cb), std::move(result_cb));
    return std::move(result_future);
}

ServiceUnitChangeListener::CreateResult ServiceUnitChangeListener::CreateByName(
        const ctx::Context& context, const UnitNameDiscoveryArgs& unit_name_args,
        ListenerCallback listener_cb) {
    return CreateByNameFuture(context, unit_name_args, std::move(listener_cb)).get();
}

}  // namespace sdv_comms::sd