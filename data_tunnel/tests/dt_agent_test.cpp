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

#include "service.h"

#include <aidl/google/sdv/data_tunnel/BnSubscriptionReadinessListener.h>
#include <aidl/google/sdv/data_tunnel/IAgentService.h>
#include <aidl/google/sdv/data_tunnel/SubscriptionParams.h>
#include <aidl/google/sdv/data_tunnel/SubscriptionResult.h>
#include <aidl/google/sdv/identity/IIdentityAgent.h>
#include <aidl/google/sdv/service_discovery/discovery/IServiceDiscoveryAgent.h>
#include <aidl/google/sdv/service_discovery/discovery/IServiceRegistrationAgent.h>
#include <aidl/google/sdv/service_discovery/discovery/ITransportSupportAgent.h>
#include <android-base/logging.h>
#include <android/binder_ibinder_platform.h>
#include <android/binder_libbinder.h>
#include <android/binder_manager.h>
#include <android/binder_process.h>
#include <fmq/AidlMessageQueue.h>
#include <gtest/gtest.h>

#include <libsdv_dt_core.h>

#include <future>
#include <map>
#include <memory>
#include <mutex>
#include <random>

#include <sdv/status.hpp>

namespace android::sdv::dt::agent::test {

using IdentityAgent = ::aidl::google::sdv::identity::IIdentityAgent;
using ServiceFqin = ::aidl::google::sdv::identity::ServiceFqin;
using ServiceIdentity = ::aidl::google::sdv::identity::ServiceIdentity;
using UnitIdentifier = ::aidl::google::sdv::identity::UnitIdentifier;

using AccessControl = ::aidl::google::sdv::service_discovery::common::AccessControl;
using ApplicationMetadata =
        ::aidl::google::sdv::service_discovery::common::metadata::ApplicationMetadata;
using PublisherUnitMetadata =
        ::aidl::google::sdv::service_discovery::common::metadata::PublisherUnitMetadata;
using RegistrationToken = ::aidl::google::sdv::service_discovery::common::RegistrationToken;
using ServiceDiscoveryAgent =
        ::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent;
using ServiceRegistrationAgent =
        ::aidl::google::sdv::service_discovery::discovery::IServiceRegistrationAgent;
using ServiceUnitDefinition =
        ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition;
using TransportSupportAgent =
        ::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent;
using UnitType = ::aidl::google::sdv::service_discovery::common::UnitType;

using DataTunnelAgent = ::aidl::google::sdv::data_tunnel::IAgentService;
using PublicationDescriptor = ::aidl::google::sdv::data_tunnel::PublicationDescriptor;

using ::aidl::android::hardware::common::fmq::MQDescriptor;
using ::aidl::android::hardware::common::fmq::UnsynchronizedWrite;
using ::android::AidlMessageQueue;

using AidlMQ = AidlMessageQueue<int8_t, UnsynchronizedWrite>;
using AidlMQDescriptor = MQDescriptor<int8_t, UnsynchronizedWrite>;

using core::CommChannel;

template <class T>
static std::shared_ptr<T> GetService() {
    static auto ptr = []() {
        std::string service_name = std::string(T::descriptor) + "/default";
        ndk::SpAIBinder service(AServiceManager_waitForService(service_name.c_str()));
        return T::fromBinder(service);
    }();

    return ptr;
}

// Sample Binder Class
class SampleBinderData {
    SampleBinderData() {
        binder_ = AIBinder_new(SampleBinderData::kClass, static_cast<void*>(this));
    }

    AIBinder* binder_ = nullptr;

    static SampleBinderData& Inst() {
        static SampleBinderData inst;
        return inst;
    }

public:
    SampleBinderData(const SampleBinderData&) = delete;
    SampleBinderData& operator=(SampleBinderData&) = delete;

    static const std::string kClassDescriptor;
    static const AIBinder_Class* kClass;

