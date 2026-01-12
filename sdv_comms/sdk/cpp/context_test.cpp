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
#include <sdv/ctx.h>

#include <gtest/gtest-spi.h>
#include <sdv/context.hpp>
#include <sdv/identity.hpp>

const char kSampleFqinStr[] = "vm:com.legoog.sdv.WindowManager/sample";
const char kSampleSdvPackageName[] = "com.legoog.sdv";
const char kSampleServiceBundleName[] = "WindowManager";
const char kSampleServiceInstanceName[] = "sample";

namespace sdv_comms::ctx::test {

TEST(Context, CreateAndVerifyIdentifiers) {
    auto fqin = id::ServiceFqin::FromString(kSampleFqinStr).value();

    // Create temporary ASDV_Context to test the wrapper.
    const ASDV_Context* ctx_ffi = ASDV_Context_temporaryCreate(ToFFI(fqin));

    auto context_result = Context::Create(ctx_ffi);
    ASSERT_TRUE(context_result.has_value());
    Context context = context_result.value();
    ASSERT_TRUE(context.IsValid());

    auto identity_result = context.GetSelfIdentity();
    ASSERT_TRUE(identity_result.has_value());

    auto fqin_result = context.GetSelfFqin();
    ASSERT_TRUE(fqin_result.has_value());

    auto public_key_result = context.GetPublicKey();
    ASSERT_TRUE(public_key_result.has_value());

    EXPECT_EQ(fqin_result.value().sdv_package_name(), kSampleSdvPackageName);
    EXPECT_EQ(fqin_result.value().service_bundle_name(), kSampleServiceBundleName);
    EXPECT_EQ(fqin_result.value().service_instance_name(), kSampleServiceInstanceName);
    EXPECT_EQ(identity_result.value().fqin(), fqin_result.value());
    EXPECT_EQ(identity_result.value().public_key(), public_key_result.value());
}

TEST(Context, CreateFailsWithNullHandle) {
    const ASDV_Context* a_null_context = nullptr;
    EXPECT_EQ(Context::Create(a_null_context).error().getStatusCode(),
              sdv::SdvStatusCode::kInvalidArgument);
}

// Note: EXPECT_FATAL_FAILURE doesn't seem to work as expected here, possibly something to do
// with the test running on-device.  Manually enabling and running the test and inspecting the
// logs shows the expected log message and crash.
TEST(Context, DISABLED_ConstructorRaisesFatalFailureOnNullHandle) {
    static auto bad_constructor_call = []() {
        const ASDV_Context* a_null_context = nullptr;
        auto bad_context = Context(a_null_context);
    };

    EXPECT_FATAL_FAILURE(bad_constructor_call(), "Null handle passed to Context constructor");
}

}  // namespace sdv_comms::ctx::test
