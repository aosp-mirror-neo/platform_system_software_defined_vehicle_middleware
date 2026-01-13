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

#include <android-base/logging.h>
#include <gtest/gtest.h>
#include <sdv/ctx.h>
#include <sdv/unit_metadata.h>
#include <sdv/unit_registration.h>
#include <sdv/unit_type.h>

#include <chrono>
#include <future>
#include <memory>
#include <thread>

#include <callback_helpers.hpp>
#include <ffi_helpers.hpp>
#include <sdv/context.hpp>
#include <sdv/dt_history_reader.hpp>
#include <sdv/dt_instant_reader.hpp>
#include <sdv/dt_publisher.hpp>
#include <sdv/dt_subscriber.hpp>
#include <sdv/identity.hpp>
#include <sdv/sd_common.hpp>
#include <sdv/sd_registration.hpp>

using sdv::SdvResult;
using sdv::SdvStatus;
using sdv::SdvStatusCode;

static const char kSampleFqinStr[] = "vm:com.legoog.sdv.WindowManager/sample";
static const char kSampleUnitTypeStr[] = "com.legoog.sdv.WindowManager#Temperature";
static const char kSampleUnitName[] = "testpub";

namespace sdv_comms::dt::test {

id::ServiceFqin _MakeTestServiceFqin() {
    auto fqin_result = id::ServiceFqin::FromString(kSampleFqinStr);
    CHECK(fqin_result.has_value());
    return fqin_result.value();
}

ctx::Context _MakeTestContext() {
    // Create a Context object for
    auto fqin = _MakeTestServiceFqin();
    return ctx::Context::CreateForTest(fqin);
}

std::string _MakeTestUnitName() {
    static int i = 1;
    return std::string("testunit") + std::to_string(i++);
}

sd::UnitRegistrationArgs _MakeTestUnitRegArgs(uint32_t version, uint32_t message_size,
                                              uint32_t message_count) {
    auto ura = sd::UnitRegistrationArgs::Create();

    auto unit_name = _MakeTestUnitName();
    CHECK(ura.SetUnitName(unit_name).isOk());

    auto sample_ut = sd::UnitType::FromString(kSampleUnitTypeStr);
    CHECK(sample_ut.has_value());
    CHECK(ura.SetUnitType(sample_ut.value()).isOk());

    auto pub_md = sd::PublisherUnitMetadata::Create();
    CHECK(pub_md.SetVersion(version).isOk());
    CHECK(pub_md.SetMessageSize(message_size).isOk());
    CHECK(pub_md.SetMessageCount(message_count).isOk());
    CHECK(ura.SetPublisherUnitMetadata(pub_md).isOk());

    return ura;
}

id::UnitIdentifier _MakeUnitIdentifier(const ctx::Context& context, const std::string& unit_name) {
    auto service_id = context.GetSelfIdentity();
    CHECK(service_id.has_value());
    auto unit_id = id::UnitIdentifier::Create(service_id.value(), unit_name);
    CHECK(unit_id.has_value());
    return unit_id.value();
}

std::pair<id::UnitIdentifier, dt::Publisher> _MakeTestPublisher(const ctx::Context& context,
                                                                uint32_t version,
                                                                uint32_t message_size,
                                                                uint32_t message_count) {
    auto unit_reg_args = _MakeTestUnitRegArgs(version, message_size, message_count);

    auto service_id = context.GetSelfIdentity();
    CHECK(service_id.has_value());
    // Have to do this before we pass ownership of unit_reg_args to publisher.
    auto unit_id = id::UnitIdentifier::Create(service_id.value(), unit_reg_args.GetUnitName());
    CHECK(unit_id.has_value());

    auto pub_result = Publisher::Create(context, std::move(unit_reg_args));
    CHECK(pub_result.has_value());

    return {unit_id.value(), std::move(pub_result.value())};
}

dt::Subscriber _MakeTestSubscriber(const ctx::Context& context,
                                   const id::UnitIdentifier pub_unit_id,
                                   bool fetch_last_message = false) {
    auto sub_result =
            Subscriber::Create(context, pub_unit_id, Subscriber::CreateOptions{fetch_last_message});
    CHECK(sub_result.has_value());
    return std::move(sub_result.value());
}

dt::HistoryReader _MakeTestHistoryReader(const ctx::Context& context,
                                         const id::UnitIdentifier pub_unit_id,
                                         uint32_t history_size = 10) {
    auto hr_result = HistoryReader::Create(context, pub_unit_id, history_size);
    CHECK(hr_result.has_value());
    return std::move(hr_result.value());
}

TEST(Publisher, CreateAsyncSuccess) {
    auto context = _MakeTestContext();
    auto unit_reg_args = _MakeTestUnitRegArgs(99, 64, 20);

    // Wrapping promise in shared_ptr for copyability.
    auto result_promise = std::make_shared<std::promise<SdvResult<Publisher>>>();
    auto result_future = result_promise->get_future();
    Publisher::CreateAsync(context, std::move(unit_reg_args),
                           [result_promise](SdvResult<Publisher> result) {
                               result_promise->set_value(std::move(result));
                           });
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(5)), std::future_status::ready);
    auto result = result_future.get();
    ASSERT_TRUE(result.has_value());

    auto pub_md_out = result.value().GetMetadata();
    ASSERT_TRUE(pub_md_out.IsValid());
    EXPECT_EQ(pub_md_out.GetVersion(), 99U);
    EXPECT_EQ(pub_md_out.GetMessageSize(), 64U);
    EXPECT_EQ(pub_md_out.GetMessageCount(), 20U);
}