    static AIBinder* GetProcessAIBinder() { return Inst().binder_; }
    static void* OnCreate(void* args) { return args; }
    static void OnDestroy(void* /*userData*/) { /* no need to delete */ }

    static binder_status_t OnTransact(AIBinder* /*binder*/, transaction_code_t /*code*/,
                                      const AParcel* /*in*/, AParcel* /*out*/) {
        return STATUS_OK;
    }
};

const std::string SampleBinderData::kClassDescriptor = "CommStackDataTunnelAgentTest";
const AIBinder_Class* SampleBinderData::kClass =
        AIBinder_Class_define(SampleBinderData::kClassDescriptor.c_str(),
                              SampleBinderData::OnCreate, SampleBinderData::OnDestroy,
                              SampleBinderData::OnTransact);

class DataTunnel_IAgentService_AIDL_Test : public testing::Test {
protected:
    DataTunnel_IAgentService_AIDL_Test() {}
};

static const ServiceIdentity::EcPublicKey kPublicKey =
        ServiceIdentity::EcPublicKey{{0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5,
                                      6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1}};

static const ServiceFqin kFqin = ServiceFqin{
        /*sdvVmName*/ "",
        /*sdvPackageName*/ "com.android.sdv.data_tunnel.agent_test",
        /*serviceBundleName*/ "DataTunnelAgentTest",
        /*serviceInstanceName*/ "default",
};

// ReadCallback will increment counters of messages seen, broken out by publication ID and message
// type.  This struct defines a type to serve as key in the map of counters.
struct CounterKey {
    core::TopicId publication_id;
    CommChannel::Message::Type type;

    CounterKey(const CommChannel::Message& msg) :
          publication_id(static_cast<core::TopicId>(msg.args_[0])), type(msg.type()) {}
    CounterKey(core::TopicId pub_id, CommChannel::Message::Type type) :
          publication_id(pub_id), type(type) {}

    bool operator<(const CounterKey& o) const {
        if (publication_id == o.publication_id) {
            return type < o.type;
        }
        return publication_id < o.publication_id;
    }
};

// Although DTAgent can accept a second Connect() from the same process, IdentityAgent does not
// support create a second identity for a single process. Thus, all tests in this file share the
// same IdentityAgent session. While the data tunnel agent session can be reset by calling
// ConnectToDataTunnelAgent multiple times.
struct DtAgentTestState {
    std::shared_ptr<DataTunnelAgent> dt_agent;
    ndk::SpAIBinder link_to_death;
    ServiceIdentity service_identity;
    std::unique_ptr<CommChannel> comm_channel;

    // Returns a copy of the counters.
    std::map<CounterKey, int> GetCounters() {
        std::lock_guard<std::mutex> counters_guard(msg_counters_mutex_);
        return msg_counters;
    }

    void ClearCounters() {
        std::lock_guard<std::mutex> counters_guard(msg_counters_mutex_);
        return msg_counters.clear();
    }

