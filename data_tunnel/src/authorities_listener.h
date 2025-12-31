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

#include "connection.h"

#include <aidl/google/sdv/ca/ICertificateAuthority.h>

// Implementation of IAuthoritiesListener to be given to ICertificateAuthority,
// provides TrustedRootsAccessor interface needed for TLS GRPC connections.

namespace android::sdv::dt::agent {

class AuthoritiesListener : public TrustedRootsAccessor {
public:
    // This method establishes a connection to ICertificateAuthority and waits for the first
    // update with a set of trusted root certs before returning, so the returned
    // AuthoritiesListener (if any) is already fully initialized and able to handle
    // GetAllTrustedRootsAsPEM and IsSignedByTrustedRoot calls.
    //
    // Will return an error if connection to ICertificateAuthority fails or it does not send an
    // initial set of trusted roots within initial_update_wait_seconds (5 minutes by default).
    static ::sdv::SdvResult<AuthoritiesListener> Create(
            std::shared_ptr<aidl::google::sdv::ca::ICertificateAuthority> cert_agent,
            int initial_update_wait_seconds = 300);

    AuthoritiesListener(const AuthoritiesListener&) = delete;
    AuthoritiesListener(AuthoritiesListener&&) = default;
    virtual ~AuthoritiesListener();

    std::string GetAllTrustedRootsAsPEM() const override;

    bool IsSignedByTrustedRoot(std::string_view leaf_cert_pem) const override;

private:
    struct AuthoritiesListenerImpl;

    AuthoritiesListener(std::shared_ptr<aidl::google::sdv::ca::ICertificateAuthority> cert_agent,
                        std::shared_ptr<AuthoritiesListenerImpl> impl);

    std::shared_ptr<aidl::google::sdv::ca::ICertificateAuthority> cert_agent_;
    std::shared_ptr<AuthoritiesListenerImpl> impl_;
};

}  // namespace android::sdv::dt::agent
