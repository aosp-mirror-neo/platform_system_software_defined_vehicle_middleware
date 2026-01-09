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

// Service Discovery Tests of lookup operations and listeners.

#include "callback_helpers.hpp"
#include "ffi_helpers.hpp"

#include <android-base/logging.h>
#include <binder/ProcessState.h>
#include <gtest/gtest.h>
#include <sdv/sd_callbacks.h>
#include <sdv/unit_discovery.h>
#include <sdv/unit_metadata.h>
#include <sdv/unit_registration.h>

#include <atomic>
#include <thread>
#include <unordered_set>

#include <sdv/dt_publisher.hpp>
#include <sdv/sd_listener.hpp>
#include <sdv/sd_lookup.hpp>

namespace sdv_comms::sd::test {

static const char kSampleFqinStr[] = "vm:com.legoog.sdv.WindowManager/sample";
static const char kSampleUnitType1Str[] = "com.legoog.sdv.Temperature";
static const char kSampleUnitType2Str[] = "com.legoog.sdv.Volume";

// Create a context for use in all tests in this file.
ctx::Context _TestContext() {
    static const ASDV_Context* ctx_ffi = []() {
        auto fqin_result = id::ServiceFqin::FromString(kSampleFqinStr);
        CHECK(fqin_result.has_value());
        auto fqin = fqin_result.value();

        const ASDV_Context* ctx_ffi = ASDV_Context_temporaryCreate(ToFFI(fqin));
        CHECK(ctx_ffi);
        return ctx_ffi;
    }();

    return ctx::Context::Create(ctx_ffi).value();
}

id::UnitIdentifier _MakeUnitIdentifier(const ctx::Context& context, const std::string& unit_name) {
    auto service_id = context.GetSelfIdentity();
    CHECK(service_id.has_value());
    auto unit_id = id::UnitIdentifier::Create(service_id.value(), unit_name);
    CHECK(unit_id.has_value());
    return unit_id.value();
}

// Stores details of a publisher for SD testing purposes.  Provides methods for
// actually creating the publisher in a given context, and for validating that a
// ServiceUnitDefinition  matches the spec.
struct _PublisherSpec {
    std::string unit_name;
    std::string unit_type;
    uint32_t version = 1;
    uint32_t message_size = 32;
    uint32_t message_count = 8;
    uint8_t app_metadata[2];

    // Create a publisher in the given context from the spec
    dt::Publisher CreatePublisherInContext(const ctx::Context& context) const {
        auto ura = sd::UnitRegistrationArgs::Create();

        CHECK(ura.SetUnitName(unit_name).isOk());

        auto sample_ut = sd::UnitType::FromString(unit_type);
        CHECK(sample_ut.has_value());
        CHECK(ura.SetUnitType(sample_ut.value()).isOk());

        auto pub_md = sd::PublisherUnitMetadata::Create();
        CHECK(pub_md.SetVersion(version).isOk());
        CHECK(pub_md.SetMessageSize(message_size).isOk());
        CHECK(pub_md.SetMessageCount(message_count).isOk());
        CHECK(ura.SetPublisherUnitMetadata(pub_md).isOk());

        CHECK(ura.SetApplicationMetadata(app_metadata, sizeof(app_metadata)).isOk());

        auto pub_result = dt::Publisher::Create(context, std::move(ura));
        CHECK(pub_result.has_value());

        return std::move(pub_result.value());
    }