TEST(Publisher, CreateFutureSuccess) {
    auto context = _MakeTestContext();
    auto unit_reg_args = _MakeTestUnitRegArgs(98, 32, 40);

    auto result_future = Publisher::CreateFuture(context, std::move(unit_reg_args));
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(5)), std::future_status::ready);
    auto result = result_future.get();
    ASSERT_TRUE(result.has_value());

    auto pub_md_out = result.value().GetMetadata();
    ASSERT_TRUE(pub_md_out.IsValid());
    EXPECT_EQ(pub_md_out.GetVersion(), 98U);
    EXPECT_EQ(pub_md_out.GetMessageSize(), 32U);
    EXPECT_EQ(pub_md_out.GetMessageCount(), 40U);
}

TEST(Publisher, CreateBlockingSuccess) {
    auto context = _MakeTestContext();
    auto unit_reg_args = _MakeTestUnitRegArgs(97, 16, 80);

    auto result = Publisher::Create(context, std::move(unit_reg_args));
    ASSERT_TRUE(result.has_value());

    auto pub_md_out = result.value().GetMetadata();
    ASSERT_TRUE(pub_md_out.IsValid());
    EXPECT_EQ(pub_md_out.GetVersion(), 97U);
    EXPECT_EQ(pub_md_out.GetMessageSize(), 16U);
    EXPECT_EQ(pub_md_out.GetMessageCount(), 80U);
}

TEST(Publisher, InvalidArgumentChecks) {
    auto context = _MakeTestContext();
    auto unit_reg_args = _MakeTestUnitRegArgs(97, 16, 80);

    auto result = Publisher::Create(context, std::move(unit_reg_args));
    ASSERT_TRUE(result.has_value());
    auto& original = result.value();

    unsigned char buffer[80];
    void* a_null_ptr = nullptr;

    EXPECT_EQ(original.WriteMessages(a_null_ptr, sizeof(buffer)).error().getStatusCode(),
              SdvStatusCode::kInvalidArgument);
    EXPECT_EQ(original.WriteMessages(buffer, 0).error().getStatusCode(),
              SdvStatusCode::kInvalidArgument);

    auto dest = std::move(original);  // Move from original to render it invalid.
    EXPECT_FALSE(original.IsValid());
    EXPECT_FALSE(original.GetMetadata().IsValid());
    EXPECT_EQ(original.WriteMessages(buffer, sizeof(buffer)).error().getStatusCode(),
              SdvStatusCode::kInvalidArgument);
}

TEST(Subscriber, CreateAsyncSuccess) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 96, 16, 64);

    // Wrapping promise in shared_ptr for copyability.
    auto result_promise = new std::promise<SdvResult<Subscriber>>();
    auto result_future = result_promise->get_future();
    Subscriber::CreateAsync(context, unit_id, Subscriber::CreateOptions(),
                            [result_promise](SdvResult<Subscriber> result) {
                                result_promise->set_value(std::move(result));
                                delete result_promise;
                            });
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(5)), std::future_status::ready);
    auto result = result_future.get();
    ASSERT_TRUE(result.has_value());

    // Verify correct metadata.
    auto pub_md = result.value().GetMetadata();
    ASSERT_TRUE(pub_md.IsValid());
    // EXPECT_EQ(pub_md.GetVersion(), 96U);
    EXPECT_EQ(pub_md.GetMessageSize(), 16U);
    EXPECT_EQ(pub_md.GetMessageCount(), 64U);
}

