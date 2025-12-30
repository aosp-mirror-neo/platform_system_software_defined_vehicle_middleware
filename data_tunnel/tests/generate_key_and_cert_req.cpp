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

// This is a simple binary wrapping BuildPrivateKeyAndCertRequest that can be used
// to generate keys and certs for testing purposes.
//
// The idea is that the cert request can then be fed to another binary that creates
// a root key and cert and attempts to sign the request using the crypto_rpc library
// and the same sequence of operations that ICertificateAuthority would use.
//
// This serves two purposes:
//   - Successfully signing the request serves as a test that BuildPrivateKeyAndCertRequest
//   - The private key from this script and the root cert and service cert chain produced
//     by that other binary can be stored and used to test GrpcConnectionManager with SSL
//     enabled.

#include "certs.h"

#include <iostream>

int main(int argc, char** argv) {
    if (argc != 2) {
        std::cout << "Expect exactly one argument" << std::endl;
        return -1;
    }
    std::string subject_name((argv[1]));

    auto result = android::sdv::dt::agent::BuildPrivateKeyAndCertRequest(subject_name);

    if (!result.has_value()) {
        std::cout << "Error building key or cert request: " << result.error() << std::endl;
        return -1;
    }

    std::cout << "Subject Name: " << subject_name << "\n\n";
    std::cout << "Private Key:\n" << result.value().pem_private_key << "\n\n";
    std::cout << "Cert Request:\n" << result.value().pem_cert_request << std::endl;
    return 0;
}
