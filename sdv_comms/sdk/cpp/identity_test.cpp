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

#include "ffi_helpers.hpp"

#include <android-base/logging.h>
#include <gtest/gtest.h>

#include <sdv/identity.hpp>

namespace sdv_comms::id::test {

// Short sample values for basic success cases
const char kSampleFqinStr[] = "vm:com.legoog.sdv.WindowManager/sample";
const char kSampleSdvVmName[] = "vm";
const char kSampleSdvPackageName[] = "com.legoog.sdv";
const char kSampleServiceBundleName[] = "WindowManager";
const char kSampleServiceInstanceName[] = "sample";

// Longest and barely-too-long values for length edge cases.
const char kLongestFqinStr[] = "vm-has-31-symbols-9012345678901:com.legoog.sdv.with_127_length_x"
                               "0123456789012345678901234567890123456789012345678901234567890123"
                               "4567890123456789012345678901234.ServiceWith_47_Symbols0123456789"
                               "012345678901234/instance-has-47-symbols-90123456789012345678901";
const char kTooLongFqinStr[] = "vm-has-31-symbols-9012345678901:com.legoog.sdv.with_127_length_x"
                               "0123456789012345678901234567890123456789012345678901234567890123"
                               "4567890123456789012345678901234.ServiceWith_47_Symbols0123456789"
                               "012345678901234/instance-has-47-symbols-90123456789012345678901x";

const char kLongestSdvVmName[] = "vm-has-31-symbols-9012345678901";
const char kTooLongSdvVmName[] = "vm-has-31-symbols-9012345678901x";

const char kLongestSdvPackageName[] =
        "com.legoog.sdv.with_127_length_x01234567890123456789012345678901"
        "234567890123456789012345678901234567890123456789012345678901234";
const char kTooLongSdvPackageName[] =
        "com.legoog.sdv.with_127_length_x01234567890123456789012345678901"
        "234567890123456789012345678901234567890123456789012345678901234x";

const char kLongestServiceBundleName[] = "ServiceWith_47_Symbols0123456789012345678901234";
const char kTooLongServiceBundleName[] = "ServiceWith_47_Symbols0123456789012345678901234x";

const char kLongestServiceInstanceName[] = "instance-has-47-symbols-90123456789012345678901";
const char kTooLongServiceInstanceName[] = "instance-has-47-symbols-90123456789012345678901x";

// ServiceFqin

TEST(ServiceFqin, AppropriatelySized) {
    ASSERT_EQ(sizeof(ServiceFqin), sizeof(ASDV_ServiceFqin));
}

// ServiceFqin::FromString

TEST(ServiceFqin, FromStringSuccess) {
    auto fqin_result = ServiceFqin::FromString(kSampleFqinStr);

    ASSERT_TRUE(fqin_result.has_value());
    EXPECT_EQ(fqin_result->sdv_vm_name(), kSampleSdvVmName);
    EXPECT_EQ(fqin_result->sdv_package_name(), kSampleSdvPackageName);
    EXPECT_EQ(fqin_result->service_bundle_name(), kSampleServiceBundleName);
    EXPECT_EQ(fqin_result->service_instance_name(), kSampleServiceInstanceName);

    // Validate wrapped FFI object since that's going to be used by other SDK code.
    const ASDV_ServiceFqin* fqin_ffi = ToFFI(fqin_result.value());
    EXPECT_STREQ((const char*)fqin_ffi->sdv_vm_name, kSampleSdvVmName);
    EXPECT_STREQ((const char*)fqin_ffi->sdv_package_name, kSampleSdvPackageName);
    EXPECT_STREQ((const char*)fqin_ffi->service_bundle_name, kSampleServiceBundleName);
    EXPECT_STREQ((const char*)fqin_ffi->service_instance_name, kSampleServiceInstanceName);
}

TEST(ServiceFqin, FromStringLongestFqin) {
    ASSERT_EQ(strlen(kLongestFqinStr), (size_t)ASDV_SERVICE_FQIN_MAX_SIZE_BYTES_INCLUDING_NUL - 1);

    auto fqin_result = ServiceFqin::FromString(kLongestFqinStr);
    ASSERT_TRUE(fqin_result.has_value());
    EXPECT_EQ(fqin_result->sdv_vm_name(), kLongestSdvVmName);
    EXPECT_EQ(fqin_result->sdv_package_name(), kLongestSdvPackageName);
    EXPECT_EQ(fqin_result->service_bundle_name(), kLongestServiceBundleName);
    EXPECT_EQ(fqin_result->service_instance_name(), kLongestServiceInstanceName);

    // Add one more character to the longest legal string and expect error.
    auto fqin_result_2 = ServiceFqin::FromString(kTooLongFqinStr);
    ASSERT_FALSE(fqin_result_2.has_value());
    EXPECT_EQ(fqin_result_2.error().getStatusCode(), sdv::SdvStatusCode::kInvalidArgument);
}

TEST(ServiceFqin, FromStringEmptyString) {
    auto fqin_result = ServiceFqin::FromString("");
    ASSERT_FALSE(fqin_result.has_value());
    EXPECT_EQ(fqin_result.error().getStatusCode(), sdv::SdvStatusCode::kInvalidArgument);
}

TEST(ServiceFqin, FromStringInvalidFqin) {
    auto fqin_result = ServiceFqin::FromString("vm:com.legoog.sdv.777WindowManager/sample");
    ASSERT_FALSE(fqin_result.has_value());
    EXPECT_EQ(fqin_result.error().getStatusCode(), sdv::SdvStatusCode::kInvalidArgument);
}

TEST(ServiceFqin, FromStringNonAscii) {
    auto fqin_result = ServiceFqin::FromString("vm:cöm.göögle.sdv.WindöwMänäger/sämple");
    ASSERT_FALSE(fqin_result.has_value());
    EXPECT_EQ(fqin_result.error().getStatusCode(), sdv::SdvStatusCode::kInvalidArgument);
}

TEST(ServiceFqin, FromStringNulInTheMiddle) {
    auto fqin_result = ServiceFqin::FromString("vm:com.legoog.sdv.Window\0Manager/sample");
    ASSERT_FALSE(fqin_result.has_value());
    EXPECT_EQ(fqin_result.error().getStatusCode(), sdv::SdvStatusCode::kInvalidArgument);
}

// ServiceFqin::Create

TEST(ServiceFqin, CreateSuccess) {
    auto fqin_result = ServiceFqin::Create(kSampleSdvVmName, kSampleSdvPackageName,
                                           kSampleServiceBundleName, kSampleServiceInstanceName);

    ASSERT_TRUE(fqin_result.has_value());
    EXPECT_EQ(fqin_result->sdv_vm_name(), kSampleSdvVmName);
    EXPECT_EQ(fqin_result->sdv_package_name(), kSampleSdvPackageName);
    EXPECT_EQ(fqin_result->service_bundle_name(), kSampleServiceBundleName);
    EXPECT_EQ(fqin_result->service_instance_name(), kSampleServiceInstanceName);

    // Validate wrapped FFI object since that's going to be used by other SDK code.
    const ASDV_ServiceFqin* fqin_ffi = ToFFI(fqin_result.value());
    EXPECT_STREQ((const char*)fqin_ffi->sdv_vm_name, kSampleSdvVmName);
    EXPECT_STREQ((const char*)fqin_ffi->sdv_package_name, kSampleSdvPackageName);
    EXPECT_STREQ((const char*)fqin_ffi->service_bundle_name, kSampleServiceBundleName);
    EXPECT_STREQ((const char*)fqin_ffi->service_instance_name, kSampleServiceInstanceName);
}

TEST(ServiceFqin, CreateLongestFqin) {
    auto fqin_result = ServiceFqin::Create(kLongestSdvVmName, kLongestSdvPackageName,
                                           kLongestServiceBundleName, kLongestServiceInstanceName);

    ASSERT_TRUE(fqin_result.has_value());
    EXPECT_EQ(fqin_result->sdv_vm_name(), kLongestSdvVmName);
    EXPECT_EQ(fqin_result->sdv_package_name(), kLongestSdvPackageName);
    EXPECT_EQ(fqin_result->service_bundle_name(), kLongestServiceBundleName);
    EXPECT_EQ(fqin_result->service_instance_name(), kLongestServiceInstanceName);
}

TEST(ServiceFqin, CreateInvalidSdvVmName) {
    for (std::string vm_name : {"", "vm-non-ascїї", kTooLongSdvVmName}) {
        auto fqin_result =
                ServiceFqin::Create(vm_name, kSampleSdvPackageName, kSampleServiceBundleName,
                                    kSampleServiceInstanceName);
        ASSERT_FALSE(fqin_result.has_value());
        EXPECT_EQ(fqin_result.error().getStatusCode(), sdv::SdvStatusCode::kInvalidArgument);
    }
}

TEST(ServiceFqin, CreateInvalidSdvPackageName) {
    for (std::string package_name : {"", "non.ascїї.sdv", kTooLongSdvPackageName}) {
        auto fqin_result =
                ServiceFqin::Create(kSampleSdvVmName, package_name, kSampleServiceBundleName,
                                    kSampleServiceInstanceName);
        ASSERT_FALSE(fqin_result.has_value());
        EXPECT_EQ(fqin_result.error().getStatusCode(), sdv::SdvStatusCode::kInvalidArgument);
    }
}

TEST(ServiceFqin, CreateInvalidServiceBundleName) {
    for (std::string bundle_name : {"", "NonAscїїServїceName", kTooLongServiceBundleName}) {
        auto fqin_result = ServiceFqin::Create(kSampleSdvVmName, kSampleSdvPackageName, bundle_name,
                                               kSampleServiceInstanceName);
        ASSERT_FALSE(fqin_result.has_value());
        EXPECT_EQ(fqin_result.error().getStatusCode(), sdv::SdvStatusCode::kInvalidArgument);
    }
}

TEST(ServiceFqin, CreateInvalidServiceInstanceName) {
    for (std::string instance_name : {"", "non-ascїї-instance-name", kTooLongServiceInstanceName}) {
        auto fqin_result = ServiceFqin::Create(kSampleSdvVmName, kSampleSdvPackageName,
                                               kSampleServiceBundleName, instance_name);
        ASSERT_FALSE(fqin_result.has_value());
        EXPECT_EQ(fqin_result.error().getStatusCode(), sdv::SdvStatusCode::kInvalidArgument);
    }
}

// ServiceFqin::ToString

TEST(ServiceFqin, ToStringSuccess) {
    auto fqin_result = ServiceFqin::FromString(kSampleFqinStr);
    ASSERT_TRUE(fqin_result.has_value());
    auto fqin = fqin_result.value();

    auto to_string_result = fqin_result.value().ToString();
    EXPECT_EQ(to_string_result, kSampleFqinStr);
}

TEST(ServiceFqin, ToStringLongestFqin) {
    auto fqin_result = ServiceFqin::FromString(kLongestFqinStr);
    ASSERT_TRUE(fqin_result.has_value());
    auto fqin = fqin_result.value();

    auto to_string_result = fqin_result.value().ToString();
    EXPECT_EQ(to_string_result, kLongestFqinStr);
}

// ServiceFqin::operator==

TEST(ServiceFqin, EqualsTrue) {
    auto fqin_result_1 = ServiceFqin::FromString(kSampleFqinStr);
    auto fqin_result_2 = ServiceFqin::FromString(kSampleFqinStr);

    ASSERT_TRUE(fqin_result_1.has_value());
    ASSERT_TRUE(fqin_result_2.has_value());

    ASSERT_TRUE(fqin_result_1.value() == fqin_result_2.value());
}

TEST(ServiceFqin, EqualsFalse) {
    auto fqin_result_1 = ServiceFqin::FromString(kSampleFqinStr);
    auto fqin_result_2 = ServiceFqin::FromString(std::string(kSampleFqinStr) + "x");

    ASSERT_TRUE(fqin_result_1.has_value());
    ASSERT_TRUE(fqin_result_2.has_value());

    ASSERT_FALSE(fqin_result_1.value() == fqin_result_2.value());
}

// ServicePublicKey

TEST(ServicePublicKey, AppropriatelySized) {
    ASSERT_EQ(sizeof(ServicePublicKey), sizeof(ASDV_ServicePublicKey));
}

TEST(ServicePublicKey, AreEqual) {
    ServicePublicKey left, right;

    ASSERT_TRUE(left == right);
}

TEST(ServicePublicKey, AreNotEqual) {
    // Default constructor inits data to 0s.
    ServicePublicKey left, right;

    left.data[0] = 1;

    ASSERT_FALSE(left == right);
}

// ServiceIdentity

TEST(ServiceIdentity, AppropriatelySized) {
    ASSERT_EQ(sizeof(ServiceIdentity), sizeof(ASDV_ServiceIdentity));
}

TEST(ServiceIdentity, Create) {
    ServiceFqin fqin_input = ServiceFqin::FromString(kSampleFqinStr).value();

    SdvResult<ServiceIdentity> si_result =
            ServiceIdentity::Create(100UL, fqin_input, ServicePublicKey());

    ASSERT_TRUE(si_result.has_value());
    ServiceIdentity& si = si_result.value();

    EXPECT_EQ(si.sid(), 100UL);

    EXPECT_EQ(si.fqin(), fqin_input);
    EXPECT_EQ(si.fqin().sdv_vm_name(), kSampleSdvVmName);
    EXPECT_EQ(si.fqin().sdv_package_name(), kSampleSdvPackageName);
    EXPECT_EQ(si.fqin().service_bundle_name(), kSampleServiceBundleName);
    EXPECT_EQ(si.fqin().service_instance_name(), kSampleServiceInstanceName);

    EXPECT_EQ(si.public_key(), ServicePublicKey());
}

TEST(ServiceIdentity, AreEqual) {
    ServiceFqin fqin_input = ServiceFqin::FromString(kSampleFqinStr).value();

    SdvResult<ServiceIdentity> left =
            ServiceIdentity::Create(100UL, fqin_input, ServicePublicKey());
    ASSERT_TRUE(left.has_value());

    SdvResult<ServiceIdentity> right =
            ServiceIdentity::Create(100UL, fqin_input, ServicePublicKey());
    ASSERT_TRUE(right.has_value());

    EXPECT_EQ(left.value(), right.value());
}

TEST(ServiceIdentity, NotEqualSid) {
    ServiceFqin fqin_input = ServiceFqin::FromString(kSampleFqinStr).value();

    SdvResult<ServiceIdentity> left =
            ServiceIdentity::Create(100UL, fqin_input, ServicePublicKey());
    ASSERT_TRUE(left.has_value());

    SdvResult<ServiceIdentity> right =
            ServiceIdentity::Create(101UL, fqin_input, ServicePublicKey());
    ASSERT_TRUE(right.has_value());

    EXPECT_NE(left.value(), right.value());
}

TEST(ServiceIdentity, NotEqualFqin) {
    ServiceFqin fqin_left = ServiceFqin::FromString(kSampleFqinStr).value();
    SdvResult<ServiceIdentity> left = ServiceIdentity::Create(100UL, fqin_left, ServicePublicKey());
    ASSERT_TRUE(left.has_value());

    ServiceFqin fqin_right = ServiceFqin::FromString(std::string(kSampleFqinStr) + "x").value();
    SdvResult<ServiceIdentity> right =
            ServiceIdentity::Create(100UL, fqin_right, ServicePublicKey());
    ASSERT_TRUE(right.has_value());

    EXPECT_NE(left.value(), right.value());
}

TEST(ServiceIdentity, NotEqualPublicKey) {
    ServiceFqin fqin_input = ServiceFqin::FromString(kSampleFqinStr).value();

    SdvResult<ServiceIdentity> left =
            ServiceIdentity::Create(100UL, fqin_input, ServicePublicKey());
    ASSERT_TRUE(left.has_value());

    ServicePublicKey right_public_key;
    right_public_key.data[0] = 1;
    SdvResult<ServiceIdentity> right = ServiceIdentity::Create(100UL, fqin_input, right_public_key);
    ASSERT_TRUE(right.has_value());

    EXPECT_NE(left.value(), right.value());
}

// UnitIdentifier tests

TEST(UnitIdentifier, Create) {
    ServiceFqin fqin_input = ServiceFqin::FromString(kSampleFqinStr).value();
    ServiceIdentity si_input =
            ServiceIdentity::Create(100UL, fqin_input, ServicePublicKey()).value();

    SdvResult<UnitIdentifier> ui_result = UnitIdentifier::Create(si_input, "fake_unit_name");

    ASSERT_TRUE(ui_result.has_value());
    const UnitIdentifier& ui = ui_result.value();

    EXPECT_EQ(ui.service_identity(), si_input);
    EXPECT_EQ(ui.unit_name(), "fake_unit_name");
}

TEST(UnitIdentifier, CreateLongestUnitName) {
    ServiceFqin fqin_input = ServiceFqin::FromString(kSampleFqinStr).value();
    ServiceIdentity si_input =
            ServiceIdentity::Create(100UL, fqin_input, ServicePublicKey()).value();

    // Longest possible unit name succeeds.
    SdvResult<UnitIdentifier> ui_result =
            UnitIdentifier::Create(si_input,
                                   "long_unit_name_"
                                   "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDE"
                                   "F0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF");

    ASSERT_TRUE(ui_result.has_value());
    const UnitIdentifier& ui = ui_result.value();
    EXPECT_EQ(ui.service_identity(), si_input);
    EXPECT_EQ(ui.unit_name(),
              "long_unit_name_"
              "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123"
              "456789ABCDEF0123456789ABCDEF");

    // 1-character-too-long unit name fails
    SdvResult<UnitIdentifier> ui_result_2 =
            UnitIdentifier::Create(si_input,
                                   "long_unit_name_"
                                   "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDE"
                                   "F0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0x");
    ASSERT_FALSE(ui_result_2.has_value());
    EXPECT_EQ(ui_result_2.error().getStatusCode(), sdv::SdvStatusCode::kInvalidArgument);
}

TEST(UnitIdentifier, AreEqual) {
    ServiceFqin fqin_input = ServiceFqin::FromString(kSampleFqinStr).value();
    ServiceIdentity si_input =
            ServiceIdentity::Create(100UL, fqin_input, ServicePublicKey()).value();

    SdvResult<UnitIdentifier> left = UnitIdentifier::Create(si_input, "fake_unit_name");
    SdvResult<UnitIdentifier> right = UnitIdentifier::Create(si_input, "fake_unit_name");

    ASSERT_EQ(left.value(), right.value());
}

TEST(UnitIdentifier, NotEqualServiceIdentity) {
    ServiceFqin fqin_input = ServiceFqin::FromString(kSampleFqinStr).value();

    SdvResult<UnitIdentifier> left =
            UnitIdentifier::Create(ServiceIdentity::Create(100UL, fqin_input, ServicePublicKey())
                                           .value(),
                                   "fake_unit_name");
    SdvResult<UnitIdentifier> right =
            UnitIdentifier::Create(ServiceIdentity::Create(101UL, fqin_input, ServicePublicKey())
                                           .value(),
                                   "fake_unit_name");

    ASSERT_NE(left.value(), right.value());
}

TEST(UnitIdentifier, NotEqualUnitName) {
    ServiceFqin fqin_input = ServiceFqin::FromString(kSampleFqinStr).value();
    ServiceIdentity si_input =
            ServiceIdentity::Create(100UL, fqin_input, ServicePublicKey()).value();

    SdvResult<UnitIdentifier> left = UnitIdentifier::Create(si_input, "fake_unit_name");
    SdvResult<UnitIdentifier> right =
            UnitIdentifier::Create(si_input, "different_fake_unit_name_2");

    ASSERT_NE(left.value(), right.value());
}

}  // namespace sdv_comms::id::test