    // Check publisher details in a ServiceUnitDefinition against context and spec.
    void ValidateServiceUnitDefinition(const ctx::Context& context,
                                       const ServiceUnitDefinition& sud) const {
        // Check IDs and type.
        EXPECT_EQ(sud.GetUnitIdentifier(), _MakeUnitIdentifier(context, unit_name));
        EXPECT_EQ(sud.GetServiceBundleIdentity(), context.GetSelfIdentity().value());
        EXPECT_EQ(sud.GetUnitType().ToString(), unit_type);

        // Check InferredMetadata
        auto inferred_md = sud.GetInferredMetadata();
        ASSERT_TRUE(inferred_md.IsValid());
        EXPECT_EQ(inferred_md.GetLocation(), InferredMetadata::RelativeLocation::LocalSdvInstance);

        // Check that GetUnitMetadata returns expected type containing expected values.
        auto unit_md = sud.GetUnitMetadata();
        EXPECT_TRUE(std::holds_alternative<PublisherUnitMetadata>(unit_md));
        auto unit_pub_md = std::get<PublisherUnitMetadata>(unit_md);
        EXPECT_EQ(unit_pub_md.GetVersion(), version);
        EXPECT_EQ(unit_pub_md.GetMessageSize(), message_size);
        EXPECT_EQ(unit_pub_md.GetMessageCount(), message_count);

        // Check that there's no ServerUnitMetadata present since this is a Publisher
        EXPECT_FALSE(sud.GetServerUnitMetadata().IsValid());

        // Check that type-specific metadata function returns correct data.
        auto pub_md = sud.GetPublisherUnitMetadata();
        ASSERT_TRUE(pub_md.IsValid());
        EXPECT_EQ(pub_md.GetVersion(), version);
        EXPECT_EQ(pub_md.GetMessageSize(), message_size);
        EXPECT_EQ(pub_md.GetMessageCount(), message_count);

        // Check that correct application-specific metadata is returned.
        auto app_md = sud.GetApplicationMetadata();
        EXPECT_EQ(app_md.size(), 2U);
        EXPECT_TRUE(app_md[0] == app_metadata[0] && app_md[1] == app_metadata[1]);
    }
};

// Creates a group of publisher specs for use in a test.  The name_suffix parameter
// is appended to unit names and unit types and should be different in each test to
// prevent collisions between tests.
std::vector<_PublisherSpec> _TestPublisherSpecs(const std::string& name_suffix) {
    // First two entries have the same type, third is different.
    return std::vector<_PublisherSpec>{_PublisherSpec{.unit_name = "unit1-" + name_suffix,
                                                      .unit_type = kSampleUnitType1Str +
                                                              ("_" + name_suffix),
                                                      .version = 10,
                                                      .message_size = 11,
                                                      .message_count = 12,
                                                      .app_metadata = {13, 14}},
                                       _PublisherSpec{.unit_name = "unit2-" + name_suffix,
                                                      .unit_type = kSampleUnitType1Str +
                                                              ("_" + name_suffix),
                                                      .version = 20,
                                                      .message_size = 21,
                                                      .message_count = 22,
                                                      .app_metadata = {23, 24}},
                                       _PublisherSpec{.unit_name = "unit3-" + name_suffix,
                                                      .unit_type = kSampleUnitType2Str +
                                                              ("_" + name_suffix),
                                                      .version = 30,
                                                      .message_size = 31,
                                                      .message_count = 32,
                                                      .app_metadata = {33, 34}}};
}

// ServiceDiscoveryManager tests

// Set up a context and set of publishers that ServiceDiscoveryManager tests will
// work against.
auto _SetupTestData(const std::string& name_suffix) {
    auto context = _TestContext();
    auto sdm = ServiceDiscoveryManager(context);

    auto pub_specs = _TestPublisherSpecs(name_suffix);

    std::vector<dt::Publisher> publishers;
    for (const auto& spec : pub_specs) {
        publishers.emplace_back(spec.CreatePublisherInContext(context));
    }
    return std::make_tuple(context, std::move(pub_specs), std::move(publishers));
}

TEST(ServiceDiscoveryManager, GetUnitAsyncSuccess) {
    auto [context, pub_specs, publishers] = _SetupTestData("getunitasyncsuccess");

    auto sdm = ServiceDiscoveryManager(context);

    // Wrapping promise in shared_ptr for copyability.
    auto result_promise = std::make_shared<std::promise<ServiceUnitResult>>();
    auto result_future = result_promise->get_future();

    sdm.GetUnitAsync(context.GetSelfFqin().value(), pub_specs[0].unit_name,
                     [result_promise](auto result) {
                         result_promise->set_value(std::move(result));
                     });

    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(5)), std::future_status::ready);
    auto result = result_future.get();

    ASSERT_TRUE(result.has_value());
    auto sud = result.value();

    pub_specs[0].ValidateServiceUnitDefinition(context, sud);
}

