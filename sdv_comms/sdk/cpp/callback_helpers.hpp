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

#include <android-base/logging.h>

#include <condition_variable>
#include <functional>
#include <future>
#include <memory>
#include <mutex>
#include <optional>
#include <tuple>
#include <vector>

#include <sdv/status.hpp>

// Helper classes for handling callbacks in the Comms Stack C++ SDK.

namespace sdv_comms {

// Templated func/clear functions for an ABI VTable defined by DEFINE_ONCE_CALLBACK.
//
// ValueType is the type of the value passed as the second parameter of to func.
//
// These expect cb_data to be a pointer to std::function that takes the ValueType and
// status code. (i.e. second and third parameters to VTable::func).
template <typename ValueType>
struct OnceTrampoline {
    using CallbackType = std::function<void(ValueType, sdv_status_t)>;
    static void Func(void* cb_data_as_void, ValueType value, sdv_status_t status) {
        if (!cb_data_as_void) {
            LOG(ERROR) << "Got OnceTrampoline::Func() called with null cb_data!";
            return;
        }
        CallbackType* callback = reinterpret_cast<CallbackType*>(cb_data_as_void);
        (*callback)(value, status);
        delete callback;
    }

    static void Clear(void* cb_data_as_void) {
        if (!cb_data_as_void) {
            LOG(ERROR) << "Got OnceTrampoline::Clear() called with null cb_data!";
            return;
        }
        CallbackType* callback = reinterpret_cast<CallbackType*>(cb_data_as_void);
        delete callback;
    }
};

// Templated func/clear functions for an ABI VTable defined by DEFINE_CALLBACK.
//
// ValueType is the type of the second parameter passed to func.
//
// These expect cb_data to be a pointer to std::function that takes an std::optional of
// the ValueType and status code and returns sdv_status_t.
//
// The purpose of std::optional here is to provide a means of indicating end-of-stream;
// for a normal Func invocation, the provided value is passed on to the callback.
// When Clear is invoked, the callback is fired one more time before it's deleted,
// with std::nullopt as the parameter.
template <typename ValueType>
struct RepeatedTrampoline {
    using CallbackType = std::function<sdv_status_t(std::optional<ValueType>)>;

    static sdv_status_t Func(void* cb_data_as_void, ValueType value) {
        if (!cb_data_as_void) {
            LOG(ERROR) << "Got RepeatedTrampoline::Func() called with null cb_data!";
            return ASDV_STATUS_INVALID_ARGUMENT;
        }
        CallbackType* callback = reinterpret_cast<CallbackType*>(cb_data_as_void);
        return (*callback)(value);
    }