TEST(Subscriber, CreateFutureSuccess) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 95, 64, 16);

    auto result_future = Subscriber::CreateFuture(context, unit_id, Subscriber::CreateOptions());
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(5)), std::future_status::ready);
    auto result = result_future.get();
    ASSERT_TRUE(result.has_value());

    // Verify correct metadata.
    auto pub_md = result.value().GetMetadata();
    ASSERT_TRUE(pub_md.IsValid());
    // EXPECT_EQ(pub_md.GetVersion(), 95U);
    EXPECT_EQ(pub_md.GetMessageSize(), 64U);
    EXPECT_EQ(pub_md.GetMessageCount(), 16U);
}

TEST(Subscriber, CreateBlockingSuccess) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 94, 199, 300);

    auto result = Subscriber::Create(context, unit_id, Subscriber::CreateOptions());
    ASSERT_TRUE(result.has_value());

    // Verify correct metadata.
    auto pub_md = result.value().GetMetadata();
    ASSERT_TRUE(pub_md.IsValid());
    // EXPECT_EQ(pub_md.GetVersion(), 94U);
    EXPECT_EQ(pub_md.GetMessageSize(), 199U);
    EXPECT_EQ(pub_md.GetMessageCount(), 300U);
}

TEST(Subscriber, InvalidArgumentChecks) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 94, 80, 20);

    auto result = Subscriber::Create(context, unit_id, Subscriber::CreateOptions());
    ASSERT_TRUE(result.has_value());
    auto& original = result.value();

    unsigned char buffer[80];
    void* a_null_ptr = nullptr;

    EXPECT_EQ(original.ReadMessages(a_null_ptr, sizeof(buffer)).error().getStatusCode(),
              SdvStatusCode::kInvalidArgument);
    EXPECT_EQ(original.ReadMessages(buffer, 0).error().getStatusCode(),
              SdvStatusCode::kInvalidArgument);

    auto dest = std::move(original);  // Move from original to render it invalid.
    EXPECT_FALSE(original.IsValid());
    EXPECT_FALSE(original.GetMetadata().IsValid());
    EXPECT_EQ(original.AvailableToRead().error().getStatusCode(), SdvStatusCode::kInvalidArgument);
    EXPECT_EQ(original.ReadMessages(buffer, sizeof(buffer)).error().getStatusCode(),
              SdvStatusCode::kInvalidArgument);
    EXPECT_EQ(original.SetNotificationHandler([](auto _) { return SdvStatusCode::kUnimplemented; })
                      .getStatusCode(),
              SdvStatusCode::kInvalidArgument);
    EXPECT_EQ(original.ClearNotificationHandler().getStatusCode(), SdvStatusCode::kInvalidArgument);
}

TEST(PublisherSubscriber, WriteAndReadMessages) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 94, 8, 100);
    auto sub = _MakeTestSubscriber(context, unit_id);

    uint8_t pub_buf[] = {0xd, 0xe, 0xa, 0xd, 0xb, 0xe, 0xe, 0xf};
    auto write_result = pub.WriteMessages(pub_buf, sizeof(pub_buf));
    ASSERT_TRUE(write_result.has_value());
    EXPECT_EQ(write_result.value(), 1U);

    auto available_result = sub.AvailableToRead();
    ASSERT_TRUE(available_result.has_value());
    EXPECT_EQ(available_result.value(), 1U);

    uint8_t sub_buf[] = {0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0};
    auto read_result = sub.ReadMessages(sub_buf, sizeof(sub_buf));
    ASSERT_TRUE(read_result.has_value()) << "Error from ReadMessages: " << read_result.error();
    EXPECT_EQ(read_result.value(), 1U);
    EXPECT_EQ(memcmp(pub_buf, sub_buf, sizeof(pub_buf)), 0);

    available_result = sub.AvailableToRead();
    ASSERT_TRUE(available_result.has_value());
    EXPECT_EQ(available_result.value(), 0U);
}