TEST(ServiceDiscoveryManager, GetUnitFutureSuccess) {
    auto [context, pub_specs, publishers] = _SetupTestData("getunitfuturesuccess");

    auto sdm = ServiceDiscoveryManager(context);

    auto result_future = sdm.GetUnitFuture(context.GetSelfFqin().value(), pub_specs[0].unit_name);
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(5)), std::future_status::ready);
    auto result = result_future.get();
    ASSERT_TRUE(result.has_value());
    auto sud = result.value();

    pub_specs[0].ValidateServiceUnitDefinition(context, sud);
}

TEST(ServiceDiscoveryManager, GetUnitBlockingSuccess) {
    auto [context, pub_specs, publishers] = _SetupTestData("getunitblockingsuccess");

    auto sdm = ServiceDiscoveryManager(context);

    auto result = sdm.GetUnit(context.GetSelfFqin().value(), pub_specs[0].unit_name);
    ASSERT_TRUE(result.has_value());
    auto sud = result.value();

    pub_specs[0].ValidateServiceUnitDefinition(context, sud);
}

TEST(ServiceDiscoveryManager, GetUnitNoneFound) {
    auto [context, pub_specs, publishers] = _SetupTestData("getunitnonefound");

    auto sdm = ServiceDiscoveryManager(context);

    auto result = sdm.GetUnit(context.GetSelfFqin().value(), "unknownunitname");
    ASSERT_FALSE(result.has_value());
    EXPECT_EQ(result.error().getStatusCode(), sdv::SdvStatusCode::kNotFound);
}

TEST(ServiceDiscoveryManager, ListUnitsByTypeAsyncSuccess) {
    auto [context, pub_specs, publishers] = _SetupTestData("listbytypeasyncsuccess");
    auto sdm = ServiceDiscoveryManager(context);

    // Wrapping promise in shared_ptr for copyability.
    auto result_promise = std::make_shared<std::promise<ServiceUnitListResult>>();
    auto result_future = result_promise->get_future();

    auto unit_type_1 = sd::UnitType::FromString(pub_specs[0].unit_type).value();
    sdm.ListUnitsByTypeAsync(unit_type_1, [result_promise](auto result) {
        result_promise->set_value(std::move(result));
    });
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(5)), std::future_status::ready);
    auto result = result_future.get();

    ASSERT_TRUE(result.has_value());
    auto sud_list = result.value();
    EXPECT_EQ(sud_list.size(), 2U);

    std::unordered_set<std::string> found_unit_names;
    for (const auto& sud : sud_list) {
        std::string unit_name(sud.GetUnitIdentifier().unit_name());
        auto pub_spec_iter =
                std::find_if(pub_specs.begin(), pub_specs.end(),
                             [&](const auto& spec) { return spec.unit_name == unit_name; });
        ASSERT_NE(pub_spec_iter, pub_specs.end());
        pub_spec_iter->ValidateServiceUnitDefinition(context, sud);
        found_unit_names.emplace(unit_name);
    }
    EXPECT_TRUE(found_unit_names.contains(pub_specs[0].unit_name));
    EXPECT_TRUE(found_unit_names.contains(pub_specs[1].unit_name));
}

TEST(ServiceDiscoveryManager, ListUnitsByTypeFutureSuccess) {
    auto [context, pub_specs, publishers] = _SetupTestData("listbytypefuturesuccess");
    auto sdm = ServiceDiscoveryManager(context);

    auto unit_type_1 = sd::UnitType::FromString(pub_specs[0].unit_type).value();
    auto result_future = sdm.ListUnitsByTypeFuture(unit_type_1);
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(5)), std::future_status::ready);
    auto result = result_future.get();

    ASSERT_TRUE(result.has_value());
    auto sud_list = result.value();
    EXPECT_EQ(sud_list.size(), 2U);

    std::unordered_set<std::string> found_unit_names;
    for (const auto& sud : sud_list) {
        std::string unit_name(sud.GetUnitIdentifier().unit_name());
        auto pub_spec_iter =
                std::find_if(pub_specs.begin(), pub_specs.end(),
                             [&](const auto& spec) { return spec.unit_name == unit_name; });
        ASSERT_NE(pub_spec_iter, pub_specs.end());
        pub_spec_iter->ValidateServiceUnitDefinition(context, sud);
        found_unit_names.emplace(unit_name);
    }
    EXPECT_TRUE(found_unit_names.contains(pub_specs[0].unit_name));
    EXPECT_TRUE(found_unit_names.contains(pub_specs[1].unit_name));
}