    static void Clear(void* cb_data_as_void) {
        if (!cb_data_as_void) {
            LOG(ERROR) << "Got RepeatedTrampoline::Clear() called with null cb_data!";
            return;
        }
        CallbackType* callback = reinterpret_cast<CallbackType*>(cb_data_as_void);
        // Fire callback with nullopt to signal termination.
        (*callback)(std::nullopt);
        delete callback;
    }
};

// nth_param_type extracts the type of the nth param of a function pointer.
// Negative numbers count back from the last parameter.
// Used for extracting VTable struct type and callback ValueType from
// an ABI function that takes a VTable parameter.

// Base template declaration needed so we can specialize it below.
template <typename F, int N>
struct nth_param_type;

// Template pattern matching: First parameter is a pointer to a function, and
// the second is the index of the param whose type we want.  This extracts
// the return type and args parameter pack from the function pointer so we can
// then get the type of one of the args.
template <int N, typename R, typename... A>
struct nth_param_type<R (*)(A...), N> {
    using type = std::tuple_element<((sizeof...(A) + N) % sizeof...(A)), std::tuple<A...>>::type;
};

// Get type of VTable struct from specified parameter of ABI function.
template <auto AbiFunc, int ArgIndex>
using VTableTypeForAbiFuncArg = nth_param_type<decltype(AbiFunc), ArgIndex>::type;

// Get type of value ABI will pass to func; always the second parameter to VTableType::func.
template <typename VTableType>
using ValueTypeForVTable = nth_param_type<decltype(VTableType::func), 1>::type;

// Make a callback suitable for passing to MakeOnceVTable, by using the provided
// AbiValueConverter (must be a static function or captureless lambda) to convert
// an ABI value type to an SDK type consumed by inner_cb.
template <auto AbiValueConverter, typename InnerCallbackType>
auto MakeOnceABICallback(InnerCallbackType inner_cb) {
    using AbiValueType = nth_param_type<decltype(+AbiValueConverter), 0>::type;
    using OuterCallbackType = OnceTrampoline<AbiValueType>::CallbackType;
    return OuterCallbackType(
            [cb = std::move(inner_cb)](AbiValueType val, sdv_status_t status) mutable {
                if (status != ASDV_STATUS_OK) {
                    cb(android::base::unexpected(sdv::SdvStatus(status)));
                    return;
                }
                cb(AbiValueConverter(val));
            });
}

// Make VTable for a one-time callback for a specific ABI function parameter.
//
// Template parameters:
//   AbiFunc: An ABI function that provides an async response.
//   VTableArgIdx: Index of the VTable argument that will be used to deliver
//                 the async response.
//   CallbackType: (Normally inferred) Type of the provided callback.  In
//                 practice this must be something that can be converted
//                 to std::function<void(AbiValueType, std_status_t)>, but
//                 making the callback as a whole the type parameter allows
//                 callers to pass a lambda that hasn't yet been function-wrapped.
//
// Function Parameters:
//   callback: Callback that takes an ABI value of the type the VTable will provide.
template <auto AbiFunc, int VTableArgIdx, typename CallbackType>
auto MakeOnceVTable(CallbackType callback) {
    using VTableType = VTableTypeForAbiFuncArg<AbiFunc, VTableArgIdx>;
    using AbiValueType = ValueTypeForVTable<VTableType>;
    // Trampoline type that will provide func and clear and defines CallbackType.
    using Trampoline = OnceTrampoline<AbiValueType>;

    // SAFETY: We don't use a smart pointer here but expect the client to pass this to
    // an ABI function that guarantees that Trampoline::Clear is eventually called.
    auto cb_data = new Trampoline::CallbackType(std::move(callback));

    return VTableType{.func = &Trampoline::Func, .clear = &Trampoline::Clear, .cb_data = cb_data};
};

// Make a callback suitable for passing to MakeRepeatedVTable, by using the provided
// AbiValueConverter (must be a static function or captureless lambda) to convert
// an ABI value type to an SDK type consumed by inner_cb.
template <auto AbiValueConverter, typename InnerCallbackType>
auto MakeRepeatedABICallback(InnerCallbackType inner_cb) {
    using AbiValueType = nth_param_type<decltype(+AbiValueConverter), 0>::type;
    using OuterCallbackType = RepeatedTrampoline<AbiValueType>::CallbackType;
    return OuterCallbackType(
            [cb = std::move(inner_cb)](std::optional<AbiValueType> val) -> sdv_status_t {
                auto status = val ? cb(AbiValueConverter(*val)) : cb(std::nullopt);
                return static_cast<sdv_status_t>(status);
            });
}

// Builds and returns a VTable that will invoke a repeated callback.
// Template and function parameters work like MakeOnceVTable; see documentation there.
template <auto AbiFunc, int VTableArgIdx, typename CallbackType>
auto MakeRepeatedVTable(CallbackType callback) {
    using VTableType = VTableTypeForAbiFuncArg<AbiFunc, VTableArgIdx>;
    using AbiValueType = ValueTypeForVTable<VTableType>;
    // Trampoline type that will provide func and clear and defines CallbackType.
    using Trampoline = RepeatedTrampoline<AbiValueType>;

    // SAFETY: We don't use a smart pointer here but expect the client to pass this to
    // an ABI function that guarantees that Trampoline::Clear is eventually called.
    auto cb_data = new Trampoline::CallbackType(std::move(callback));

    return VTableType{.func = &Trampoline::Func, .clear = &Trampoline::Clear, .cb_data = cb_data};
}

// Builds a callback that will resolve a future and returns it and the future to be satisfied.
// ValueType must be movable.
template <typename ValueType>
inline std::pair<std::future<ValueType>, std::function<void(ValueType)>>
MakeFutureSatisfyingCallback() {
    // Wrap move-only promise in shared_ptr so that the lambda below is copyable, as
    // required by std::function.
    // We do not need to synchronize access to the promise, as it's only accessed via
    // the get_future call below and a single call to set_value when the callback runs.
    auto result_promise = std::make_shared<std::promise<ValueType>>();

    // Take future from promise now so we don't need to hold on to the promise itself.
    auto fut = result_promise->get_future();

    auto callback = [rp = std::move(result_promise)](ValueType value) {
        rp->set_value(std::move(value));
    };
    return {std::move(fut), std::move(callback)};
}

// Meant to support testing of SDK APIs that create repeated callbacks, this
// class provides a RecordEvent method that can be used as a repeating callback.
//
// It simply records the values it receives in a vector, and provides
// methods for waiting on events to arrive and to access recorded events.
template <typename EventType>
class RepeatedCallbackRecorder {
public:
    // Record a callback firing; note that the signature here matches the signature
    // of a repeated callback exposed in the SDK for the given EventType.
    sdv::SdvStatusCode RecordEvent(std::optional<EventType> event) {
        // Scope to limit lock lifetime.
        {
            std::lock_guard<std::mutex> lock(mutex_);
            events_.emplace_back(std::move(event));
        }
        cvar_.notify_one();
        return sdv::SdvStatusCode::kOk;
    }

    // Get the current count of events.
    size_t GetEventCount() const {
        std::lock_guard<std::mutex> lock(mutex_);
        return events_.size();
    }

    // Retrieve a const ref to the event at a specific index.
    // Requires idx to be in-bound.
    const std::optional<EventType>& GetEvent(size_t idx) const {
        std::lock_guard<std::mutex> lock(mutex_);
        CHECK(idx < events_.size());
        return events_[idx];
    }

    // Waits until at least N events have been accumulated or a timeout expires.
    // Returns true if the requested number were received, or false if timeout happened.
    bool WaitForNthEvent(size_t min_events,
                         std::chrono::milliseconds timeout = std::chrono::seconds(2)) {
        std::unique_lock<std::mutex> lock(mutex_);
        return cvar_.wait_for(lock, timeout,
                              [this, min_events]() { return events_.size() >= min_events; });
    }

private:
    std::vector<std::optional<EventType>> events_;

    mutable std::mutex mutex_;
    std::condition_variable cvar_;
};

// Creates a RepeatedCallbackRecorder and a callable object that can be passed
// as a repeated callback to an SDK method and forwards calls to the recorder's
// RecordEvent method.
template <typename EventType>
auto MakeEventRecordingCallback() {
    auto recorder = std::make_shared<RepeatedCallbackRecorder<EventType>>();
    auto callback = [recorder](std::optional<EventType> ev) {
        return recorder->RecordEvent(std::move(ev));
    };
    return std::make_pair(recorder, callback);
}

}  // namespace sdv_comms