TEST(PublisherSubscriber, ReadWhenNothingAvailable) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 94, 8, 100);
    auto sub = _MakeTestSubscriber(context, unit_id);

    auto available_result = sub.AvailableToRead();
    ASSERT_TRUE(available_result.has_value());
    EXPECT_EQ(available_result.value(), 0U);

    uint8_t sub_buf[] = {0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0};
    auto read_result = sub.ReadMessages(sub_buf, sizeof(sub_buf));
    EXPECT_TRUE(read_result.has_value());
    EXPECT_EQ(read_result.value(), 0U);
}

TEST(PublisherSubscriber, CreateSubscriberFetchLastMessageFalse) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 93, 8, 100);

    // Do the write before we create the subscriber
    uint8_t pub_buf[] = {0xd, 0xe, 0xa, 0xd, 0xb, 0xe, 0xe, 0xf};
    ASSERT_EQ(pub.WriteMessages(pub_buf, sizeof(pub_buf)).value(), 1U);

    auto sub = _MakeTestSubscriber(context, unit_id, /* fetch_last_message */ false);

    // NOTE: This fails as AvailableToRead is returning 1.
    auto available_result = sub.AvailableToRead();
    ASSERT_TRUE(available_result.has_value());
    EXPECT_EQ(available_result.value(), 0U);
}

TEST(PublisherSubscriber, CreateSubscriberFetchLastMessageTrue) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 92, 8, 100);

    // Do two writes before we create the subscriber
    uint8_t pub_buf_1[] = {0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1, 0x1};
    auto write_result = pub.WriteMessages(pub_buf_1, sizeof(pub_buf_1));
    ASSERT_TRUE(write_result.has_value());
    EXPECT_EQ(write_result.value(), 1U);

    uint8_t pub_buf_2[] = {0x2, 0x2, 0x2, 0x2, 0x2, 0x2, 0x2, 0x2};
    write_result = pub.WriteMessages(pub_buf_2, sizeof(pub_buf_2));
    ASSERT_TRUE(write_result.has_value());
    EXPECT_EQ(write_result.value(), 1U);

    auto sub = _MakeTestSubscriber(context, unit_id, /* fetch_last_message */ true);

    // We wrote two messages but should only see one available.
    auto available_result = sub.AvailableToRead();
    ASSERT_TRUE(available_result.has_value());
    EXPECT_EQ(available_result.value(), 1U);

    // Check that the second message written is the one the subscriber sees.
    uint8_t sub_buf[] = {0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0};
    auto read_result = sub.ReadMessages(sub_buf, sizeof(sub_buf));
    ASSERT_TRUE(read_result.has_value());
    EXPECT_EQ(read_result.value(), 1U);
    EXPECT_EQ(memcmp(sub_buf, pub_buf_2, sizeof(sub_buf)), 0);
}

TEST(PublisherSubscriber, WriteTriggersNotificationHandler) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 93, 8, 100);
    auto sub = _MakeTestSubscriber(context, unit_id);

    auto [recorder, callback] = MakeEventRecordingCallback<Subscriber::NotificationType>();

    ASSERT_TRUE(sub.SetNotificationHandler(std::move(callback)).isOk());
    // We don't expect any event yet since no data has been written to the FMQ.
    ASSERT_FALSE(recorder->WaitForNthEvent(1));

    // Do a write.
    uint8_t pub_buf[] = {0xd, 0xe, 0xa, 0xd, 0xb, 0xe, 0xe, 0xf};
    ASSERT_EQ(pub.WriteMessages(pub_buf, sizeof(pub_buf)).value(), 1U);

    // Wait for the recorder to receive an event.
    ASSERT_TRUE(recorder->WaitForNthEvent(1));
    auto notice = recorder->GetEvent(0);
    EXPECT_TRUE(notice);
    EXPECT_EQ(*notice, Subscriber::NotificationType::DataAvailable);
}