TEST(ServiceDiscoveryManager, ListUnitsByTypeBlockingSuccess) {
    auto [context, pub_specs, publishers] = _SetupTestData("listbytypeblockingsuccess");
    auto sdm = ServiceDiscoveryManager(context);

    auto unit_type_1 = sd::UnitType::FromString(pub_specs[0].unit_type).value();
    auto result = sdm.ListUnitsByType(unit_type_1);

    ASSERT_TRUE(result.has_value());
    auto sud_list = result.value();
    EXPECT_EQ(sud_list.size(), 2U);

    std::unordered_set<std::string> found_unit_names;
    for (const auto& sud : sud_list) {
        std::string unit_name(sud.GetUnitIdentifier().unit_name());
        auto pub_spec_iter =
                std::find_if(pub_specs.begin(), pub_specs.end(),
                             [&](const auto& spec) { return spec.unit_name == unit_name; });
        ASSERT_NE(pub_spec_iter, pub_specs.end());
        pub_spec_iter->ValidateServiceUnitDefinition(context, sud);
        found_unit_names.emplace(unit_name);
    }
    EXPECT_TRUE(found_unit_names.contains(pub_specs[0].unit_name));
    EXPECT_TRUE(found_unit_names.contains(pub_specs[1].unit_name));
}

TEST(ServiceDiscoveryManager, ListUnitsByTypeNoneFound) {
    auto [context, pub_specs, publishers] = _SetupTestData("listbytypenonefound");
    auto sdm = ServiceDiscoveryManager(context);

    auto unit_type = sd::UnitType::FromString(std::string(kSampleUnitType1Str) + "x").value();
    auto result = sdm.ListUnitsByType(unit_type);

    // Get back "success" with empty list.
    ASSERT_TRUE(result.has_value());
    EXPECT_TRUE(result.value().empty());
}

TEST(ServiceDiscoveryManager, ListUnitsByNameAsyncSuccess) {
    auto [context, pub_specs, publishers] = _SetupTestData("listbynameasyncsuccess");
    auto sdm = ServiceDiscoveryManager(context);

    // Wrapping promise in shared_ptr for copyability.
    auto result_promise = std::make_shared<std::promise<ServiceUnitListResult>>();
    auto result_future = result_promise->get_future();

    auto unda = UnitNameDiscoveryArgs::Create("com.legoog.sdv", "WindowManager");
    ASSERT_TRUE(unda.has_value());
    sdm.ListUnitsByNameAsync(unda.value(), [result_promise](auto result) {
        result_promise->set_value(std::move(result));
    });
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(5)), std::future_status::ready);
    auto result = result_future.get();

    ASSERT_TRUE(result.has_value());
    auto sud_list = result.value();
    EXPECT_GE(sud_list.size(), 3U);

    std::unordered_set<std::string> found_unit_names;
    for (const auto& sud : sud_list) {
        std::string unit_name(sud.GetUnitIdentifier().unit_name());
        auto pub_spec_iter =
                std::find_if(pub_specs.begin(), pub_specs.end(),
                             [&](const auto& spec) { return spec.unit_name == unit_name; });
        if (pub_spec_iter != pub_specs.end()) {
            pub_spec_iter->ValidateServiceUnitDefinition(context, sud);
            found_unit_names.emplace(unit_name);
        }
    }
    EXPECT_TRUE(found_unit_names.contains(pub_specs[0].unit_name));
    EXPECT_TRUE(found_unit_names.contains(pub_specs[1].unit_name));
    EXPECT_TRUE(found_unit_names.contains(pub_specs[2].unit_name));
}