    bool IncrementCounter(const CounterKey& key) {
        std::lock_guard<std::mutex> counters_guard(msg_counters_mutex_);
        return msg_counters[key]++;
    }

private:
    std::mutex msg_counters_mutex_;
    std::map<CounterKey, int> msg_counters;
};

static void ReadCallback(const CommChannel::Message& msg, void* state_ptr, int32_t) {
    auto& state = *static_cast<DtAgentTestState*>(state_ptr);
    state.IncrementCounter(CounterKey(msg));
}

std::unique_ptr<DtAgentTestState> InitializeDtAgentTestState() {
    auto result = std::make_unique<DtAgentTestState>();

    // Start up test instance of DT Agent
    result->dt_agent = StartTestService();
    CHECK(result->dt_agent);

    auto id_agent = GetService<IdentityAgent>();
    CHECK(id_agent);
    result->link_to_death.set(SampleBinderData::GetProcessAIBinder());
    auto call_status = id_agent->createIdentity(kPublicKey, kFqin, result->link_to_death,
                                                &result->service_identity);
    CHECK(call_status.isOk()) << "IdentityAgent::createIdentity() failed! "
                              << call_status.getDescription();

    ::aidl::google::sdv::data_tunnel::ClientDescriptor client_descriptor;
    call_status = result->dt_agent->Connect(result->service_identity.sid, &client_descriptor);
    CHECK(call_status.isOk()) << "IAgentService::Connect() failed! "
                              << call_status.getDescription();

    // Build CommChannel and verify it's in proper state.
    int fd_read = client_descriptor.comm_pipe_handle.fds[0].release();
    int fd_write = client_descriptor.comm_pipe_handle.fds[1].release();
    result->comm_channel =
            CommChannel::FromPipeFds(fd_read, fd_write, ReadCallback, result.get(), 0);
    CHECK(result->comm_channel->IsConnected()) << "Comm channel is in a bad state";

    return result;
}

DtAgentTestState& GetDtAgentTestState() {
    static std::unique_ptr<DtAgentTestState> global_state = InitializeDtAgentTestState();
    return *global_state;
}

struct SubscriptionReadinessListener :
      public aidl::google::sdv::data_tunnel::BnSubscriptionReadinessListener {
    ndk::ScopedAStatus onSubscriptionReady(int32_t status_code) override {
        result_promise.set_value(status_code);
        return ndk::ScopedAStatus::ok();
    }

    std::promise<int32_t> result_promise;
};

TEST_F(DataTunnel_IAgentService_AIDL_Test, ConnectTwiceWithExistingClientAlive_Fail) {
    auto& state = GetDtAgentTestState();

    ::aidl::google::sdv::data_tunnel::ClientDescriptor client_descriptor;
    auto call_status = state.dt_agent->Connect(state.service_identity.sid, &client_descriptor);

    ASSERT_FALSE(call_status.isOk());
}

TEST_F(DataTunnel_IAgentService_AIDL_Test, ConnectTwiceWithExistingClientNotAlive_Success) {
    auto& state = GetDtAgentTestState();

    // Simulate the case when the CommChannel is closed by the client.
    state.comm_channel.reset();

    // Sleep briefly to give DT Agent time to process disconnect
    sleep(3);

    ::aidl::google::sdv::data_tunnel::ClientDescriptor client_descriptor;
    auto call_status = state.dt_agent->Connect(state.service_identity.sid, &client_descriptor);

    ASSERT_TRUE(call_status.isOk()) << call_status.getDescription();

    // Restore CommChannel for other tests.
    int fd_read = client_descriptor.comm_pipe_handle.fds[0].release();
    int fd_write = client_descriptor.comm_pipe_handle.fds[1].release();
    state.comm_channel = CommChannel::FromPipeFds(fd_read, fd_write, ReadCallback, &state, 0);
    CHECK(state.comm_channel->IsConnected()) << "Comm channel is in a bad state";
}

TEST_F(DataTunnel_IAgentService_AIDL_Test, Essentials) {
    auto sd_agent = GetService<ServiceDiscoveryAgent>();
    ASSERT_NE(sd_agent, nullptr);
    auto sr_agent = GetService<ServiceRegistrationAgent>();
    ASSERT_NE(sr_agent, nullptr);

    /*
     * Create identity and connect to DT agent
     */
    auto& state = GetDtAgentTestState();

    /*
     *  Prepare buffers
     */
    constexpr size_t message_size = 128;
    constexpr size_t message_count = 32;

    int8_t pub_buff[message_size];
    int8_t sub_buff[message_size];
    memset(pub_buff, 0xCC, message_size);
    memset(sub_buff, 0xAA, message_size);

    /*
     * Register Service Unit
     */
    const char* service_unit_name = "data-tunnel-agent-test";
    auto unit_type = UnitType{/*sdvPackageName*/ kFqin.sdvPackageName,
                              /*serviceBundleName*/ kFqin.serviceBundleName,
                              /*typeName*/ "Default"};

    auto access_control = AccessControl{};
    auto app_metadata =
            ApplicationMetadata{/*typedMetadata*/ ApplicationMetadata::TypedMetadata{
                                        PublisherUnitMetadata{/*version*/ 1, message_size,
                                                              message_count}},
                                /*valueHolder*/ {0, 1, 2, 3, 4}};

    RegistrationToken registration_token;
    auto call_status = sr_agent->registerServiceUnit(service_unit_name, unit_type, access_control,
                                                     app_metadata, &registration_token);
    ASSERT_TRUE(call_status.isOk()) << "IServiceRegistrationAgent::registerServiceUnit() failed! "
                                    << call_status.getDescription();

    /*
     * Create an FMQ instance for the publisher
     */
    auto pub_mq = std::make_unique<AidlMQ>(message_size * message_count);
    ASSERT_TRUE(pub_mq->isValid()) << "Unable to create FMQ for publisher!";

    // Get the descriptor and seal future mappings against writing even before
    // sending it to DT Agent.
    auto pub_mq_desc = pub_mq->dupeDesc();
    CHECK(ashmem_set_prot_region(pub_mq_desc.handle.fds[0].get(), PROT_READ) == 0)
            << "Error from ashmem_set_prot_region: " << errno;

    /*
     * Create a publication (associated with the newly registered Service Unit)
     */
    PublicationDescriptor publication_descriptor_publ;
    call_status = state.dt_agent->RegisterPublication(registration_token, pub_mq_desc,
                                                      &publication_descriptor_publ);
    ASSERT_TRUE(call_status.isOk())
            << "IAgentService::RegisterPublication() failed! " << call_status.getDescription();

    /*
     * Validate if Publisher's publication descriptor is configured properly
     */
    ASSERT_EQ(static_cast<int>(message_size), publication_descriptor_publ.message_size)
            << "Message size value returned by IAgentService::RegisterPublication() is incorrect!";
    ASSERT_EQ(static_cast<int>(message_count), publication_descriptor_publ.message_count)
            << "Message count value returned by IAgentService::RegisterPublication() is incorrect!";

    /*
     * Use Service Discovery to lookup for the test service using it's fqin and service name
     */
    std::optional<ServiceUnitDefinition> service_unit_definition;
    call_status = sd_agent->getServiceUnit(state.service_identity.name, service_unit_name,
                                           &service_unit_definition);
    ASSERT_TRUE(call_status.isOk())
            << "IServiceDiscovery::getServiceUnit() failed! " << call_status.getDescription();

    ASSERT_TRUE(service_unit_definition.has_value())
            << "IServiceDiscovery::getServiceUnit() returned nothing!";

    /*
     * Subscribe to the newly created publication
     */
    auto readiness_listener = ndk::SharedRefBase::make<SubscriptionReadinessListener>();
    auto unit_identifier = UnitIdentifier{/*identity*/ service_unit_definition->identity,
                                          /*unit_name*/ service_unit_name};

    aidl::google::sdv::data_tunnel::SubscriptionParams sub_params{
            .unit_identifier = unit_identifier,
            .readiness_listener = readiness_listener,
    };
    aidl::google::sdv::data_tunnel::SubscriptionResult sub_result;
    call_status = state.dt_agent->SubscribeToPublicationExtended(sub_params, &sub_result);
    ASSERT_TRUE(call_status.isOk()) << "IAgentService::SubscribeToPublicationExtended() failed! "
                                    << call_status.getDescription();

    auto ready_future = readiness_listener->result_promise.get_future();
    ASSERT_EQ(ready_future.wait_for(std::chrono::seconds(1)), std::future_status::ready);
    ASSERT_EQ(ready_future.get(), static_cast<int32_t>(::sdv::SdvStatusCode::kOk));

    /*
     * Validate if Subscriber's publication descriptor is configured properly
     */
    ASSERT_EQ(static_cast<int>(message_size), sub_result.publication_descriptor.message_size)
            << "Message size value returned by IAgentService::SubscribeToPublicationExtended() is "
               "incorrect!";
    ASSERT_EQ(static_cast<int>(message_count), sub_result.publication_descriptor.message_count)
            << "Message count value returned by IAgentService::SubscribeToPublicationExtended() is "
               "incorrect!";

    /*
     * Create an FMQ instance for the subscriber
     */
    auto sub_mq = std::make_unique<AidlMQ>(sub_result.publication_descriptor.mq_desc, false);
    ASSERT_TRUE(sub_mq->isValid()) << "Unable to create FMQ for from a publisher's descriptor !";

    /*
     * Write to the FMQ buffer
     */
    ASSERT_TRUE(pub_mq->write(pub_buff, message_size))
            << "Unable to write into publisher's FMQ buffer!";

    /*
     * Read from the FMQ buffer
     */
    ASSERT_EQ(sub_mq->availableToRead(), message_size)
            << "availableToRead() returns unexpected value!";
    ASSERT_TRUE(sub_mq->read(sub_buff, message_size))
            << "Unable to read from subscriber's FMQ buffer!";
    // Validate the result
    ASSERT_EQ(memcmp(pub_buff, sub_buff, message_size), 0)
            << "The received messages isn't the same as the published one!";

    /*
     * Subscribe using "old" SubscribeToPublication
     */
    auto publication_descriptor_subs = PublicationDescriptor();
    call_status =
            state.dt_agent->SubscribeToPublication(unit_identifier, &publication_descriptor_subs);
    ASSERT_TRUE(call_status.isOk())
            << "IAgentService::SubscribeToPublication() failed! " << call_status.getDescription();

    /*
     * Validate if Subscriber's publication descriptor is configured properly
     */
    ASSERT_EQ(static_cast<int>(message_size), publication_descriptor_subs.message_size)
            << "Message size value returned by IAgentService::SubscribeToPublication() is "
               "incorrect!";
    ASSERT_EQ(static_cast<int>(message_count), publication_descriptor_subs.message_count)
            << "Message count value returned by IAgentService::SubscribeToPublication() is "
               "incorrect!";

    /*
     * Create an FMQ instance for the subscriber
     */
    sub_mq = std::make_unique<AidlMQ>(publication_descriptor_subs.mq_desc, false);
    ASSERT_TRUE(sub_mq->isValid()) << "Unable to create FMQ for from a publisher's descriptor !";

    /*
     * Read from the FMQ buffer -- same data already published
     */
    ASSERT_EQ(sub_mq->availableToRead(), message_size)
            << "availableToRead() returns unexpected value!";
    ASSERT_TRUE(sub_mq->read(sub_buff, message_size))
            << "Unable to read from subscriber's FMQ buffer!";
    // Validate the result
    ASSERT_EQ(memcmp(pub_buff, sub_buff, message_size), 0)
            << "The received messages isn't the same as the published one!";
}

TEST_F(DataTunnel_IAgentService_AIDL_Test, RegisterPublicationWithEmptyMQDescriptor) {
    auto sr_agent = GetService<ServiceRegistrationAgent>();
    ASSERT_NE(sr_agent, nullptr);

    /*
     * Create identity and connect to DT agent
     */
    auto& state = GetDtAgentTestState();

    /*
     * Register Service Unit
     */
    const char* service_unit_name = "dt-agent-invalid-mq-desc-1";
    auto unit_type = UnitType{/*sdvPackageName*/ kFqin.sdvPackageName,
                              /*serviceBundleName*/ kFqin.serviceBundleName,
                              /*typeName*/ "Default"};

    constexpr size_t message_size = 128;
    constexpr size_t message_count = 32;
    auto access_control = AccessControl{};
    auto app_metadata =
            ApplicationMetadata{/*typedMetadata*/ ApplicationMetadata::TypedMetadata{
                                        PublisherUnitMetadata{/*version*/ 1, message_size,
                                                              message_count}},
                                /*valueHolder*/ {0, 1, 2, 3, 4}};

    RegistrationToken registration_token;
    auto call_status = sr_agent->registerServiceUnit(service_unit_name, unit_type, access_control,
                                                     app_metadata, &registration_token);
    ASSERT_TRUE(call_status.isOk()) << "IServiceRegistrationAgent::registerServiceUnit() failed! "
                                    << call_status.getDescription();

    // Call RegisterPublication with an empty descriptor
    PublicationDescriptor publication_descriptor_publ;
    call_status = state.dt_agent->RegisterPublication(registration_token, AidlMQDescriptor(),
                                                      &publication_descriptor_publ);
    ASSERT_FALSE(call_status.isOk())
            << "IAgentService::RegisterPublication() should not have succeeded! "
            << call_status.getDescription();
    ASSERT_EQ(call_status.getExceptionCode(), EX_ILLEGAL_ARGUMENT);
}

TEST_F(DataTunnel_IAgentService_AIDL_Test, RegisterPublicationWithInvalidMQDescriptor) {
    auto sr_agent = GetService<ServiceRegistrationAgent>();
    ASSERT_NE(sr_agent, nullptr);

    /*
     * Create identity and connect to DT agent
     */
    auto& state = GetDtAgentTestState();

    /*
     * Register Service Unit
     */
    const char* service_unit_name = "dt-agent-invalid-mq-desc-2";
    auto unit_type = UnitType{/*sdvPackageName*/ kFqin.sdvPackageName,
                              /*serviceBundleName*/ kFqin.serviceBundleName,
                              /*typeName*/ "Default"};

    constexpr size_t message_size = 128;
    constexpr size_t message_count = 32;
    auto access_control = AccessControl{};
    auto app_metadata =
            ApplicationMetadata{/*typedMetadata*/ ApplicationMetadata::TypedMetadata{
                                        PublisherUnitMetadata{/*version*/ 1, message_size,
                                                              message_count}},
                                /*valueHolder*/ {0, 1, 2, 3, 4}};

    RegistrationToken registration_token;
    auto call_status = sr_agent->registerServiceUnit(service_unit_name, unit_type, access_control,
                                                     app_metadata, &registration_token);
    ASSERT_TRUE(call_status.isOk()) << "IServiceRegistrationAgent::registerServiceUnit() failed! "
                                    << call_status.getDescription();

    // Call RegisterPublication with an empty descriptor
    PublicationDescriptor publication_descriptor_publ;
    auto mq_desc = AidlMQDescriptor();
    mq_desc.handle.fds.emplace_back();  // Empty ScopedFileDescriptor
    call_status = state.dt_agent->RegisterPublication(registration_token, AidlMQDescriptor(),
                                                      &publication_descriptor_publ);
    ASSERT_FALSE(call_status.isOk())
            << "IAgentService::RegisterPublication() should not have succeeded! "
            << call_status.getDescription();
    ASSERT_EQ(call_status.getExceptionCode(), EX_ILLEGAL_ARGUMENT);
}

PublicationDescriptor MakePublication(std::string service_unit_name,
                                      std::unique_ptr<AidlMQ>* mq_out = nullptr) {
    constexpr size_t message_size = 128;
    constexpr size_t message_count = 32;
    auto unit_type = UnitType{/*sdvPackageName*/ kFqin.sdvPackageName,
                              /*serviceBundleName*/ kFqin.serviceBundleName,
                              /*typeName*/ "Default"};

    auto access_control = AccessControl{};
    auto app_metadata =
            ApplicationMetadata{/*typedMetadata*/ ApplicationMetadata::TypedMetadata{
                                        PublisherUnitMetadata{/*version*/ 1, message_size,
                                                              message_count}},
                                /*valueHolder*/ {0, 1, 2, 3, 4}};

    auto sr_agent = GetService<ServiceRegistrationAgent>();
    CHECK(sr_agent != nullptr);

    RegistrationToken registration_token;
    auto call_status = sr_agent->registerServiceUnit(service_unit_name, unit_type, access_control,
                                                     app_metadata, &registration_token);
    CHECK(call_status.isOk()) << "IServiceRegistrationAgent::registerServiceUnit() failed!"
                              << call_status.getDescription();
    auto dt_agent = GetDtAgentTestState().dt_agent;

    // Create FMQ instance.
    auto pub_mq = std::make_unique<AidlMQ>(message_size * message_count);
    CHECK(pub_mq->isValid()) << "Unable to create FMQ for publisher!";

    // Register publication, passing in FMQ descriptor.
    PublicationDescriptor publication_descriptor;
    call_status = dt_agent->RegisterPublication(registration_token, pub_mq->dupeDesc(),
                                                &publication_descriptor);
    CHECK(call_status.isOk()) << "IAgentService::RegisterPublication() failed! "
                              << call_status.getDescription();
    if (mq_out) {
        *mq_out = std::move(pub_mq);
    }
    return publication_descriptor;
}

PublicationDescriptor MakeSubscriber(const ServiceIdentity& service_identity,
                                     std::string service_unit_name) {
    auto unit_identifier = UnitIdentifier{/*identity*/ service_identity,
                                          /*unit_name*/ service_unit_name};
    auto dt_agent = GetDtAgentTestState().dt_agent;
    PublicationDescriptor publication_descriptor;
    auto call_status = dt_agent->SubscribeToPublication(unit_identifier, &publication_descriptor);
    CHECK(call_status.isOk()) << "IAgentService::SubscribeToPublication() failed! "
                              << call_status.getDescription();
    return publication_descriptor;
}

TEST_F(DataTunnel_IAgentService_AIDL_Test, NotificationState) {
    using Message = CommChannel::Message;
    using Message::Type::DataAvailable;
    using Message::Type::Exception;

    // How long to wait for messages to propagate through DT Agent and
    // back to the CommChannel after an event that may have generated some.
    static const auto kPropagationWait = std::chrono::seconds(1);

    /*
     * Get identity and DT Agent connection
     */
    auto& state = GetDtAgentTestState();

    auto& comm_channel = *state.comm_channel;

    // Make first publication
    std::unique_ptr<AidlMQ> pub1_mq;
    auto pub1 = MakePublication("pub-one", &pub1_mq);
    core::TopicId pub1_id = static_cast<core::TopicId>(pub1.publication_id);
    auto pub1_data_available = Message::DataAvailableMessage(pub1_id, 1);

    // Go ahead and write a message to the MQ for the first publication.
    int8_t message_buffer[128];
    pub1_mq->write(message_buffer, sizeof(message_buffer));

    // Make second publication
    auto pub2 = MakePublication("pub-two");
    core::TopicId pub2_id = static_cast<core::TopicId>(pub2.publication_id);
    auto pub2_data_available = Message::DataAvailableMessage(pub2_id, 1);

    // Make subscriber to first publication
    auto sub1 = MakeSubscriber(state.service_identity, "pub-one");

    // Make subscriber to second publication
    auto sub2 = MakeSubscriber(state.service_identity, "pub-two");

    // Send initial notifications and verify nothing comes through.
    state.ClearCounters();  // Reset all counters
    comm_channel.SendMessage(pub1_data_available);
    comm_channel.SendMessage(pub2_data_available);
    std::this_thread::sleep_for(kPropagationWait);
    ASSERT_EQ(state.GetCounters().size(), 0ul);  // Should have received no messages.

    // Enable notifications for pub-one
    comm_channel.SendMessage(Message::SetNotificationStateMessage(pub1_id, true));
    std::this_thread::sleep_for(kPropagationWait);
    {
        // We get an initial DataAvailable notification because data was already present.
        auto counters = state.GetCounters();
        ASSERT_EQ(counters.size(), 1ul);
        ASSERT_EQ(counters.at(CounterKey(pub1_id, DataAvailable)), 1);
    }

    // Send availability messages for both pubs again
    state.ClearCounters();  // Reset all counters
    comm_channel.SendMessage(pub1_data_available);
    comm_channel.SendMessage(pub2_data_available);
    std::this_thread::sleep_for(kPropagationWait);
    {
        // Only first pub gets the message.
        auto counters = state.GetCounters();
        ASSERT_EQ(counters.size(), 1ul);
        ASSERT_EQ(counters.at(CounterKey(pub1_id, DataAvailable)), 1);
    }

    // Enable notifications for pub-one again, get another Data Available
    state.ClearCounters();  // Reset all counters
    comm_channel.SendMessage(Message::SetNotificationStateMessage(pub1_id, true));
    std::this_thread::sleep_for(kPropagationWait);
    {
        // We get an initial DataAvailable notification because data was already present.
        auto counters = state.GetCounters();
        ASSERT_EQ(counters.size(), 1ul);
        ASSERT_EQ(counters.at(CounterKey(pub1_id, DataAvailable)), 1);
    }

    // Send availability messages for both pubs, only notified for pub1.
    state.ClearCounters();  // Reset all counters
    comm_channel.SendMessage(pub1_data_available);
    comm_channel.SendMessage(pub2_data_available);
    std::this_thread::sleep_for(kPropagationWait);
    {
        auto counters = state.GetCounters();
        ASSERT_EQ(counters.size(), 1ul);
        ASSERT_EQ(counters.at(CounterKey(pub1_id, DataAvailable)), 1);
    }

    // Enable notifications for pub-two, but no DataAvailable since it's empty.
    state.ClearCounters();  // Reset all counters
    comm_channel.SendMessage(Message::SetNotificationStateMessage(pub2_id, true));
    {
        auto counters = state.GetCounters();
        ASSERT_EQ(counters.size(), 0ul);
    }

    // Send availability message for pub2 only, notified only for pub2
    state.ClearCounters();  // Reset all counters
    comm_channel.SendMessage(pub2_data_available);
    std::this_thread::sleep_for(kPropagationWait);
    {
        // Still get exactly one message for pub2
        auto counters = state.GetCounters();
        ASSERT_EQ(counters.size(), 1ul);
        ASSERT_EQ(counters.at(CounterKey(pub2_id, DataAvailable)), 1);
    }

    // Disable notifications for pub-two
    comm_channel.SendMessage(Message::SetNotificationStateMessage(pub1_id, false));

    // Send availability messages for both pubs, only notified for pub2.
    state.ClearCounters();  // Reset all counters
    comm_channel.SendMessage(pub1_data_available);
    comm_channel.SendMessage(pub2_data_available);
    std::this_thread::sleep_for(kPropagationWait);
    {
        auto counters = state.GetCounters();
        ASSERT_EQ(counters.size(), 1ul);
        ASSERT_EQ(counters.at(CounterKey(pub2_id, DataAvailable)), 1);
    }

    // Disable notifications for pub-one again (another idempotency check)
    comm_channel.SendMessage(Message::SetNotificationStateMessage(pub1_id, false));

    // Send availability messages for both pubs, only notified for pub2.
    state.ClearCounters();  // Reset all counters
    comm_channel.SendMessage(pub1_data_available);
    comm_channel.SendMessage(pub2_data_available);
    std::this_thread::sleep_for(kPropagationWait);
    {
        auto counters = state.GetCounters();
        ASSERT_EQ(counters.size(), 1ul);
        ASSERT_EQ(counters.at(CounterKey(pub2_id, DataAvailable)), 1);
    }

    // Disable notifications for pub-two
    comm_channel.SendMessage(Message::SetNotificationStateMessage(pub2_id, false));

    // Check nothing goes through, now.
    state.ClearCounters();  // Reset all counters
    comm_channel.SendMessage(pub1_data_available);
    comm_channel.SendMessage(pub2_data_available);
    std::this_thread::sleep_for(kPropagationWait);
    ASSERT_EQ(state.GetCounters().size(), 0ul);
}

}  // namespace android::sdv::dt::agent::test
