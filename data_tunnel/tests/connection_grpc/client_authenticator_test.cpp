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

#include "connection_grpc/client_authenticator.h"

#include "connection.h"

#include <gmock/gmock.h>
#include <grpc/grpc_security_constants.h>
#include <grpcpp/security/auth_context.h>
#include <gtest/gtest.h>

namespace android::sdv::dt::agent {

namespace {

using ::testing::_;
using ::testing::Return;
using ::testing::StrEq;

// Mock for the TrustedRootsAccessor to control certificate validation behavior in tests.
class MockTrustedRootsAccessor : public TrustedRootsAccessor {
public:
    MOCK_METHOD(bool, IsSignedByTrustedRoot, (std::string_view cert_pem), (const, override));
    MOCK_METHOD(std::string, GetAllTrustedRootsAsPEM, (), (const, override));
};

// Mock for `grpc::AuthContext` for use in tests.
class MockAuthContext : public grpc::AuthContext {
public:
    MOCK_METHOD(std::vector<grpc::string_ref>, FindPropertyValues, (const std::string& name),
                (const, override));
    MOCK_METHOD(bool, IsPeerAuthenticated, (), (const, override));
    MOCK_METHOD(std::vector<grpc::string_ref>, GetPeerIdentity, (), (const, override));
    MOCK_METHOD(std::string, GetPeerIdentityPropertyName, (), (const, override));
    MOCK_METHOD(void, AddProperty, (const std::string& key, const grpc::string_ref& value),
                (override));
    MOCK_METHOD(bool, SetPeerIdentityPropertyName, (const std::string& name), (override));
    MOCK_METHOD(grpc::AuthPropertyIterator, begin, (), (const, override));
    MOCK_METHOD(grpc::AuthPropertyIterator, end, (), (const, override));
};

class ClientAuthenticatorTest : public ::testing::Test {
protected:
    void SetUp() override {
        authenticator_ = std::make_unique<ClientAuthenticator>(mock_roots_accessor_);
    }

    MockTrustedRootsAccessor mock_roots_accessor_;
    std::unique_ptr<ClientAuthenticator> authenticator_;
    MockAuthContext mock_auth_context_;
    grpc::AuthMetadataProcessor::InputMetadata input_metadata_;
    grpc::AuthMetadataProcessor::OutputMetadata output_metadata_1_;
    grpc::AuthMetadataProcessor::OutputMetadata output_metadata_2_;
};

TEST_F(ClientAuthenticatorTest, ProcessWithTrustedCertificate) {
    const std::string trusted_cert = "trusted_cert_pem";

    EXPECT_CALL(mock_auth_context_, FindPropertyValues(StrEq(GRPC_X509_PEM_CERT_PROPERTY_NAME)))
            .WillOnce(Return(std::vector<grpc::string_ref>{trusted_cert}));

    EXPECT_CALL(mock_roots_accessor_, IsSignedByTrustedRoot(StrEq(trusted_cert)))
            .WillOnce(Return(true));

    grpc::Status status = authenticator_->Process(input_metadata_, &mock_auth_context_,
                                                  &output_metadata_1_, &output_metadata_2_);

    EXPECT_EQ(status.error_code(), grpc::StatusCode::OK);
    EXPECT_TRUE(status.error_message().empty());
}

TEST_F(ClientAuthenticatorTest, ProcessWithUntrustedCertificate) {
    const std::string untrusted_cert = "untrusted_cert_pem";

    EXPECT_CALL(mock_auth_context_, FindPropertyValues(StrEq(GRPC_X509_PEM_CERT_PROPERTY_NAME)))
            .WillOnce(Return(std::vector<grpc::string_ref>{untrusted_cert}));

    EXPECT_CALL(mock_roots_accessor_, IsSignedByTrustedRoot(StrEq(untrusted_cert)))
            .WillOnce(Return(false));

    grpc::Status status = authenticator_->Process(input_metadata_, &mock_auth_context_,
                                                  &output_metadata_1_, &output_metadata_2_);

    EXPECT_EQ(status.error_code(), grpc::StatusCode::UNAUTHENTICATED);
    EXPECT_EQ(status.error_message(),
              "Certificate provided by peer not signed by a known trusted root.");
}

TEST_F(ClientAuthenticatorTest, ProcessWithNoCertificates) {
    EXPECT_CALL(mock_auth_context_, FindPropertyValues(StrEq(GRPC_X509_PEM_CERT_PROPERTY_NAME)))
            .WillOnce(Return(std::vector<grpc::string_ref>{}));

    EXPECT_CALL(mock_roots_accessor_, IsSignedByTrustedRoot(_)).Times(0);

    grpc::Status status = authenticator_->Process(input_metadata_, &mock_auth_context_,
                                                  &output_metadata_1_, &output_metadata_2_);

    EXPECT_EQ(status.error_code(), grpc::StatusCode::UNAUTHENTICATED);
    EXPECT_EQ(status.error_message(),
              "Certificate provided by peer not signed by a known trusted root.");
}

TEST_F(ClientAuthenticatorTest, ProcessWithOneTrustedAmongMultipleCertificates) {
    const std::string untrusted_cert1 = "untrusted_cert_pem_1";
    const std::string trusted_cert = "trusted_cert_pem";

    EXPECT_CALL(mock_auth_context_, FindPropertyValues(StrEq(GRPC_X509_PEM_CERT_PROPERTY_NAME)))
            .WillOnce(Return(std::vector<grpc::string_ref>{untrusted_cert1, trusted_cert}));

    EXPECT_CALL(mock_roots_accessor_, IsSignedByTrustedRoot(StrEq(untrusted_cert1)))
            .WillOnce(Return(false));
    EXPECT_CALL(mock_roots_accessor_, IsSignedByTrustedRoot(StrEq(trusted_cert)))
            .WillOnce(Return(true));

    grpc::Status status = authenticator_->Process(input_metadata_, &mock_auth_context_,
                                                  &output_metadata_1_, &output_metadata_2_);

    EXPECT_EQ(status.error_code(), grpc::StatusCode::OK);
}

TEST_F(ClientAuthenticatorTest, ProcessWithMultipleUntrustedCertificates) {
    const std::string untrusted_cert1 = "untrusted_cert_pem_1";
    const std::string untrusted_cert2 = "untrusted_cert_pem_2";

    EXPECT_CALL(mock_auth_context_, FindPropertyValues(StrEq(GRPC_X509_PEM_CERT_PROPERTY_NAME)))
            .WillOnce(Return(std::vector<grpc::string_ref>{untrusted_cert1, untrusted_cert2}));

    EXPECT_CALL(mock_roots_accessor_, IsSignedByTrustedRoot(StrEq(untrusted_cert1)))
            .WillOnce(Return(false));
    EXPECT_CALL(mock_roots_accessor_, IsSignedByTrustedRoot(StrEq(untrusted_cert2)))
            .WillOnce(Return(false));

    grpc::Status status = authenticator_->Process(input_metadata_, &mock_auth_context_,
                                                  &output_metadata_1_, &output_metadata_2_);

    EXPECT_EQ(status.error_code(), grpc::StatusCode::UNAUTHENTICATED);
}

}  // namespace

}  // namespace android::sdv::dt::agent