TEST(ServiceDiscoveryManager, ListUnitsByNameFutureSuccess) {
    auto [context, pub_specs, publishers] = _SetupTestData("listbynamefuturesuccess");
    auto sdm = ServiceDiscoveryManager(context);

    auto unda = UnitNameDiscoveryArgs::Create("com.legoog.sdv", "WindowManager");
    ASSERT_TRUE(unda.has_value());
    auto result_future = sdm.ListUnitsByNameFuture(unda.value());
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(5)), std::future_status::ready);
    auto result = result_future.get();

    ASSERT_TRUE(result.has_value());
    auto sud_list = result.value();
    EXPECT_GE(sud_list.size(), 3U);

    std::unordered_set<std::string> found_unit_names;
    for (const auto& sud : sud_list) {
        std::string unit_name(sud.GetUnitIdentifier().unit_name());
        auto pub_spec_iter =
                std::find_if(pub_specs.begin(), pub_specs.end(),
                             [&](const auto& spec) { return spec.unit_name == unit_name; });
        if (pub_spec_iter != pub_specs.end()) {
            pub_spec_iter->ValidateServiceUnitDefinition(context, sud);
            found_unit_names.emplace(unit_name);
        }
    }
    EXPECT_TRUE(found_unit_names.contains(pub_specs[0].unit_name));
    EXPECT_TRUE(found_unit_names.contains(pub_specs[1].unit_name));
    EXPECT_TRUE(found_unit_names.contains(pub_specs[2].unit_name));
}

TEST(ServiceDiscoveryManager, ListUnitsByNameBlockingSuccess) {
    auto [context, pub_specs, publishers] = _SetupTestData("listbynameblockingsuccess");
    auto sdm = ServiceDiscoveryManager(context);

    auto unda = UnitNameDiscoveryArgs::Create("com.legoog.sdv", "WindowManager");
    ASSERT_TRUE(unda.has_value());
    auto result = sdm.ListUnitsByName(unda.value());
    ASSERT_TRUE(result.has_value());
    auto sud_list = result.value();
    EXPECT_GE(sud_list.size(), 3U);

    std::unordered_set<std::string> found_unit_names;
    for (const auto& sud : sud_list) {
        std::string unit_name(sud.GetUnitIdentifier().unit_name());
        auto pub_spec_iter =
                std::find_if(pub_specs.begin(), pub_specs.end(),
                             [&](const auto& spec) { return spec.unit_name == unit_name; });
        if (pub_spec_iter != pub_specs.end()) {
            pub_spec_iter->ValidateServiceUnitDefinition(context, sud);
            found_unit_names.emplace(unit_name);
        }
    }
    EXPECT_TRUE(found_unit_names.contains(pub_specs[0].unit_name));
    EXPECT_TRUE(found_unit_names.contains(pub_specs[1].unit_name));
    EXPECT_TRUE(found_unit_names.contains(pub_specs[2].unit_name));
}

TEST(ServiceDiscoveryManager, ListUnitsByNameNoneFound) {
    auto [context, pub_specs, publishers] = _SetupTestData("listbynamenonefound");
    auto sdm = ServiceDiscoveryManager(context);

    auto unda = UnitNameDiscoveryArgs::Create("com.legoog.sdv", "WindowManager", "bob");
    ASSERT_TRUE(unda.has_value());
    auto result = sdm.ListUnitsByName(unda.value());

    // Get back "success" with empty list.
    ASSERT_TRUE(result.has_value());
    EXPECT_TRUE(result.value().empty());
}

// ServiceChangeEventListener tests

