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

#include "authorities_listener.h"

#include <aidl/google/sdv/ca/IAuthoritiesListener.h>
#include <aidl/google/sdv/ca/ICertificateAuthority.h>
#include <android-base/file.h>
#include <android-base/logging.h>
#include <gtest/gtest.h>
#include <log/log.h>

#include <fstream>
#include <iostream>
#include <iterator>
#include <string>

namespace android::sdv::dt::agent::test {

using ICertificateAuthority = aidl::google::sdv::ca::ICertificateAuthority;
using IAuthoritiesListener = aidl::google::sdv::ca::IAuthoritiesListener;

class FakeCertificateAuthority : public ICertificateAuthority {
public:
    explicit FakeCertificateAuthority(
            std::function<ndk::ScopedAStatus(const std::shared_ptr<IAuthoritiesListener>&)>
                    on_add_listener) :
          listener_(nullptr), on_add_listener_(on_add_listener) {
        CHECK(on_add_listener_) << "FakeCertificateAuthority requires a non-empty on_add_listener!";
    }

    bool hasListener() { return listener_ != nullptr; }

    ndk::ScopedAStatus addAuthoritiesListener(
            const std::shared_ptr<IAuthoritiesListener>& listener) override {
        CHECK(listener_ == nullptr) << "Got addAuthoritiesListener more than once!";
        auto result = on_add_listener_(listener);
        if (result.isOk()) {
            listener_ = listener;
        }
        return result;
    }

    ndk::ScopedAStatus removeAuthoritiesListener(
            const std::shared_ptr<IAuthoritiesListener>& listener) override {
        CHECK(listener_ != nullptr) << "Got removeAuthoritiesListener without corresponding add!";
        CHECK(listener_ == listener) << "Got different listener for removal than for add!";
        listener_.reset();
        return ndk::ScopedAStatus::ok();
    }

    ndk::ScopedAStatus requestCertification(const std::string&, std::string*) override {
        LOG(FATAL) << "Unexpected call to requestCertification!";
        return ndk::ScopedAStatus::ok();
    }

    ndk::ScopedAStatus isEnabled(bool*) override {
        LOG(FATAL) << "Unexpected call to requestCertification!";
        return ndk::ScopedAStatus::ok();
    }

    ndk::ScopedAStatus getInterfaceVersion(int32_t* result) override {
        *result = 1;
        return ndk::ScopedAStatus::ok();
    }

    ndk::ScopedAStatus getInterfaceHash(std::string* result) override {
        *result = "deadbeef";
        return ndk::ScopedAStatus::ok();
    }

protected:
    ndk::SpAIBinder asBinder() override { return nullptr; }

    bool isRemote() override { return true; }