TEST(PublisherSubscriber, ClearNotificationHandler) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 93, 8, 100);
    auto sub = _MakeTestSubscriber(context, unit_id);

    auto [recorder, callback] = MakeEventRecordingCallback<Subscriber::NotificationType>();
    ASSERT_TRUE(sub.SetNotificationHandler(std::move(callback)).isOk());
    // We don't expect any event yet since no data has been written to the FMQ.
    ASSERT_FALSE(recorder->WaitForNthEvent(1));

    // Do a write.
    uint8_t pub_buf[] = {0xd, 0xe, 0xa, 0xd, 0xb, 0xe, 0xe, 0xf};
    ASSERT_EQ(pub.WriteMessages(pub_buf, sizeof(pub_buf)).value(), 1U);

    ASSERT_TRUE(recorder->WaitForNthEvent(1));
    EXPECT_NE(recorder->GetEvent(0), std::nullopt);

    ASSERT_TRUE(sub.ClearNotificationHandler().isOk());

    // Expect the terminal callback that provides no value
    ASSERT_TRUE(recorder->WaitForNthEvent(2));
    EXPECT_EQ(recorder->GetEvent(1), std::nullopt);

    // Do another write, check that no event arrives.
    ASSERT_EQ(pub.WriteMessages(pub_buf, sizeof(pub_buf)).value(), 1U);
    ASSERT_FALSE(recorder->WaitForNthEvent(3));
}

TEST(PublisherSubscriber, ReplaceNotificationHandler) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 93, 8, 100);
    auto sub = _MakeTestSubscriber(context, unit_id);

    auto [recorder_1, callback_1] = MakeEventRecordingCallback<Subscriber::NotificationType>();
    ASSERT_TRUE(sub.SetNotificationHandler(std::move(callback_1)).isOk());
    // We don't expect any event yet since no data has been written to the FMQ.
    ASSERT_FALSE(recorder_1->WaitForNthEvent(1));

    // Do a write.
    uint8_t pub_buf[] = {0xd, 0xe, 0xa, 0xd, 0xb, 0xe, 0xe, 0xf};
    ASSERT_EQ(pub.WriteMessages(pub_buf, sizeof(pub_buf)).value(), 1U);

    ASSERT_TRUE(recorder_1->WaitForNthEvent(1));
    EXPECT_NE(recorder_1->GetEvent(0), std::nullopt);

    // Create and install a different callback
    auto [recorder_2, callback_2] = MakeEventRecordingCallback<Subscriber::NotificationType>();
    ASSERT_TRUE(sub.SetNotificationHandler(std::move(callback_2)).isOk());

    // First callback gets terminator
    ASSERT_TRUE(recorder_1->WaitForNthEvent(2));
    EXPECT_EQ(recorder_1->GetEvent(1), std::nullopt);

    // Do another write, check that only second callback gets event.
    ASSERT_EQ(pub.WriteMessages(pub_buf, sizeof(pub_buf)).value(), 1U);
    ASSERT_TRUE(recorder_2->WaitForNthEvent(1));
    ASSERT_FALSE(recorder_1->WaitForNthEvent(3));
}

TEST(PublisherSubscriber, NotificationHandlerTerminatesOnSubscriberDestruction) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 93, 8, 100);

    auto [recorder, callback] = MakeEventRecordingCallback<Subscriber::NotificationType>();

    {  // Scope to control sub lifetime
        auto sub = _MakeTestSubscriber(context, unit_id);
        ASSERT_TRUE(sub.SetNotificationHandler(std::move(callback)).isOk());
    }

    ASSERT_TRUE(recorder->WaitForNthEvent(0));
    EXPECT_EQ(recorder->GetEvent(0), std::nullopt);
}

TEST(PublisherSubscriber, DISABLED_NotificationHandlerTerminatesOnPublisherDestruction) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 93, 8, 100);
    // Put the Subscriber in an optional so we can .reset() to destroy it.
    auto sub = _MakeTestSubscriber(context, unit_id);

    auto [recorder, callback] = MakeEventRecordingCallback<Subscriber::NotificationType>();
    ASSERT_TRUE(sub.SetNotificationHandler(std::move(callback)).isOk());

    {
        // Move pub into another instance that will get destroyed at the end of this scope.
        Publisher doomed_pub = std::move(pub);
    }

    ASSERT_TRUE(recorder->WaitForNthEvent(0));
    EXPECT_EQ(recorder->GetEvent(0), std::nullopt);
}