TEST(ServiceUnitChangeListener, ListenByTypeSuccess) {
    android::ProcessState::self()->startThreadPool();
    auto context = _TestContext();
    auto pub_specs = _TestPublisherSpecs("listenbytypesuccess");

    auto [recorder, callback] = MakeEventRecordingCallback<ServiceUnitChangeEvent>();

    // First publisher created before listener
    auto pub_0 = pub_specs[0].CreatePublisherInContext(context);

    // Create listener
    auto unit_type_1 = sd::UnitType::FromString(pub_specs[0].unit_type).value();
    auto listener_result =
            ServiceUnitChangeListener::CreateByType(context, unit_type_1, std::move(callback));
    ASSERT_TRUE(listener_result.has_value());
    auto listener = std::move(listener_result.value());

    // At listener creation event generated for existing publisher.
    ASSERT_TRUE(recorder->WaitForNthEvent(1));
    auto ev0 = recorder->GetEvent(0);
    ASSERT_TRUE(ev0);
    EXPECT_EQ(ev0->event_type, ServiceUnitChangeEvent::Type::Registered);
    pub_specs[0].ValidateServiceUnitDefinition(context, ev0->unit_definition);

    // Create second publisher after listener.
    auto pub_1 = pub_specs[1].CreatePublisherInContext(context);

    // Event generated for new publisher.
    ASSERT_TRUE(recorder->WaitForNthEvent(2));
    auto ev1 = recorder->GetEvent(1);
    ASSERT_TRUE(ev1);
    EXPECT_EQ(ev1->event_type, ServiceUnitChangeEvent::Type::Registered);
    pub_specs[1].ValidateServiceUnitDefinition(context, ev1->unit_definition);

    // Create third publisher with non-matching type, no event comes.
    auto pub_2 = pub_specs[2].CreatePublisherInContext(context);
    ASSERT_FALSE(recorder->WaitForNthEvent(3));

    // Stop the listener and check that we see the terminal event.
    listener.Stop();
    ASSERT_TRUE(recorder->WaitForNthEvent(3));
    auto ev2 = recorder->GetEvent(2);
    EXPECT_FALSE(ev2);
}

// ServiceChangeEventListener tests

TEST(ServiceUnitChangeListener, ListenByNameSuccess) {
    android::ProcessState::self()->startThreadPool();
    auto context = _TestContext();
    auto pub_specs = _TestPublisherSpecs("listenbynamesuccess");

    auto [recorder, callback] = MakeEventRecordingCallback<ServiceUnitChangeEvent>();

    // First publisher created before listener
    auto pub_0 = pub_specs[0].CreatePublisherInContext(context);

    // Create listener
    auto unda = UnitNameDiscoveryArgs::Create("com.legoog.sdv", "WindowManager").value();
    auto listener_result =
            ServiceUnitChangeListener::CreateByName(context, unda, std::move(callback));
    ASSERT_TRUE(listener_result.has_value());
    auto listener = std::move(listener_result.value());

    // We may get a lot of pre-existing units from other tests; sleep briefly for all
    // of them to arrive.
    std::this_thread::sleep_for(std::chrono::seconds(2));
    size_t initial_event_count = recorder->GetEventCount();

    // May be multiple initial events, find and validate the one for pub_0.
    bool found_pub_0 = false;
    for (size_t idx = 0; idx < initial_event_count; idx++) {
        const auto& ev = recorder->GetEvent(idx);
        ASSERT_TRUE(ev);
        const auto& sud = ev->unit_definition;
        if (sud.GetUnitIdentifier().unit_name() == pub_specs[0].unit_name) {
            found_pub_0 = true;
            pub_specs[0].ValidateServiceUnitDefinition(context, sud);
        }
    }
    ASSERT_TRUE(found_pub_0);

    // Create second publisher after listener.
    auto pub_1 = pub_specs[1].CreatePublisherInContext(context);

    // Event generated for new publisher.
    ASSERT_TRUE(recorder->WaitForNthEvent(initial_event_count + 1));
    auto ev1 = recorder->GetEvent(initial_event_count);
    ASSERT_TRUE(ev1);
    EXPECT_EQ(ev1->event_type, ServiceUnitChangeEvent::Type::Registered);
    pub_specs[1].ValidateServiceUnitDefinition(context, ev1->unit_definition);

    // Stop the listener and check that we see the terminal event.
    listener.Stop();
    ASSERT_TRUE(recorder->WaitForNthEvent(initial_event_count + 2));
    auto ev2 = recorder->GetEvent(initial_event_count + 1);
    EXPECT_FALSE(ev2);
}

}  // namespace sdv_comms::sd::test