    std::shared_ptr<IAuthoritiesListener> listener_;
    std::function<ndk::ScopedAStatus(const std::shared_ptr<IAuthoritiesListener>&)>
            on_add_listener_;
};

std::string ReadSslTestDataFile(const std::string& filename) {
    std::string full_path =
            android::base::GetExecutableDirectory() + "/tests/ssl_test_data/" + filename;
    std::ifstream file(full_path);
    std::string contents;
    copy(std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>(),
         std::back_inserter(contents));
    CHECK(!contents.empty()) << "Could not read file " << full_path;
    return contents;
}

static const char kEndCertMarker[] = "-----END CERTIFICATE-----\n";

std::string GetLeafCertFromChainPEM(const std::string& cert_chain_pem) {
    auto end_pos = cert_chain_pem.find(kEndCertMarker);
    CHECK(end_pos != std::string::npos);
    return cert_chain_pem.substr(0, end_pos + strlen(kEndCertMarker));
}

class DTAgentAuthListenerTest : public testing::Test {
protected:
    DTAgentAuthListenerTest() {}
};

TEST_F(DTAgentAuthListenerTest, CreateAndDestroy) {
    auto fake_ca = ndk::SharedRefBase::make<FakeCertificateAuthority>([](auto listener) {
        listener->onAuthoritiesChanged("");
        return ndk::ScopedAStatus::ok();
    });

    {  // Scope to control lifetime of listener.
        auto create_result = AuthoritiesListener::Create(fake_ca);
        ASSERT_TRUE(create_result.has_value());
        ASSERT_TRUE(fake_ca->hasListener());
    }

    // Check that listener removed from cert authority during destruction.
    ASSERT_FALSE(fake_ca->hasListener());
}

TEST_F(DTAgentAuthListenerTest, CreateFail) {
    auto fake_ca = ndk::SharedRefBase::make<FakeCertificateAuthority>(
            [](auto) { return ndk::ScopedAStatus(AStatus_fromServiceSpecificError(10)); });

    auto create_result = AuthoritiesListener::Create(fake_ca);
    ASSERT_FALSE(create_result.has_value());
}

TEST_F(DTAgentAuthListenerTest, CreateFirstRootSetTimeout) {
    auto fake_ca = ndk::SharedRefBase::make<FakeCertificateAuthority>([](auto) {
        // Do not fire onAuthoritiesChanged so timeout occurs.
        return ndk::ScopedAStatus::ok();
    });

    ASSERT_DEATH(AuthoritiesListener::Create(fake_ca, /* initial_update_wait_seconds */ 1), "");
}

TEST_F(DTAgentAuthListenerTest, UpdateAddsRootCertificate) {
    auto root_cert_0_pem = ReadSslTestDataFile("root_0_cert.pem");
    auto leaf_cert_0_pem = GetLeafCertFromChainPEM(ReadSslTestDataFile("service_0_cert_chain.pem"));
    auto leaf_cert_1_pem = GetLeafCertFromChainPEM(ReadSslTestDataFile("service_1_cert_chain.pem"));

    std::shared_ptr<IAuthoritiesListener> inner_listener;
    auto fake_ca = ndk::SharedRefBase::make<FakeCertificateAuthority>([&](auto l) {
        inner_listener = l;
        l->onAuthoritiesChanged("");
        return ndk::ScopedAStatus::ok();
    });

    auto outer_listener = AuthoritiesListener::Create(fake_ca).value();
    ASSERT_FALSE(outer_listener.IsSignedByTrustedRoot(leaf_cert_0_pem));
    ASSERT_FALSE(outer_listener.IsSignedByTrustedRoot(leaf_cert_1_pem));
    ASSERT_EQ(outer_listener.GetAllTrustedRootsAsPEM(), "");

    inner_listener->onAuthoritiesChanged(root_cert_0_pem);
    ASSERT_TRUE(outer_listener.IsSignedByTrustedRoot(leaf_cert_0_pem));
    ASSERT_FALSE(outer_listener.IsSignedByTrustedRoot(leaf_cert_1_pem));
    ASSERT_EQ(outer_listener.GetAllTrustedRootsAsPEM(), root_cert_0_pem);
}

TEST_F(DTAgentAuthListenerTest, UpdateRemovesRootCertificate) {
    auto all_root_certs_pem = ReadSslTestDataFile("root_certs.pem");
    auto root_cert_1_pem = ReadSslTestDataFile("root_1_cert.pem");
    auto leaf_cert_0_pem = GetLeafCertFromChainPEM(ReadSslTestDataFile("service_0_cert_chain.pem"));
    auto leaf_cert_1_pem = GetLeafCertFromChainPEM(ReadSslTestDataFile("service_1_cert_chain.pem"));

    std::shared_ptr<IAuthoritiesListener> inner_listener;
    auto fake_ca = ndk::SharedRefBase::make<FakeCertificateAuthority>([&](auto l) {
        inner_listener = l;
        l->onAuthoritiesChanged(all_root_certs_pem);
        return ndk::ScopedAStatus::ok();
    });

    auto outer_listener = AuthoritiesListener::Create(fake_ca).value();
    ASSERT_TRUE(outer_listener.IsSignedByTrustedRoot(leaf_cert_0_pem));
    ASSERT_TRUE(outer_listener.IsSignedByTrustedRoot(leaf_cert_1_pem));
    ASSERT_EQ(outer_listener.GetAllTrustedRootsAsPEM(), all_root_certs_pem);

    inner_listener->onAuthoritiesChanged(root_cert_1_pem);
    ASSERT_FALSE(outer_listener.IsSignedByTrustedRoot(leaf_cert_0_pem));
    ASSERT_TRUE(outer_listener.IsSignedByTrustedRoot(leaf_cert_1_pem));
    ASSERT_EQ(outer_listener.GetAllTrustedRootsAsPEM(), root_cert_1_pem);
}

}  // namespace android::sdv::dt::agent::test
