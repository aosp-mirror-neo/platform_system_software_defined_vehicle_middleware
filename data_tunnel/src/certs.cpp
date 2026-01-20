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

#define LOG_TAG "sdv_dt_agent:certs"

#include "certs.h"

#include <android-base/logging.h>
#include <log/log.h>
#include <openssl/base.h>
#include <openssl/bio.h>
#include <openssl/ec_key.h>
#include <openssl/nid.h>
#include <openssl/pem.h>
#include <openssl/stack.h>
#include <openssl/x509.h>

#include <iostream>

namespace android::sdv::dt::agent {

namespace {

using Error = android::base::unexpected<std::string>;

using bssl::UniquePtr;
using Stack_GENERAL_NAME = STACK_OF(GENERAL_NAME);
using Stack_X509_EXTENSION = STACK_OF(X509_EXTENSION);
using Stack_X509_INFO = STACK_OF(X509_INFO);

Result<std::pair<bssl::UniquePtr<EVP_PKEY>, std::string>> CreatePrivateKeyAndPEM() {
    UniquePtr<EC_KEY> ec_key(EC_KEY_new_by_curve_name(NID_X9_62_prime256v1));
    if (!ec_key) {
        return Error("EC_KEY_new_by_curve_name failed");
    }

    if (!EC_KEY_generate_key(ec_key.get())) {
        return Error("EC_KEY_generate_key failed");
    }

    UniquePtr<EVP_PKEY> pkey(EVP_PKEY_new());
    if (!pkey) {
        return Error("EVP_PKEY_new failed");
    }

    if (!EVP_PKEY_assign_EC_KEY(pkey.get(), ec_key.release())) {
        return Error("EC_KEY_copy failed");
    }

    UniquePtr<BIO> bio(BIO_new(BIO_s_mem()));
    if (!PEM_write_bio_PrivateKey(bio.get(), pkey.get(), nullptr, nullptr, 0, nullptr, nullptr)) {
        return Error("PEM_write_bio_PrivateKey failed");
    }

    const char* pem_data;
    size_t pem_len;
    if (!BIO_mem_contents(bio.get(), reinterpret_cast<const uint8_t**>(&pem_data), &pem_len)) {
        return Error("BIO_mem_contents failed");
    }

    return std::make_pair(std::move(pkey), std::string(pem_data, pem_len));
}

Result<std::string> BuildCertificationRequest(EVP_PKEY* pkey, const std::string& subject_name) {
    // let mut cert_req_builder = X509ReqBuilder::new()?;
    UniquePtr<X509_REQ> x509_req(X509_REQ_new());
    if (!x509_req) {
        return Error("X509_REQ_new failed");
    }

    // cert_req_builder.set_pubkey(&pkey)?;
    if (!X509_REQ_set_pubkey(x509_req.get(), pkey)) {
        return Error("X509_set_pubkey failed");
    }

    // let v3_context = cert_req_builder.x509v3_context(None);
    X509V3_CTX x509v3_ctx;
    memset(&x509v3_ctx, 0, sizeof(x509v3_ctx));
    X509V3_set_ctx(&x509v3_ctx, nullptr, nullptr, x509_req.get(), nullptr, 0);

    // let san_value = BASE32_DNSSEC.encode(subject_name.as_bytes());
    auto san_value = EncodeNameForCert(subject_name);
    ALOGD("Building certificate request for %s.  Encoded SAN: %s.", subject_name.c_str(),
          san_value.c_str());

    // let subject_alternative_name = openssl::x509::extension::SubjectAlternativeName::new()
    //     .dns(&san_value)
    //     .build(&v3_context)?;
    bssl::UniquePtr<ASN1_STRING> san_value_asn1(ASN1_STRING_type_new(V_ASN1_IA5STRING));
    ASN1_STRING_set(san_value_asn1.get(), san_value.data(), san_value.length());
    bssl::UniquePtr<GENERAL_NAME> san_gn(GENERAL_NAME_new());
    san_gn->type = GEN_DNS;
    san_gn->d.dNSName = san_value_asn1.release();
    UniquePtr<Stack_GENERAL_NAME> san_stack(sk_GENERAL_NAME_new_null());
    sk_GENERAL_NAME_push(san_stack.get(), san_gn.release());
    UniquePtr<X509_EXTENSION> san_ext(X509V3_EXT_i2d(NID_subject_alt_name, false, san_stack.get()));

    // let mut extension_stack: Stack<X509Extension> = Stack::new()?;
    UniquePtr<Stack_X509_EXTENSION> ext_stack(sk_X509_EXTENSION_new_null());

    // extension_stack.push(subject_alternative_name)?;
    sk_X509_EXTENSION_push(ext_stack.get(), san_ext.release());

    // cert_req_builder.add_extensions(&extension_stack)?;
    X509_REQ_add_extensions(x509_req.get(), ext_stack.get());

    // cert_req_builder.sign(&pkey, MessageDigest::sha256())?;
    X509_REQ_sign(x509_req.get(), pkey, EVP_sha256());

    UniquePtr<BIO> bio(BIO_new(BIO_s_mem()));
    if (!PEM_write_bio_X509_REQ(bio.get(), x509_req.get())) {
        return Error("PEM_write_bio_X509_REQ failed");
    }

    const char* pem_data;
    size_t pem_len;
    if (!BIO_mem_contents(bio.get(), reinterpret_cast<const uint8_t**>(&pem_data), &pem_len)) {
        return Error("BIO_mem_contents failed");
    }

    return std::string(pem_data, pem_len);
}

template <typename ByteContainer>
std::array<uint8_t, 32> Sha256(const ByteContainer& bytes) {
    std::array<uint8_t, SHA256_DIGEST_LENGTH> result;
    SHA256_CTX sha256;
    SHA256_Init(&sha256);
    SHA256_Update(&sha256, bytes.data(), bytes.size());
    SHA256_Final(result.data(), &sha256);
    return result;
}

// Mostly cribbed from
// samples/sdv_gateway/grpc_tls_credentials/grpc_tls_credentials_manager.cpp
template <typename ByteContainer>
std::string Base32DnsSecEncode(const ByteContainer& bytes) {
    // The alphabet is the same as one used in Rust.
    // https://doc.servo.org/data_encoding/constant.BASE32_DNSSEC.html
    const std::string alphabet = "0123456789abcdefghijklmnopqrstuv";

    std::string result;
    int bits_left = 0;
    uint32_t buffer = 0;

    for (size_t i = 0; i < bytes.size(); i++) {
        uint8_t c = static_cast<const uint8_t>(bytes.data()[i]);
        buffer = (buffer << 8) | c;
        bits_left += 8;

        while (bits_left >= 5) {
            int i = (buffer >> (bits_left - 5)) & 31;
            result += alphabet[i];
            bits_left -= 5;
        }
    }

    if (bits_left > 0) {
        int i = (buffer << (5 - bits_left)) & 31;
        result += alphabet[i];
    }

    return result;
}

// Decode a BASE32-DNSSEC-encoded string to the original data.
//
// Returns std::nullopt if the input data is not a valid BASE32-DNSSEC string.
template <typename ByteContainer>
std::optional<std::string> Base32DnsSecDecode(const ByteContainer& bytes) {
    std::string result;
    int bits_left = 0;
    uint32_t buffer = 0;

    for (size_t i = 0; i < bytes.size(); i++) {
        uint8_t c = static_cast<const uint8_t>(bytes.data()[i]);
        char n;
        if (c >= '0' && c <= '9') {
            n = c - '0';
        } else if (c >= 'a' && c <= 'v') {
            n = c - 'a' + 10;
        } else if (c >= 'A' && c <= 'V') {
            n = c - 'A' + 10;
        } else {
            return std::nullopt;
        }
        buffer = (buffer << 5) | n;
        bits_left += 5;

        while (bits_left >= 8) {
            result += (buffer >> (bits_left - 8)) & 255;
            bits_left -= 8;
        }
    }

    return result;
}

}  // namespace

std::string EncodeNameForCert(const std::string& input) {
    return Base32DnsSecEncode(Sha256(input));
}

std::optional<std::string> DecodedRootCNFromChainPEM(std::string_view chain_pem) {
    if (!chain_pem.data()) {
        ALOGE("DecodedRootCNFromChainPEM: got chain PEM with null data");
    }
    bssl::UniquePtr<X509> last_cert;
    bssl::UniquePtr<BIO> bio(BIO_new_mem_buf(chain_pem.data(), chain_pem.length()));
    while (auto next_cert =
                   bssl::UniquePtr<X509>(PEM_read_bio_X509(bio.get(), nullptr, nullptr, nullptr))) {
        last_cert = std::move(next_cert);
    }
    if (!last_cert) {
        ALOGW("DecodedRootCNFromChainPEM: No certificate found in chain PEM");
        return std::nullopt;
    }
    auto* issuer = X509_get_issuer_name(last_cert.get());
    if (!issuer) {
        ALOGW("DecodedRootCNFromChainPEM: Root certificate lacks issuer");
        return std::nullopt;
    }
    auto* subject = X509_get_subject_name(last_cert.get());
    if (!subject) {
        ALOGW("DecodedRootCNFromChainPEM: Root certificate lacks subject");
        return std::nullopt;
    }
    if (X509_NAME_cmp(issuer, subject) != 0) {
        ALOGW("DecodedRootCNFromChainPEM: Root certificate is not self-issued.");
        return std::nullopt;
    }
    int cn_index = X509_NAME_get_index_by_NID(subject, NID_commonName, -1);
    if (cn_index == -1) {
        ALOGW("DecodedRootCNFromChainPEM: Root certificate subject lacks common name");
        return std::nullopt;
    }
    const auto* cn_entry_ptr = X509_NAME_get_entry(subject, cn_index);
    if (!cn_entry_ptr) {
        ALOGE("DecodedRootCNFromChainPEM: Error getting subject common name index");
        return std::nullopt;
    }
    auto* cn_asn1 = X509_NAME_ENTRY_get_data(cn_entry_ptr);
    if (!cn_asn1) {
        ALOGE("DecodedRootCNFromChainPEM: Error getting subject common name entry ptr");
        return std::nullopt;
    }
    const auto* cn_data = reinterpret_cast<const char*>(ASN1_STRING_get0_data(cn_asn1));
    if (!cn_data) {
        ALOGE("DecodedRootCNFromChainPEM: Error getting subject common name data");
        return std::nullopt;
    }
    std::string_view cn_view(cn_data, ASN1_STRING_length(cn_asn1));
    return Base32DnsSecDecode(cn_view);
}

Result<PrivateKeyAndCertRequest> BuildPrivateKeyAndCertRequest(const std::string& subject_name) {
    auto key_and_pem_result = CreatePrivateKeyAndPEM();
    if (!key_and_pem_result.has_value()) {
        return Error(key_and_pem_result.error() + " in CreatePrivateKeyAndPEM");
    }
    auto [pkey, pkey_pem] = std::move(key_and_pem_result.value());

    auto cert_req_result = BuildCertificationRequest(pkey.get(), subject_name);
    if (!cert_req_result.has_value()) {
        return Error(cert_req_result.error() + "in BuiltCertificationRequest");
    }

    return PrivateKeyAndCertRequest{.pem_private_key = std::move(pkey_pem),
                                    .pem_cert_request = std::move(cert_req_result.value())};
}

Result<TrustedRootSet> TrustedRootSet::BuildFromPEM(std::string_view root_cert_pem) {
    UniquePtr<X509_STORE> x509_store(X509_STORE_new());

    // Empty root set case; BIO_write will fail on 0 length.
    if (root_cert_pem.length() == 0) {
        return TrustedRootSet(std::move(x509_store));
    }

    UniquePtr<BIO> root_certs_bio(BIO_new(BIO_s_mem()));
    if (!BIO_write(root_certs_bio.get(), root_cert_pem.data(), root_cert_pem.length())) {
        return Error("Failed to copy root_certs into BIO.");
    }

    UniquePtr<Stack_X509_INFO> root_certs_info(
            PEM_X509_INFO_read_bio(root_certs_bio.get(), nullptr, nullptr, nullptr));

    bool added_cert = false;
    for (size_t i = 0; i < sk_X509_INFO_num(root_certs_info.get()); i++) {
        auto entry = sk_X509_INFO_value(root_certs_info.get(), i);
        if (entry->x509) {
            added_cert = true;
            X509_STORE_add_cert(x509_store.get(), entry->x509);
        } else {
            ALOGE("Entry in root certs PEM that is not Cert.");
            continue;
        }
    }

    if (!added_cert) {
        return Error("No certificates found in root certs PEM.");
    }

    return TrustedRootSet(std::move(x509_store));
}

TrustedRootSet::TrustedRootSet(UniquePtr<X509_STORE>&& store) : store_(std::move(store)) {}

bool TrustedRootSet::IsSignedByAny(const std::string_view leaf_cert_pem) const {
    UniquePtr<BIO> leaf_cert_bio(BIO_new(BIO_s_mem()));
    if (!BIO_write(leaf_cert_bio.get(), leaf_cert_pem.data(), leaf_cert_pem.length())) {
        ALOGE("TrustedRootSet::IsSignedByAny error from BIO_write!");
        return false;
    }

    UniquePtr<X509> leaf_cert_x509(
            PEM_read_bio_X509(leaf_cert_bio.get(), nullptr, nullptr, nullptr));
    if (!leaf_cert_x509) {
        ALOGE("TrustedRootSet::IsSignedByAny got malformed / unparseable cert!");
        return false;
    }

    UniquePtr<X509_STORE_CTX> x509_store_ctx(X509_STORE_CTX_new());
    X509_STORE_CTX_init(x509_store_ctx.get(), store_.get(), leaf_cert_x509.get(), nullptr);

    auto verify_result = X509_verify_cert(x509_store_ctx.get());
    if (verify_result < 0) {
        ALOGE("TrustedRootSet::IsSignedByAny encountered error from X509_verify_cert: %d",
              verify_result);
        return false;
    }
    return verify_result > 0;
}

}  // namespace android::sdv::dt::agent
