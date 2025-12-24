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

#define LOG_TAG "sdv_dt_agent:auth_listener"

#include "authorities_listener.h"

#include "certs.h"

#include <aidl/google/sdv/ca/BnAuthoritiesListener.h>
#include <android-base/logging.h>
#include <log/log.h>

namespace android::sdv::dt::agent {

using ICertificateAuthority = ::aidl::google::sdv::ca::ICertificateAuthority;

// This is the actual listener binder object that is registered with ICertificateAuthority.
//
// It's defined as a nested struct and held as a member by AuthoritiesListener so that
/// AuthoritiesListener destructor can call removeAuthoritiesListener on it and users of
// the class don't need deal with it.
struct AuthoritiesListener::AuthoritiesListenerImpl :
      public aidl::google::sdv::ca::BnAuthoritiesListener {
    ndk::ScopedAStatus onAuthoritiesChanged(const std::string& trusted_roots_pem) override {
        std::unique_lock guard(mutex_);
        auto trusted_root_result = TrustedRootSet::BuildFromPEM(trusted_roots_pem);
        if (!trusted_root_result.has_value()) {
            ALOGE("Error processing authorities from ICertificateAuthority!  Ignoring.  (%s)",
                  trusted_root_result.error().c_str());
            return ndk::ScopedAStatus::ok();
        }

        trusted_roots_pem_ = trusted_roots_pem;
        trusted_root_set_.emplace(std::move(trusted_root_result.value()));
        guard.unlock();
        cv_.notify_all();
        return ndk::ScopedAStatus::ok();
    }

    mutable std::mutex mutex_;
    mutable std::condition_variable cv_;
    std::optional<std::string> trusted_roots_pem_;
    std::optional<TrustedRootSet> trusted_root_set_;
};

::sdv::SdvResult<AuthoritiesListener> AuthoritiesListener::Create(
        std::shared_ptr<ICertificateAuthority> cert_agent, int initial_update_wait_seconds) {
    CHECK(cert_agent) << "Null ICertificateAuthority passed to AuthoritiesListener::Create!";
    CHECK(initial_update_wait_seconds > 0) << "Illegal timeout " << initial_update_wait_seconds
                                           << " passed to AuthoritiesListener::Create!";
    auto impl = ndk::SharedRefBase::make<AuthoritiesListenerImpl>();
    auto call_status = cert_agent->addAuthoritiesListener(impl);
    if (!call_status.isOk()) {
        return base::unexpected(
                ::sdv::SdvStatus(::sdv::SdvStatusCode::kUnavailable,
                                 std::format("Error from addAuthoritiesListener: %s",
                                             call_status.getDescription().c_str())));
    }

    // Go ahead and create the result so that its destructor will clean up the
    // the listener with ICertificateAuthority if anything after this fails.
    AuthoritiesListener result(std::move(cert_agent), impl);

    // Wait for first callback.
    // TODO(b/392124274): Capture this dependency and timeout in partner documentation.
    std::unique_lock guard(impl->mutex_);
    CHECK(impl->cv_.wait_for(guard, std::chrono::seconds(initial_update_wait_seconds), [&impl]() {
        return impl->trusted_roots_pem_.has_value();
    })) << "Did not receive trusted roots from ICertificateAuthority within timeout.";

    return result;
}

AuthoritiesListener::~AuthoritiesListener() {
    if (impl_) {
        cert_agent_->removeAuthoritiesListener(impl_);
    }
}

std::string AuthoritiesListener::GetAllTrustedRootsAsPEM() const {
    std::unique_lock guard(impl_->mutex_);
    CHECK(impl_->trusted_roots_pem_.has_value())
            << "GetAllTrustedRootsAsPEM somehow called before any authorities received!";
    return *impl_->trusted_roots_pem_;
}

bool AuthoritiesListener::IsSignedByTrustedRoot(std::string_view leaf_cert_pem) const {
    std::unique_lock guard(impl_->mutex_);
    CHECK(impl_->trusted_root_set_.has_value())
            << "GetAllTrustedRootsAsPEM somehow called before any authorities received!";
    return impl_->trusted_root_set_->IsSignedByAny(leaf_cert_pem);
}

AuthoritiesListener::AuthoritiesListener(std::shared_ptr<ICertificateAuthority> cert_agent,
                                         std::shared_ptr<AuthoritiesListenerImpl> impl) :
      cert_agent_(cert_agent), impl_(impl) {}

}  // namespace android::sdv::dt::agent
