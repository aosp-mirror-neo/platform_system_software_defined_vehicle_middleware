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

#include "identity.hpp"

#include <sdv/status.h>

#include <memory>

struct ASDV_Context;

namespace sdv_comms {

// Forward declarations for friendship.
namespace dt {
class HistoryReader;
class InstantReader;
class Publisher;
class Subscriber;
}  // namespace dt
namespace sd {
class ServiceDiscoveryManager;
class ServiceUnitChangeListener;
}  // namespace sd

namespace ctx {

/*  Context is a reference to an internal object provided by the Context library implementation.
 *
 *  THREAD SAFE: All public methods of Context are thread-safe and can be
 *  called from different threads without risk of data races.
 *
 *  IMMUTABLE: All functions defined in this interface MUST NOT mutate internal state.
 *
 *  NON BLOCKING: All functions defined on this object MUST NOT block and return immediately.
 */
class Context {
public:
    // Copyable.
    Context(const Context&) = default;

    // Construct a Context around an opaque ASDV_Context handle.
    // Will raise a fatal error if called with a null handle.
    // DEPRECATED: Use Create static method instead.
    explicit Context(const ASDV_Context* _Nonnull handle);

    // Create a new context around a handle.
    static SdvResult<Context> Create(const ASDV_Context* _Nonnull handle);

    // Create a new context for testing / sample purposes only.
    static Context CreateForTest(const id::ServiceFqin& fqin);

    // Test whether the Context contains a reference to a valid internal object.
    bool IsValid() const;

    /**
     * Returns the Identity of the Service Bundle for which this Context was created.
     *
     * Parameters:
     *
     * Return value:
     *    ServiceIdentity of the Service Bundle.
     */
    SdvResult<id::ServiceIdentity> GetSelfIdentity() const;

    /**
     * Returns the FQIN of the Service Bundle for which this Context was created.
     *
     * Parameters:
     *
     * Return value:
     *    FQIN of the Service Bundle.
     */
    SdvResult<id::ServiceFqin> GetSelfFqin() const;

    /**
     * Returns public key to be used for cryptographic identification of the Service
     * Bundle for which this Context was created.
     *
     * Parameters:
     *
     * Return value:
     *    Constant reference to a Public key of the Service Bundle.
     */
    SdvResult<id::ServicePublicKey> GetPublicKey() const;

private:
    friend dt::HistoryReader;
    friend dt::InstantReader;
    friend dt::Publisher;
    friend dt::Subscriber;
    friend sd::ServiceDiscoveryManager;
    friend sd::ServiceUnitChangeListener;

    // No default construction
    Context() = delete;

    const ASDV_Context* _Nonnull ToABIPtr() const;

    // Pointer to internal implementation.
    const ASDV_Context* _Nonnull handle_;
};

}  // namespace ctx
}  // namespace sdv_comms
