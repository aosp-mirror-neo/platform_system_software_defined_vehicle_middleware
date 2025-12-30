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

#include <android-base/expected.h>
#include <openssl/base.h>
#include <openssl/x509.h>

#include <string>

namespace android::sdv::dt::agent {

std::string EncodeNameForCert(const std::string& input);

std::optional<std::string> DecodedRootCNFromChainPEM(std::string_view chain_pem);

template <typename SuccessType>
using Result = android::base::expected<SuccessType, std::string>;

struct PrivateKeyAndCertRequest {
    std::string pem_private_key;
    std::string pem_cert_request;
};

Result<PrivateKeyAndCertRequest> BuildPrivateKeyAndCertRequest(const std::string& subject_name);

class TrustedRootSet {
public:
    static Result<TrustedRootSet> BuildFromPEM(std::string_view root_certs);

    TrustedRootSet(TrustedRootSet&&) = default;
    TrustedRootSet() = default;

    bool IsSignedByAny(const std::string_view leaf_cert_pem) const;

private:
    TrustedRootSet(const TrustedRootSet&) = delete;

    explicit TrustedRootSet(bssl::UniquePtr<X509_STORE>&&);

    bssl::UniquePtr<X509_STORE> store_;
};

}  // namespace android::sdv::dt::agent