TEST(HistoryReader, CreateAsyncSuccess) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 96, 16, 64);

    // Wrapping promise in shared_ptr for copyability.
    auto result_promise = new std::promise<SdvResult<HistoryReader>>();
    auto result_future = result_promise->get_future();
    HistoryReader::CreateAsync(context, unit_id,
                               /* history_size = */ 10,
                               [result_promise](SdvResult<HistoryReader> result) {
                                   result_promise->set_value(std::move(result));
                                   delete result_promise;
                               });
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(5)), std::future_status::ready);
    auto result = result_future.get();
    ASSERT_TRUE(result.has_value());

    // Verify correct metadata.
    auto pub_md = result.value().GetMetadata();
    ASSERT_TRUE(pub_md.IsValid());
    // EXPECT_EQ(pub_md.GetVersion(), 96U);
    EXPECT_EQ(pub_md.GetMessageSize(), 16U);
    EXPECT_EQ(pub_md.GetMessageCount(), 64U);
}

TEST(HistoryReader, CreateFutureSuccess) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 95, 64, 16);

    auto result_future = HistoryReader::CreateFuture(context, unit_id, /* history_size = */ 10);
    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(5)), std::future_status::ready);
    auto result = result_future.get();
    ASSERT_TRUE(result.has_value());

    // Verify correct metadata.
    auto pub_md = result.value().GetMetadata();
    ASSERT_TRUE(pub_md.IsValid());
    // EXPECT_EQ(pub_md.GetVersion(), 95U);
    EXPECT_EQ(pub_md.GetMessageSize(), 64U);
    EXPECT_EQ(pub_md.GetMessageCount(), 16U);
}

TEST(HistoryReader, CreateBlockingSuccess) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 94, 199, 300);

    auto result = HistoryReader::Create(context, unit_id, /* history_size = */ 10);
    ASSERT_TRUE(result.has_value());

    // Verify correct metadata.
    auto pub_md = result.value().GetMetadata();
    ASSERT_TRUE(pub_md.IsValid());
    // EXPECT_EQ(pub_md.GetVersion(), 94U);
    EXPECT_EQ(pub_md.GetMessageSize(), 199U);
    EXPECT_EQ(pub_md.GetMessageCount(), 300U);
}

TEST(HistoryReader, InvalidArgumentChecks) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 94, 80, 20);

    auto result = HistoryReader::Create(context, unit_id, /* history_size = */ 10);
    ASSERT_TRUE(result.has_value());
    auto& original = result.value();

    unsigned char buffer[80];
    uint8_t* a_null_ptr = nullptr;

    EXPECT_EQ(original.ReadLastMessages(a_null_ptr, sizeof(buffer)).error().getStatusCode(),
              SdvStatusCode::kInvalidArgument);
    EXPECT_EQ(original.ReadLastMessages(buffer, 0).error().getStatusCode(),
              SdvStatusCode::kInvalidArgument);

    auto dest = std::move(original);  // Move from original to render it invalid.
    EXPECT_FALSE(original.IsValid());
    EXPECT_FALSE(original.GetMetadata().IsValid());
    EXPECT_EQ(original.AvailableToRead().error().getStatusCode(), SdvStatusCode::kInvalidArgument);
    EXPECT_EQ(original.ReadLastMessages(buffer, sizeof(buffer)).error().getStatusCode(),
              SdvStatusCode::kInvalidArgument);
}

TEST(PublisherHistoryReader, WriteAndReadMessages) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 94, 8, 100);
    auto hr = _MakeTestHistoryReader(context, unit_id, /* history_size = */ 3);

    uint8_t pub_buf[] = {0xd, 0xe, 0xa, 0xd, 0xb, 0xe, 0xe, 0xf};
    auto write_result = pub.WriteMessages(pub_buf, sizeof(pub_buf));
    ASSERT_TRUE(write_result.has_value());
    EXPECT_EQ(write_result.value(), 1U);

    // Write may not be instantly visible to HistoryReader;
    // retry in a loop for up to 5 seconds.
    bool data_available = false;
    auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(5);
    for (; !data_available && (std::chrono::steady_clock::now() < deadline);
         std::this_thread::sleep_for(std::chrono::milliseconds(100))) {
        auto available_result = hr.AvailableToRead();
        ASSERT_TRUE(available_result.has_value());
        data_available = (available_result.value() == 1U);
    }
    ASSERT_TRUE(data_available);

    uint8_t read_buf[] = {0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0};
    auto read_result = hr.ReadLastMessages(read_buf, sizeof(read_buf));
    ASSERT_TRUE(read_result.has_value()) << "Error from ReadLastMessages: " << read_result.error();
    EXPECT_EQ(read_result.value(), 1U);
    EXPECT_EQ(memcmp(pub_buf, read_buf, sizeof(pub_buf)), 0);

    auto available_result = hr.AvailableToRead();
    ASSERT_TRUE(available_result.has_value());
    EXPECT_EQ(available_result.value(), 1U);
}

