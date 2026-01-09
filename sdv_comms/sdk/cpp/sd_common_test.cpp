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

#include <gtest/gtest.h>
#include <sdv/sd_callbacks.h>
#include <sdv/unit_discovery.h>
#include <sdv/unit_metadata.h>
#include <sdv/unit_registration.h>
#include <sdv/unit_type.h>

#include <sdv/sd_common.hpp>

using sdv::SdvStatusCode;

namespace sdv_comms::sd::test {

// UnitType tests

static const char* kSampleUnitTypeString = "com.legoog.sdv.Temperature";
static const char* kSampleSdvPackageName = "com.legoog.sdv";
static const char* kSampleServiceBundleName = "WindowManager";
static const char* kSampleUnitTypeName = "Temperature";
static const char kSampleUnitName[] = "sample";
static const char kSampleSdvVmName[] = "vm";
static const char* kSampleUnitTypeSubPackageName = "subpkg.Temperature";

static const char* kSampleUnitTypeWithSubpackageString =
        "com.legoog.sdv.WindowManager#subpkg.Temperature";

static const char* kLongestUnitTypeString =
        "com.legoog.sdv.with_127_length_x01234567890123456789012345678901"
        "234567890123456789012345678901234567890123456789012345678901234."
        "UnitTypeWith95symbolsintotal23545678901234567890"
        "12345678901234567890123456789012345678901234567";
static const char* kLongestSdvPackageName =
        "com.legoog.sdv.with_127_length_x01234567890123456789012345678901"
        "234567890123456789012345678901234567890123456789012345678901234";
static const char* kTooLongSdvPackageName =
        "com.legoog.sdv.with_128_length_x01234567890123456789012345678901"
        "2345678901234567890123456789012345678901234567890123456789012345";
static const char* kLongestServiceBundleName = "ServiceWith_47_Symbols0123456789012345678901234";
static const char* kTooLongServiceBundleName = "ServiceWith_48_Symbols01234567890123456789012345";
static const char* kLongestUnitTypeName = "UnitTypeWith95symbolsintotal23545678901234567890"
                                          "12345678901234567890123456789012345678901234567";
static const char* kTooLongUnitTypeName = "UnitTypeWith96symbolsintotal23545678901234567890"
                                          "123456789012345678901234567890123456789012345678";

static const char* kLongestUnitName = "UnitNameWith127symbolsintotal2354567890123456789"
                                      "012345678901234567890123456789012345678901234567"
                                      "8901234567890123456789012345678";
static const char* kTooLongUnitName = "UnitNameWith128symbolsintotal2354567890123456789"
                                      "012345678901234567890123456789012345678901234567"
                                      "89012345678901234567890123456789";

static const char kLongestSdvVmName[] = "vm-has-31-symbols-9012345678901";
static const char kTooLongSdvVmName[] = "vm-has-32-symbols-90123456789012";

TEST(UnitType, AppropriatelySized) {
    ASSERT_EQ(sizeof(UnitType), sizeof(ASDV_UnitType));
}

TEST(UnitType, FromStringSucceedsWithValidString) {
    auto unit_type_result = UnitType::FromString(kSampleUnitTypeString);
    ASSERT_TRUE(unit_type_result.has_value());

    const auto& unit_type = unit_type_result.value();
    EXPECT_EQ(unit_type.sdv_package_name(), kSampleSdvPackageName);
    EXPECT_EQ(unit_type.unit_type_name(), kSampleUnitTypeName);

    // Check wrapped record since that's what gets passed into ABI functions.
    const ASDV_UnitType* unit_type_ffi = ToFFI(unit_type);
    EXPECT_STREQ((const char*)unit_type_ffi->sdv_package_name, kSampleSdvPackageName);
    EXPECT_STREQ((const char*)unit_type_ffi->unit_type_name, kSampleUnitTypeName);
}

TEST(UnitType, FromStringSucceedsOnLongestPossibleString) {
    auto unit_type_result = UnitType::FromString(kLongestUnitTypeString);
    ASSERT_TRUE(unit_type_result.has_value());

    const auto& unit_type = unit_type_result.value();

    EXPECT_EQ(unit_type.sdv_package_name(), kLongestSdvPackageName);
    EXPECT_EQ(unit_type.unit_type_name(), kLongestUnitTypeName);

    // Check wrapped record since that's what gets passed into ABI functions.
    const ASDV_UnitType* unit_type_ffi = ToFFI(unit_type);
    EXPECT_STREQ((const char*)unit_type_ffi->sdv_package_name, kLongestSdvPackageName);
    EXPECT_STREQ((const char*)unit_type_ffi->unit_type_name, kLongestUnitTypeName);
}

TEST(UnitType, FromStringSucceedsWithSubpackage) {
    auto unit_type_result = UnitType::FromString(kSampleUnitTypeWithSubpackageString);
    ASSERT_TRUE(unit_type_result.has_value());

    const auto& unit_type = unit_type_result.value();
    EXPECT_EQ(unit_type.sdv_package_name(), kSampleSdvPackageName);
    EXPECT_EQ(unit_type.unit_type_name(), kSampleUnitTypeSubPackageName);
}

TEST(UnitType, FromStringFailsOnTooLongString) {
    std::string too_long_unit_type_string = std::string(kLongestUnitTypeString) + "x";

    auto unit_type_result = UnitType::FromString(too_long_unit_type_string);
    ASSERT_FALSE(unit_type_result.has_value());
    EXPECT_EQ(unit_type_result.error().getStatusCode(), SdvStatusCode::kInvalidArgument);
}

TEST(UnitType, FromStringFailsOnInvalidContent) {
    for (std::string unit_type_str :
         {// Empty string
          "",
          // Invalid dot count
          "comgoogsdvWindowManager#Temperature", "comgoogsdv.WindowManager#Temperature",
          // Invalid hash count
          "com.legoog.sdv.WindowManager#mai#n",
          // Invalid hash order
          "com.legoog.sdv#WindowManager.Temperature", "com.goog#sdv.WindowManager.Temperature",
          // Non-ascii characters
          "cöm.google.sdv#WindöwManager.Temperature",
          // Nul in the middle
          "com.legoog.sdv.\0Temperature"}) {
        auto unit_type_result = UnitType::FromString(unit_type_str);
        EXPECT_FALSE(unit_type_result.has_value())
                << "UnitType::FromString unexpected success on input \"" << unit_type_str << "\"";
        if (!unit_type_result.has_value()) {
            EXPECT_EQ(unit_type_result.error().getStatusCode(), SdvStatusCode::kInvalidArgument)
                    << "UnitType::FromString unexpected error code on input \"" << unit_type_str
                    << "\"";
        }
    }
}

TEST(UnitType, ToStringSuccess) {
    auto unit_type_result = UnitType::FromString(kSampleUnitTypeString);
    ASSERT_TRUE(unit_type_result.has_value());
    EXPECT_EQ(unit_type_result.value().ToString(), kSampleUnitTypeString);
}

TEST(UnitType, ToStringSuccessOnLongest) {
    auto unit_type_result = UnitType::FromString(kLongestUnitTypeString);
    ASSERT_TRUE(unit_type_result.has_value());
    EXPECT_EQ(unit_type_result.value().ToString(), kLongestUnitTypeString);
}

TEST(UnitType, CreateSucceeds) {
    auto unit_type_result =
            UnitType::Create(kSampleSdvPackageName, kSampleServiceBundleName, kSampleUnitTypeName);
    ASSERT_TRUE(unit_type_result.has_value());
    const auto& unit_type = unit_type_result.value();

    EXPECT_EQ(unit_type.sdv_package_name(), kSampleSdvPackageName);
    EXPECT_EQ(unit_type.unit_type_name(), kSampleUnitTypeName);

    // Check wrapped record since that's what gets passed into ABI functions.
    const ASDV_UnitType* unit_type_ffi = ToFFI(unit_type);
    EXPECT_STREQ((const char*)unit_type_ffi->sdv_package_name, kSampleSdvPackageName);
    EXPECT_STREQ((const char*)unit_type_ffi->unit_type_name, kSampleUnitTypeName);
}

TEST(UnitType, CreateSucceedsForLongest) {
    auto unit_type_result = UnitType::Create(kLongestSdvPackageName, kLongestServiceBundleName,
                                             kLongestUnitTypeName);
    ASSERT_TRUE(unit_type_result.has_value());
    const auto& unit_type = unit_type_result.value();

    EXPECT_EQ(unit_type.sdv_package_name(), kLongestSdvPackageName);
    EXPECT_EQ(unit_type.unit_type_name(), kLongestUnitTypeName);

    // Check wrapped record since that's what gets passed into ABI functions.
    const ASDV_UnitType* unit_type_ffi = ToFFI(unit_type);
    EXPECT_STREQ((const char*)unit_type_ffi->sdv_package_name, kLongestSdvPackageName);
    EXPECT_STREQ((const char*)unit_type_ffi->unit_type_name, kLongestUnitTypeName);
}

TEST(UnitType, CreateFailsForInvalidSdvPackageName) {
    for (std::string sdv_package_name :
         {"", "comgoogsdv", "non.ascїї.sdv", kTooLongSdvPackageName}) {
        auto unit_type_result =
                UnitType::Create(sdv_package_name, kSampleServiceBundleName, kSampleUnitTypeName);
        EXPECT_FALSE(unit_type_result.has_value())
                << "UnitType::Create unexpected success with sdv_package_name \""
                << sdv_package_name << "\"";
        if (!unit_type_result.has_value()) {
            EXPECT_EQ(unit_type_result.error().getStatusCode(), SdvStatusCode::kInvalidArgument)
                    << "UnitType::Create unexpected success with sdv_package_name \""
                    << sdv_package_name << "\"";
        }
    }
}

TEST(UnitType, CreateFailsForInvalidUnitTypeName) {
    for (std::string unit_type_name : {"", "0Temperature", "temperature", "Te__mperature",
                                       "non-ascїї-unit_type-name", kTooLongUnitTypeName}) {
        auto unit_type_result = UnitType::Create(kSampleSdvPackageName, "", unit_type_name);
        EXPECT_FALSE(unit_type_result.has_value())
                << "UnitType::Create unexpected success with unit_type_name \"" << unit_type_name
                << "\"";
        if (!unit_type_result.has_value()) {
            EXPECT_EQ(unit_type_result.error().getStatusCode(), SdvStatusCode::kInvalidArgument)
                    << "UnitType::Create unexpected success with unit_type_name \""
                    << unit_type_name << "\"";
        }
    }
}

// ServerUnitMetadata tests

TEST(ServerUnitMetadata, Copyable) {
    ServerUnitMetadata md = ServerUnitMetadata::Create();
    md.SetVersion(42);

    ServerUnitMetadata md2 = md;
    ASSERT_TRUE(md2.IsValid());
    EXPECT_EQ(md2.GetVersion(), 42U);

    ASSERT_TRUE(md.IsValid());         // Original object still valid
    EXPECT_NE(ToFFI(md), ToFFI(md2));  // Different underlying objects.
}

TEST(ServerUnitMetadata, Movable) {
    ServerUnitMetadata md = ServerUnitMetadata::Create();
    md.SetVersion(42);

    ServerUnitMetadata md2 = std::move(md);
    ASSERT_TRUE(md2.IsValid());
    EXPECT_EQ(md2.GetVersion(), 42U);

    ASSERT_FALSE(md.IsValid());  // Original object moved-from.
}

TEST(ServerUnitMetadata, DefaultVersion) {
    ServerUnitMetadata md = ServerUnitMetadata::Create();

    EXPECT_EQ(md.GetVersion(), 0U);
}

TEST(ServerUnitMetadata, Equal) {
    auto md1 = ServerUnitMetadata::Create();
    auto md2 = ServerUnitMetadata::Create();

    md1.SetVersion(42);
    md2.SetVersion(42);

    EXPECT_EQ(md1, md2);
}

TEST(ServerUnitMetadata, NotEqual) {
    auto md1 = ServerUnitMetadata::Create();
    auto md2 = ServerUnitMetadata::Create();

    md1.SetVersion(42);
    md2.SetVersion(43);

    EXPECT_NE(md1, md2);
}

// PublisherUnitMetadata tests

TEST(PublisherUnitMetadata, AttributeSetAndGet) {
    PublisherUnitMetadata md = PublisherUnitMetadata::Create();

    md.SetVersion(42);
    md.SetMessageSize(43);
    md.SetMessageCount(44);

    EXPECT_EQ(md.GetVersion(), 42U);
    EXPECT_EQ(md.GetMessageSize(), 43U);
    EXPECT_EQ(md.GetMessageCount(), 44U);
}

TEST(PublisherUnitMetadata, Copyable) {
    PublisherUnitMetadata md = PublisherUnitMetadata::Create();
    md.SetVersion(42);

    PublisherUnitMetadata md2 = md;
    ASSERT_TRUE(md2.IsValid());
    EXPECT_EQ(md2.GetVersion(), 42U);

    ASSERT_TRUE(md.IsValid());         // Original object still valid
    EXPECT_NE(ToFFI(md), ToFFI(md2));  // Different underlying objects.
}

TEST(PublisherUnitMetadata, Movable) {
    PublisherUnitMetadata md = PublisherUnitMetadata::Create();
    md.SetVersion(42);

    PublisherUnitMetadata md2 = std::move(md);
    ASSERT_TRUE(md2.IsValid());
    EXPECT_EQ(md2.GetVersion(), 42U);

    ASSERT_FALSE(md.IsValid());  // Original object moved-from.
}

TEST(PublisherUnitMetadata, DefaultVersion) {
    PublisherUnitMetadata md = PublisherUnitMetadata::Create();

    EXPECT_EQ(md.GetVersion(), 0U);
}

TEST(PublisherUnitMetadata, Equal) {
    auto md1 = PublisherUnitMetadata::Create();
    auto md2 = PublisherUnitMetadata::Create();

    md1.SetVersion(42);
    md2.SetVersion(42);

    EXPECT_EQ(md1, md2);
}

TEST(PublisherUnitMetadata, NotEqual) {
    auto md1 = PublisherUnitMetadata::Create();
    auto md2 = PublisherUnitMetadata::Create();

    md1.SetVersion(42);
    md2.SetVersion(43);

    EXPECT_NE(md1, md2);
}

// UnitNameDiscoveryArgs tests

TEST(UnitNameDiscoveryArgs, AppropriatelySized) {
    ASSERT_EQ(sizeof(UnitNameDiscoveryArgs), sizeof(ASDV_UnitNameDiscoveryArgs));
}

TEST(UnitNameDiscoveryArgs, CreateSuccess) {
    SdvResult<UnitNameDiscoveryArgs> unda_result =
            UnitNameDiscoveryArgs::Create(kSampleSdvPackageName, kSampleServiceBundleName,
                                          kSampleUnitName, kSampleSdvVmName);
    ASSERT_TRUE(unda_result.has_value());
    EXPECT_EQ(unda_result->sdv_package_name(), kSampleSdvPackageName);
    EXPECT_EQ(unda_result->service_bundle_name(), kSampleServiceBundleName);
    EXPECT_EQ(unda_result->unit_name(), kSampleUnitName);
    EXPECT_EQ(unda_result->sdv_vm_name(), kSampleSdvVmName);

    // Validate wrapped FFI object since that's going to be used by other SDK code.
    const ASDV_UnitNameDiscoveryArgs* unda_ffi = ToFFI(unda_result.value());
    EXPECT_STREQ((const char*)unda_ffi->sdv_package_name, kSampleSdvPackageName);
    EXPECT_STREQ((const char*)unda_ffi->service_bundle_name, kSampleServiceBundleName);
    EXPECT_STREQ((const char*)unda_ffi->unit_name, kSampleUnitName);
    EXPECT_STREQ((const char*)unda_ffi->sdv_vm_name, kSampleSdvVmName);
}

TEST(UnitNameDiscoveryArgs, CreateSuccessWithoutOptionals) {
    SdvResult<UnitNameDiscoveryArgs> unda_result =
            UnitNameDiscoveryArgs::Create(kSampleSdvPackageName, kSampleServiceBundleName);
    ASSERT_TRUE(unda_result.has_value());
    EXPECT_EQ(unda_result->sdv_package_name(), kSampleSdvPackageName);
    EXPECT_EQ(unda_result->service_bundle_name(), kSampleServiceBundleName);
    EXPECT_EQ(unda_result->unit_name(), "");
    EXPECT_EQ(unda_result->sdv_vm_name(), "");

    // Validate wrapped FFI object since that's going to be used by other SDK code.
    const ASDV_UnitNameDiscoveryArgs* unda_ffi = ToFFI(unda_result.value());
    EXPECT_STREQ((const char*)unda_ffi->sdv_package_name, kSampleSdvPackageName);
    EXPECT_STREQ((const char*)unda_ffi->service_bundle_name, kSampleServiceBundleName);
    EXPECT_STREQ((const char*)unda_ffi->unit_name, "");
    EXPECT_STREQ((const char*)unda_ffi->sdv_vm_name, "");
}

TEST(UnitNameDiscoveryArgs, CreateSuccessWithLongestFieldValues) {
    SdvResult<UnitNameDiscoveryArgs> unda_result =
            UnitNameDiscoveryArgs::Create(kLongestSdvPackageName, kLongestServiceBundleName,
                                          kLongestUnitName, kLongestSdvVmName);
    ASSERT_TRUE(unda_result.has_value());
    EXPECT_EQ(unda_result->sdv_package_name(), kLongestSdvPackageName);
    EXPECT_EQ(unda_result->service_bundle_name(), kLongestServiceBundleName);
    EXPECT_EQ(unda_result->unit_name(), kLongestUnitName);
    EXPECT_EQ(unda_result->sdv_vm_name(), kLongestSdvVmName);

    // Validate wrapped FFI object since that's going to be used by other SDK code.
    const ASDV_UnitNameDiscoveryArgs* unda_ffi = ToFFI(unda_result.value());
    EXPECT_STREQ((const char*)unda_ffi->sdv_package_name, kLongestSdvPackageName);
    EXPECT_STREQ((const char*)unda_ffi->service_bundle_name, kLongestServiceBundleName);
    EXPECT_STREQ((const char*)unda_ffi->unit_name, kLongestUnitName);
    EXPECT_STREQ((const char*)unda_ffi->sdv_vm_name, kLongestSdvVmName);
}

TEST(UnitNameDiscoveryArgs, CreateFailsWithTooLongFieldValues) {
    auto unda_result =
            UnitNameDiscoveryArgs::Create(kTooLongSdvPackageName, kSampleServiceBundleName,
                                          kSampleUnitName, kSampleSdvVmName);
    ASSERT_FALSE(unda_result.has_value());
    EXPECT_EQ(unda_result.error().getStatusCode(), sdv::SdvStatusCode::kInvalidArgument);

    unda_result = UnitNameDiscoveryArgs::Create(kSampleSdvPackageName, kTooLongServiceBundleName,
                                                kSampleUnitName, kSampleSdvVmName);
    EXPECT_EQ(unda_result.error().getStatusCode(), sdv::SdvStatusCode::kInvalidArgument);
    ASSERT_FALSE(unda_result.has_value());

    unda_result = UnitNameDiscoveryArgs::Create(kSampleSdvPackageName, kSampleServiceBundleName,
                                                kTooLongUnitName, kSampleSdvVmName);
    ASSERT_FALSE(unda_result.has_value());
    EXPECT_EQ(unda_result.error().getStatusCode(), sdv::SdvStatusCode::kInvalidArgument);

    unda_result = UnitNameDiscoveryArgs::Create(kSampleSdvPackageName, kSampleServiceBundleName,
                                                kSampleUnitName, kTooLongSdvVmName);
    ASSERT_FALSE(unda_result.has_value());
    EXPECT_EQ(unda_result.error().getStatusCode(), sdv::SdvStatusCode::kInvalidArgument);
}

// InferredMetadata tests
TEST(InferredMetadata, Movable) {
    InferredMetadata md = InferredMetadata::Create();
    ASSERT_TRUE(md.IsValid());
    // Ugliness to grab the underlying pointer.
    auto md_ptr = *reinterpret_cast<ASDV_InferredMetadata**>(&md);

    InferredMetadata md2 = std::move(md);
    EXPECT_TRUE(md2.IsValid());
    EXPECT_FALSE(md.IsValid());  // Original object moved-from.
    EXPECT_EQ(*reinterpret_cast<ASDV_InferredMetadata**>(&md2), md_ptr);  // Handle moved.
}

TEST(InferredMetadata, DefaultLocation) {
    InferredMetadata md = InferredMetadata::Create();
    ASSERT_TRUE(md.IsValid());

    EXPECT_EQ(md.GetLocation(), InferredMetadata::RelativeLocation::Unknown);
}

// No ServiceUnitDefinition tests
//
// No unit tests possible: ABI does not provide a means to create an
// ASDV_ServiceUnitDefinition directly.  Rust SDK tests its version of
// this wrapper by calling internal code that isn't exposed in the ABI
// to construct an ASDV_ServiceUnitDefinition, but that code isn't
// callable from C++.  ServiceUnitDefinition is exercised as part of
// ServiceDiscoveryManager and ServiceUnitChangeListener tests in
// sd_e2e_test.cpp.

}  // namespace sdv_comms::sd::test
