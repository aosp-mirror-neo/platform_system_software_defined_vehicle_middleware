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

#include "certs.h"

#include <android-base/file.h>
#include <android-base/logging.h>
#include <gtest/gtest.h>

#include <chrono>
#include <fstream>
#include <iostream>
#include <iterator>
#include <string>

namespace android::sdv::dt::agent::test {

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

class DTAgentCertUtilsTest : public testing::Test {
protected:
    DTAgentCertUtilsTest() {}
};

TEST_F(DTAgentCertUtilsTest, TrustedRootSetAllRoots) {
    auto all_roots_pem = ReadSslTestDataFile("root_certs.pem");
    auto trusted_set_result = TrustedRootSet::BuildFromPEM(all_roots_pem);
    ASSERT_TRUE(trusted_set_result.has_value());
    auto trusted_set = std::move(trusted_set_result.value());

    auto leaf_cert_0_pem = GetLeafCertFromChainPEM(ReadSslTestDataFile("service_0_cert_chain.pem"));
    ASSERT_TRUE(trusted_set.IsSignedByAny(leaf_cert_0_pem));

    auto leaf_cert_1_pem = GetLeafCertFromChainPEM(ReadSslTestDataFile("service_1_cert_chain.pem"));
    ASSERT_TRUE(trusted_set.IsSignedByAny(leaf_cert_1_pem));
}

TEST_F(DTAgentCertUtilsTest, TrustedRootSetOnlyOneRoot) {
    auto all_roots_pem = ReadSslTestDataFile("root_0_cert.pem");
    auto trusted_set_result = TrustedRootSet::BuildFromPEM(all_roots_pem);
    ASSERT_TRUE(trusted_set_result.has_value());
    auto trusted_set = std::move(trusted_set_result.value());

    auto leaf_cert_0_pem = GetLeafCertFromChainPEM(ReadSslTestDataFile("service_0_cert_chain.pem"));
    ASSERT_TRUE(trusted_set.IsSignedByAny(leaf_cert_0_pem));

    auto leaf_cert_1_pem = GetLeafCertFromChainPEM(ReadSslTestDataFile("service_1_cert_chain.pem"));
    ASSERT_FALSE(trusted_set.IsSignedByAny(leaf_cert_1_pem));
}

TEST_F(DTAgentCertUtilsTest, TrustedRootSetBuildFromPEM_Malformed) {
    auto trusted_set_result = TrustedRootSet::BuildFromPEM("this is not a valid pem");
    ASSERT_FALSE(trusted_set_result.has_value());
}

TEST_F(DTAgentCertUtilsTest, TrustedRootSetCanBeEmpty) {
    auto trusted_set_result = TrustedRootSet::BuildFromPEM("");
    ASSERT_TRUE(trusted_set_result.has_value());
    auto trusted_set = std::move(trusted_set_result.value());

    auto leaf_cert_0_pem = GetLeafCertFromChainPEM(ReadSslTestDataFile("service_0_cert_chain.pem"));
    ASSERT_FALSE(trusted_set.IsSignedByAny(leaf_cert_0_pem));

    auto leaf_cert_1_pem = GetLeafCertFromChainPEM(ReadSslTestDataFile("service_1_cert_chain.pem"));
    ASSERT_FALSE(trusted_set.IsSignedByAny(leaf_cert_1_pem));
}

TEST_F(DTAgentCertUtilsTest, DecodedRootCNFromChainPEM_Success) {
    auto chain_0_pem = ReadSslTestDataFile("service_0_cert_chain.pem");
    auto chain_0_root_cn = DecodedRootCNFromChainPEM(chain_0_pem);
    ASSERT_TRUE(chain_0_root_cn);
    ASSERT_EQ("vm0:CertAuthority", *chain_0_root_cn);
}

TEST_F(DTAgentCertUtilsTest, DecodedRootCNFromChainPEM_EmptyChain) {
    // No certificate present in chain.
    ASSERT_FALSE(DecodedRootCNFromChainPEM(""));
}

TEST_F(DTAgentCertUtilsTest, DecodedRootCNFromChainPEM_MalformedRoot) {
    auto chain_with_malformed_root = ReadSslTestDataFile("cert_chain_with_malformed_root.pem");
    ASSERT_FALSE(DecodedRootCNFromChainPEM(chain_with_malformed_root));
}

TEST_F(DTAgentCertUtilsTest, DecodedRootCNFromChainPEM_RootCertWithoutCommonName) {
    auto root_cert_no_common_name = ReadSslTestDataFile("root_cert_without_common_name.pem");
    ASSERT_FALSE(DecodedRootCNFromChainPEM(root_cert_no_common_name));
}

TEST_F(DTAgentCertUtilsTest, DecodedRootCNFromChainPEM_RootCertWithInvalidCommonName) {
    auto root_cert_invalid_common_name =
            ReadSslTestDataFile("root_cert_with_invalid_common_name.pem");
    ASSERT_FALSE(DecodedRootCNFromChainPEM(root_cert_invalid_common_name));
}
// Tests for EncodeNameForCert
TEST_F(DTAgentCertUtilsTest, EncodeNameForCert_BasicInput) {
    // This function applies SHA256 and then Base32 DNSSEC encoding.
    EXPECT_EQ("h3i99r9ra9ql050pdc6g8v8a92j08f60uu6t36iemdvvvu2t832g",
              EncodeNameForCert("vm0:CertAuthority"));
    EXPECT_EQ("uuft7gepl8i86a0ad9e65metj5hfagsk0412kh2gapkjbgqachp0",
              EncodeNameForCert("another_test_string"));
}

TEST_F(DTAgentCertUtilsTest, EncodeNameForCert_EmptyInput) {
    EXPECT_EQ("seoc8gkovge196nruj49irtp4gjqsgf4cidp6j54imchmu2in1ag", EncodeNameForCert(""));
}

// Tests for BuildPrivateKeyAndCertRequest
TEST_F(DTAgentCertUtilsTest, BuildPrivateKeyAndCertRequest_Success) {
    std::string subject_name = "test_subject_name";
    auto result = BuildPrivateKeyAndCertRequest(subject_name);
    ASSERT_TRUE(result.has_value());

    // Check if private key PEM is valid
    ASSERT_FALSE(result->pem_private_key.empty());
    ASSERT_NE(result->pem_private_key.find("-----BEGIN PRIVATE KEY-----"), std::string::npos)
            << result->pem_private_key;
    ASSERT_NE(result->pem_private_key.find("-----END PRIVATE KEY-----"), std::string::npos)
            << result->pem_private_key;

    // Check if cert request PEM is valid
    ASSERT_FALSE(result->pem_cert_request.empty());
    ASSERT_NE(result->pem_cert_request.find("-----BEGIN CERTIFICATE REQUEST-----"),
              std::string::npos)
            << result->pem_cert_request;
    ASSERT_NE(result->pem_cert_request.find("-----END CERTIFICATE REQUEST-----"), std::string::npos)
            << result->pem_cert_request;
    ;
}

}  // namespace android::sdv::dt::agent::test