TEST(PublisherHistoryReader, ReadWhenNothingAvailable) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 94, 8, 100);
    auto sub = _MakeTestSubscriber(context, unit_id);

    auto available_result = sub.AvailableToRead();
    ASSERT_TRUE(available_result.has_value());
    EXPECT_EQ(available_result.value(), 0U);

    uint8_t sub_buf[] = {0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0};
    auto read_result = sub.ReadMessages(sub_buf, sizeof(sub_buf));
    EXPECT_TRUE(read_result.has_value());
    EXPECT_EQ(read_result.value(), 0U);
}

TEST(InstantReader, GetLastMessageMessageAsync) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 94, 8, 100);

    uint8_t pub_buf[] = {0xd, 0xe, 0xa, 0xd, 0xb, 0xe, 0xe, 0xf};
    auto write_result = pub.WriteMessages(pub_buf, sizeof(pub_buf));
    ASSERT_TRUE(write_result.has_value());
    EXPECT_EQ(write_result.value(), 1U);

    auto result_promise = std::make_shared<std::promise<InstantReader::Result>>();
    auto result_future = result_promise->get_future();
    InstantReader::GetLastMessageAsync(context, unit_id,
                                       [result_promise](InstantReader::Result result) {
                                           result_promise->set_value(std::move(result));
                                       });

    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(5)), std::future_status::ready);
    auto result = result_future.get();
    ASSERT_TRUE(result.has_value());
    auto message = result.value();
    ASSERT_EQ(message.size(), sizeof(pub_buf));
    EXPECT_EQ(memcmp(pub_buf, message.data(), sizeof(pub_buf)), 0);
}

TEST(InstantReader, GetLastMessageMessageFuture) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 94, 8, 100);

    uint8_t pub_buf[] = {0xd, 0xe, 0xa, 0xd, 0xb, 0xe, 0xe, 0xf};
    auto write_result = pub.WriteMessages(pub_buf, sizeof(pub_buf));
    ASSERT_TRUE(write_result.has_value());
    EXPECT_EQ(write_result.value(), 1U);

    auto result_future = InstantReader::GetLastMessageFuture(context, unit_id);

    ASSERT_EQ(result_future.wait_for(std::chrono::seconds(5)), std::future_status::ready);
    auto result = result_future.get();
    ASSERT_TRUE(result.has_value());
    auto message = result.value();
    ASSERT_EQ(message.size(), sizeof(pub_buf));
    EXPECT_EQ(memcmp(pub_buf, message.data(), sizeof(pub_buf)), 0);
}

TEST(PublisherInstantReader, GetLastMessageMessageBlocking) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 94, 8, 100);

    uint8_t pub_buf[] = {0xd, 0xe, 0xa, 0xd, 0xb, 0xe, 0xe, 0xf};
    auto write_result = pub.WriteMessages(pub_buf, sizeof(pub_buf));
    ASSERT_TRUE(write_result.has_value());
    EXPECT_EQ(write_result.value(), 1U);

    auto read_result = InstantReader::GetLastMessage(context, unit_id);
    ASSERT_TRUE(read_result.has_value());
    auto read_message = read_result.value();
    ASSERT_EQ(read_message.size(), sizeof(pub_buf));
    EXPECT_EQ(memcmp(pub_buf, read_message.data(), sizeof(pub_buf)), 0);
}

TEST(PublisherInstantReader, ReadWhenNothingAvailable) {
    auto context = _MakeTestContext();
    auto [unit_id, pub] = _MakeTestPublisher(context, 94, 8, 100);

    auto read_result = InstantReader::GetLastMessage(context, unit_id);
    ASSERT_TRUE(read_result.has_value());
    auto read_message = read_result.value();
    ASSERT_EQ(read_message.size(), 0U);
}

}  // namespace sdv_comms::dt::test
