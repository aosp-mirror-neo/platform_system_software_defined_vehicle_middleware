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
#include <sdv/sd_callbacks.h>
#include <sdv/unit_discovery.h>
#include <sdv/unit_metadata.h>
#include <sdv/unit_registration.h>
#include <sdv/unit_type.h>

#include <sdv/sd_common.hpp>
#include <sdv/sd_registration.hpp>

using sdv::SdvResult;
using sdv::SdvStatus;
using sdv::SdvStatusCode;

namespace sdv_comms::sd {

// reinterpret_cast to get ahold of the handle so we can show that move-construction
// just moves the handle.
inline const ASDV_UnitRegistrationArgs* ToFFI(const sd::UnitRegistrationArgs& ura) {
    return *reinterpret_cast<ASDV_UnitRegistrationArgs* const*>(&ura);
}

TEST(UnitRegistrationArgs, Moveable) {
    UnitRegistrationArgs ura = UnitRegistrationArgs::Create();
    ASSERT_TRUE(ura.IsValid());
    const ASDV_UnitRegistrationArgs* data = ToFFI(ura);
    UnitRegistrationArgs ura2 = std::move(ura);
    EXPECT_TRUE(ura2.IsValid());
    EXPECT_FALSE(ura.IsValid());
    EXPECT_EQ(ToFFI(ura2), data);
}

TEST(UnitRegistrationArgs, SetGetUnitName) {
    UnitRegistrationArgs ura = UnitRegistrationArgs::Create();

    // Check default value.
    EXPECT_EQ(ura.GetUnitName(), "");

    // Try invalid value; no uppercase permitted.
    auto status = ura.SetUnitName("Tinu");
    EXPECT_EQ(status.getStatusCode(), SdvStatusCode::kInvalidArgument);
    EXPECT_EQ(ura.GetUnitName(), "");

    // Try too-long value
    std::string long_unit_name(10000, 'x');
    status = ura.SetUnitName(long_unit_name);
    EXPECT_EQ(status.getStatusCode(), SdvStatusCode::kInvalidArgument);
    EXPECT_EQ(ura.GetUnitName(), "");

    // Success case.
    status = ura.SetUnitName("tinu");
    ASSERT_EQ(status.getStatusCode(), SdvStatusCode::kOk);
    EXPECT_EQ(ura.GetUnitName(), "tinu");

    // Move the handle.
    UnitRegistrationArgs ura2 = std::move(ura);
    EXPECT_EQ(ura.GetUnitName(), "");
    EXPECT_EQ(ura2.GetUnitName(), "tinu");
}

TEST(UnitRegistrationArgs, SetGetUnitType) {
    static const char* kUnitTypeString = "com.legoog.sdv.Temperature";
    UnitType sample_ut = UnitType::FromString(kUnitTypeString).value();

    UnitRegistrationArgs ura = UnitRegistrationArgs::Create();

    UnitType invalid_ut = ura.GetUnitType();
    ASSERT_FALSE(invalid_ut.IsValid());

    // Try with invalid argument.
    auto status = ura.SetUnitType(invalid_ut);
    ASSERT_EQ(status.getStatusCode(), SdvStatusCode::kInvalidArgument);

    status = ura.SetUnitType(sample_ut);
    ASSERT_EQ(status.getStatusCode(), SdvStatusCode::kOk);

    auto new_ut = ura.GetUnitType();
    ASSERT_TRUE(new_ut.IsValid());
    EXPECT_EQ(new_ut.ToString(), kUnitTypeString);

    // Move the handle and see the UnitType move to the new holder.
    UnitRegistrationArgs ura2 = std::move(ura);
    EXPECT_FALSE(ura.GetUnitType().IsValid());
    EXPECT_EQ(ura2.GetUnitType().ToString(), kUnitTypeString);

    // Set on invalid URA
    EXPECT_EQ(ura.SetUnitType(sample_ut).getStatusCode(), SdvStatusCode::kInvalidArgument);
}

TEST(UnitRegistrationArgs, SetGetUnitMetadata) {
    UnitRegistrationArgs ura = UnitRegistrationArgs::Create();

    EXPECT_TRUE(std::holds_alternative<NoUnitMetadata>(ura.GetUnitMetadata()));

    ServerUnitMetadata sud = ServerUnitMetadata::Create();
    sud.SetVersion(31337);

    auto status = ura.SetUnitMetadata(sud);
    ASSERT_EQ(status.getStatusCode(), SdvStatusCode::kOk);

    auto ura_umd = ura.GetUnitMetadata();
    ASSERT_TRUE(std::holds_alternative<ServerUnitMetadata>(ura_umd));
    EXPECT_EQ(std::get<ServerUnitMetadata>(ura_umd), sud);

    PublisherUnitMetadata pud = PublisherUnitMetadata::Create();
    pud.SetVersion(42);

    status = ura.SetUnitMetadata(pud);
    ASSERT_EQ(status.getStatusCode(), SdvStatusCode::kOk);

    auto ura_umd2 = ura.GetUnitMetadata();
    ASSERT_TRUE(std::holds_alternative<PublisherUnitMetadata>(ura_umd2));
    EXPECT_EQ(std::get<PublisherUnitMetadata>(ura_umd2), pud);

    // Move the handle.
    UnitRegistrationArgs ura2 = std::move(ura);
    EXPECT_TRUE(std::holds_alternative<NoUnitMetadata>(ura.GetUnitMetadata()));
    auto ura_umd3 = ura2.GetUnitMetadata();
    ASSERT_TRUE(std::holds_alternative<PublisherUnitMetadata>(ura_umd2));
    EXPECT_EQ(std::get<PublisherUnitMetadata>(ura_umd3), pud);

    // Set on invalid URA
    EXPECT_EQ(ura.SetUnitMetadata(sud).getStatusCode(), SdvStatusCode::kInvalidArgument);
}

TEST(UnitRegistrationArgs, SetGetServerUnitMetadata) {
    UnitRegistrationArgs ura = UnitRegistrationArgs::Create();

    ASSERT_FALSE(ura.GetServerUnitMetadata().IsValid());

    ServerUnitMetadata sud = ServerUnitMetadata::Create();
    sud.SetVersion(31337);

    auto status = ura.SetServerUnitMetadata(sud);
    ASSERT_EQ(status.getStatusCode(), SdvStatusCode::kOk);

    EXPECT_EQ(ura.GetServerUnitMetadata(), sud);

    // Move the handle.
    UnitRegistrationArgs ura2 = std::move(ura);
    EXPECT_TRUE(std::holds_alternative<NoUnitMetadata>(ura.GetUnitMetadata()));
    EXPECT_FALSE(ura.GetServerUnitMetadata().IsValid());
    EXPECT_EQ(ura2.GetServerUnitMetadata(), sud);

    // Set on invalid URA
    EXPECT_EQ(ura.SetServerUnitMetadata(sud).getStatusCode(), SdvStatusCode::kInvalidArgument);
}

TEST(UnitRegistrationArgs, SetGetPublisherUnitMetadata) {
    UnitRegistrationArgs ura = UnitRegistrationArgs::Create();

    ASSERT_FALSE(ura.GetPublisherUnitMetadata().IsValid());

    PublisherUnitMetadata pud = PublisherUnitMetadata::Create();
    pud.SetVersion(42);

    auto status = ura.SetPublisherUnitMetadata(pud);
    ASSERT_EQ(status.getStatusCode(), SdvStatusCode::kOk);

    EXPECT_EQ(ura.GetPublisherUnitMetadata(), pud);

    // Move the handle.
    UnitRegistrationArgs ura2 = std::move(ura);
    EXPECT_FALSE(ura.GetPublisherUnitMetadata().IsValid());
    EXPECT_EQ(ura2.GetPublisherUnitMetadata(), pud);

    // Set on invalid URA
    EXPECT_EQ(ura.SetPublisherUnitMetadata(pud).getStatusCode(), SdvStatusCode::kInvalidArgument);
}

TEST(UnitRegistrationArgs, UnitMetadataInteractions) {
    UnitRegistrationArgs ura = UnitRegistrationArgs::Create();
    ServerUnitMetadata sud = ServerUnitMetadata::Create();
    PublisherUnitMetadata pud = PublisherUnitMetadata::Create();

    // Default state
    EXPECT_TRUE(std::holds_alternative<NoUnitMetadata>(ura.GetUnitMetadata()));
    EXPECT_FALSE(ura.GetServerUnitMetadata().IsValid());
    EXPECT_FALSE(ura.GetPublisherUnitMetadata().IsValid());

    // Set ServerUnitMetadata via SetUnitMetadata
    ura.SetUnitMetadata(sud);
    EXPECT_TRUE(std::holds_alternative<ServerUnitMetadata>(ura.GetUnitMetadata()));
    EXPECT_TRUE(ura.GetServerUnitMetadata().IsValid());
    EXPECT_FALSE(ura.GetPublisherUnitMetadata().IsValid());

    // Set PublisherUnitMetadata via SetUnitMetadata
    ura.SetUnitMetadata(pud);
    EXPECT_TRUE(std::holds_alternative<PublisherUnitMetadata>(ura.GetUnitMetadata()));
    EXPECT_FALSE(ura.GetServerUnitMetadata().IsValid());
    EXPECT_TRUE(ura.GetPublisherUnitMetadata().IsValid());

    // Set ServerUnitMetadata via SetServerUnitMetadata
    ura.SetServerUnitMetadata(sud);
    EXPECT_TRUE(std::holds_alternative<ServerUnitMetadata>(ura.GetUnitMetadata()));
    EXPECT_TRUE(ura.GetServerUnitMetadata().IsValid());
    EXPECT_FALSE(ura.GetPublisherUnitMetadata().IsValid());

    // Set PublisherUnitMetadata via SetPublisherUnitMetadata
    ura.SetPublisherUnitMetadata(pud);
    EXPECT_TRUE(std::holds_alternative<PublisherUnitMetadata>(ura.GetUnitMetadata()));
    EXPECT_FALSE(ura.GetServerUnitMetadata().IsValid());
    EXPECT_TRUE(ura.GetPublisherUnitMetadata().IsValid());
}

TEST(UnitRegistrationArgs, SetApplicationMetadata) {
    std::string app_md = "deadbeef";

    UnitRegistrationArgs ura = UnitRegistrationArgs::Create();

    auto status = ura.SetApplicationMetadata((unsigned char*)app_md.data(), app_md.size());
    ASSERT_EQ(status.getStatusCode(), SdvStatusCode::kOk);
}

}  // namespace sdv_comms::sd