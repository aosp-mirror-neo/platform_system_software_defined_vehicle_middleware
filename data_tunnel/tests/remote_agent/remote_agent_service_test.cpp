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

#include "remote_agent/remote_agent_service.h"

#include "config.h"
#include "connection.h"
#include "mock_agents.h"
#include "proto/dt_proto.h"
#include "remote_agent/remote_agent_test_utils.h"
#include "remote_agent_test_utils.h"
#include "topic_manager.h"

#include <gtest/gtest.h>

#include <memory>

using ::testing::_;
using ::testing::ByMove;
using ::testing::DoAll;
using ::testing::Eq;
using ::testing::Mock;
using ::testing::Return;
using ::testing::SaveArg;
using ::testing::SetArgPointee;
using ::testing::StrictMock;

using ::aidl::google::sdv::ca::IAuthoritiesListener;
using ::aidl::google::sdv::identity::ServiceFqin;
using ::aidl::google::sdv::identity::ServiceIdentity;
using ::aidl::google::sdv::service_discovery::common::RegistrationToken;
using ::aidl::google::sdv::service_discovery::common::UnitType;
using ::aidl::google::sdv::service_discovery::common::metadata::TransportMetadata;
using ::aidl::google::sdv::service_discovery::discovery::IServiceUnitChangeListener;
using ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition;

namespace android::sdv::dt::agent::test {

namespace {

static const ServiceFqin kDtAgentFqin = ServiceFqin{
        /*sdvVmName*/ "",
        /*sdvPackageName*/ config::kPackageName,
        /*serviceBundleName*/ config::kServiceBundleName,
        /*serviceInstanceName*/ "default",
};

static const UnitType kDtAgentUnitType = UnitType{/*sdvPackageName*/ config::kPackageName,
                                                  /*serviceBundleName*/ config::kServiceBundleName,
                                                  /*typeName*/ "DataTunnelAgentUnitType"};
static const char* kTestVmName = "testvm1";

static const char* kFakeCertChain = "FAKE-CERT-CHAIN";

class RemoteAgentServiceTest : public ::testing::Test {
protected:
    void PopulateService_TLSDisabled() {
        ServiceIdentity identity{.name = kDtAgentFqin};
        identity.name.sdvVmName = kTestVmName;
        EXPECT_CALL(*identity_agent_, createIdentity(_, Eq(kDtAgentFqin), _, _))
                .WillOnce(DoAll(SetArgPointee<3>(identity), Return(ndk::ScopedAStatus::ok())));

        EXPECT_CALL(*certificate_authority_, isEnabled(_))
                .WillOnce(DoAll(SetArgPointee<0>(false), Return(ndk::ScopedAStatus::ok())));

        service_ = RemoteAgentService::Create(topic_manager_, ndk::SpAIBinder(), IPAddressV4(),
                                              connection_manager_factory_.AsStdFunction(),
                                              identity_agent_, certificate_authority_,
                                              service_registration_agent_, transport_support_agent_,
                                              service_discovery_agent_);

        ASSERT_NE(service_, nullptr);
    }

    void PopulateService_TLSEnabled() {
        ServiceIdentity identity{.name = kDtAgentFqin};
        identity.name.sdvVmName = kTestVmName;
        EXPECT_CALL(*identity_agent_, createIdentity(_, Eq(kDtAgentFqin), _, _))
                .WillOnce(DoAll(SetArgPointee<3>(identity), Return(ndk::ScopedAStatus::ok())));

        EXPECT_CALL(*certificate_authority_, isEnabled(_))
                .WillOnce(DoAll(SetArgPointee<0>(true), Return(ndk::ScopedAStatus::ok())));

        EXPECT_CALL(*certificate_authority_, requestCertification(_, _))
                .WillOnce(
                        DoAll(SetArgPointee<1>(kFakeCertChain), Return(ndk::ScopedAStatus::ok())));

        EXPECT_CALL(*certificate_authority_, addAuthoritiesListener(_))
                .WillOnce(
                        // AuthoritiesListener::Create waits for first event so fake one.
                        [&](std::shared_ptr<IAuthoritiesListener> authorities_listener) {
                            authorities_listener->onAuthoritiesChanged("");
                            authorities_listener_ = authorities_listener;
                            return ndk::ScopedAStatus::ok();
                        });

        service_ = RemoteAgentService::Create(topic_manager_, ndk::SpAIBinder(), IPAddressV4(),
                                              connection_manager_factory_.AsStdFunction(),
                                              identity_agent_, certificate_authority_,
                                              service_registration_agent_, transport_support_agent_,
                                              service_discovery_agent_);

        ASSERT_NE(service_, nullptr);

        Mock::VerifyAndClearExpectations(identity_agent_.get());
        Mock::VerifyAndClearExpectations(certificate_authority_.get());

        // Create expectation for call during eventual destruction.

        EXPECT_CALL(*certificate_authority_, removeAuthoritiesListener(Eq(authorities_listener_)))
                .WillOnce(Return(ndk::ScopedAStatus::ok()));
    }

    RemoteAgentServiceTest() :
          identity_agent_(ndk::SharedRefBase::make<StrictMock<MockIdentityAgent>>()),
          certificate_authority_(ndk::SharedRefBase::make<StrictMock<MockCertificateAuthority>>()),
          service_registration_agent_(
                  ndk::SharedRefBase::make<StrictMock<MockServiceRegistrationAgent>>()),
          service_discovery_agent_(
                  ndk::SharedRefBase::make<StrictMock<MockServiceDiscoveryAgent>>()),
          transport_support_agent_(
                  ndk::SharedRefBase::make<StrictMock<MockTransportSupportAgent>>()) {}

    TopicManager topic_manager_;
    ::testing::MockFunction<ConnectionManagerFactory> connection_manager_factory_;
    StrictMock<MockConnectionManager>* connection_manager_;
    std::shared_ptr<StrictMock<MockIdentityAgent>> identity_agent_;
    std::shared_ptr<StrictMock<MockCertificateAuthority>> certificate_authority_;
    std::shared_ptr<StrictMock<MockServiceRegistrationAgent>> service_registration_agent_;
    std::shared_ptr<StrictMock<MockServiceDiscoveryAgent>> service_discovery_agent_;
    std::shared_ptr<StrictMock<MockTransportSupportAgent>> transport_support_agent_;
    std::shared_ptr<IAuthoritiesListener> authorities_listener_;
    std::shared_ptr<IServiceUnitChangeListener> service_unit_change_listener_;
    ConnectionManager::EventHandler* listen_event_handler_;
    FakeConnection* connection_;
    std::unique_ptr<RemoteAgentService> service_;
};

TEST_F(RemoteAgentServiceTest, Create_and_Destroy_TLSDisabled) {
    ServiceIdentity identity{.name = kDtAgentFqin};
    identity.name.sdvVmName = kTestVmName;
    EXPECT_CALL(*identity_agent_, createIdentity(_, Eq(kDtAgentFqin), _, _))
            .WillOnce(DoAll(SetArgPointee<3>(identity), Return(ndk::ScopedAStatus::ok())));

    EXPECT_CALL(*certificate_authority_, isEnabled(_))
            .WillOnce(DoAll(SetArgPointee<0>(false), Return(ndk::ScopedAStatus::ok())));

    service_ =
            RemoteAgentService::Create(topic_manager_, ndk::SpAIBinder(), IPAddressV4(),
                                       connection_manager_factory_.AsStdFunction(), identity_agent_,
                                       certificate_authority_, service_registration_agent_,
                                       transport_support_agent_, service_discovery_agent_);

    service_.reset();
}

TEST_F(RemoteAgentServiceTest, Create_and_Destroy_TLSEnabled) {
    ServiceIdentity identity{.name = kDtAgentFqin};
    identity.name.sdvVmName = kTestVmName;
    EXPECT_CALL(*identity_agent_, createIdentity(_, Eq(kDtAgentFqin), _, _))
            .WillOnce(DoAll(SetArgPointee<3>(identity), Return(ndk::ScopedAStatus::ok())));

    EXPECT_CALL(*certificate_authority_, isEnabled(_))
            .WillOnce(DoAll(SetArgPointee<0>(true), Return(ndk::ScopedAStatus::ok())));

    EXPECT_CALL(*certificate_authority_, requestCertification(_, _))
            .WillOnce(DoAll(SetArgPointee<1>(kFakeCertChain), Return(ndk::ScopedAStatus::ok())));

    EXPECT_CALL(*certificate_authority_, addAuthoritiesListener(_))
            .WillOnce(
                    // AuthoritiesListener::Create waits for first event so fake one.
                    [&](std::shared_ptr<IAuthoritiesListener> authorities_listener) {
                        authorities_listener->onAuthoritiesChanged("");
                        authorities_listener_ = authorities_listener;
                        return ndk::ScopedAStatus::ok();
                    });

    service_ =
            RemoteAgentService::Create(topic_manager_, ndk::SpAIBinder(), IPAddressV4(),
                                       connection_manager_factory_.AsStdFunction(), identity_agent_,
                                       certificate_authority_, service_registration_agent_,
                                       transport_support_agent_, service_discovery_agent_);

    Mock::VerifyAndClearExpectations(identity_agent_.get());
    Mock::VerifyAndClearExpectations(certificate_authority_.get());

    EXPECT_CALL(*certificate_authority_, removeAuthoritiesListener(Eq(authorities_listener_)))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));

    service_.reset();

    Mock::VerifyAndClearExpectations(certificate_authority_.get());
}

TEST_F(RemoteAgentServiceTest, Startup_and_Shutdown_TLSDisabled) {
    // Populates service_
    PopulateService_TLSDisabled();

    auto connection_manager = std::make_unique<StrictMock<MockConnectionManager>>();
    connection_manager_ = connection_manager.get();
    EXPECT_CALL(connection_manager_factory_, Call(_, Eq(std::nullopt)))
            .WillOnce(Return(ByMove(std::move(connection_manager))));

    EXPECT_CALL(*connection_manager_, Listen(_)).WillOnce(Return(true));

    RegistrationToken fake_registration_token;
    fake_registration_token.identity.sid = 42;
    fake_registration_token.identity.name = kDtAgentFqin;
    EXPECT_CALL(*service_registration_agent_,
                registerServiceUnit(Eq(config::kServiceUnitName), Eq(kDtAgentUnitType), _, _, _))
            .WillOnce(DoAll(SetArgPointee<4>(fake_registration_token),
                            Return(ndk::ScopedAStatus::ok())));

    TransportMetadata expected_transport_metadata{};
    auto listen_address = SocketAddressV4(connection_manager_->GetLocalIp(),
                                          connection_manager_->GetListenPort());
    listen_address.Serialize(expected_transport_metadata.value_holder);
    EXPECT_CALL(*transport_support_agent_,
                setAgentMetadata(Eq(fake_registration_token), Eq(expected_transport_metadata)))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));

    EXPECT_CALL(*service_discovery_agent_,
                registerServiceUnitChangeListenerByType(Eq(kDtAgentUnitType), _))
            .WillOnce(DoAll(SaveArg<1>(&service_unit_change_listener_),
                            Return(ndk::ScopedAStatus::ok())));

    ASSERT_TRUE(service_->Startup());

    Mock::VerifyAndClearExpectations(&connection_manager_factory_);
    Mock::VerifyAndClearExpectations(connection_manager_);
    Mock::VerifyAndClearExpectations(service_registration_agent_.get());
    Mock::VerifyAndClearExpectations(transport_support_agent_.get());
    Mock::VerifyAndClearExpectations(service_discovery_agent_.get());

    // Double startup does nothing.
    EXPECT_CALL(connection_manager_factory_, Call(_, _)).Times(0);
    EXPECT_CALL(*connection_manager_, Listen(_)).Times(0);
    EXPECT_CALL(*service_registration_agent_, registerServiceUnit(_, _, _, _, _)).Times(0);
    EXPECT_CALL(*transport_support_agent_, setAgentMetadata(_, _)).Times(0);
    EXPECT_CALL(*service_discovery_agent_, registerServiceUnitChangeListenerByType(_, _)).Times(0);
    ASSERT_TRUE(service_->Startup());

    Mock::VerifyAndClearExpectations(&connection_manager_factory_);
    Mock::VerifyAndClearExpectations(connection_manager_);
    Mock::VerifyAndClearExpectations(service_registration_agent_.get());
    Mock::VerifyAndClearExpectations(transport_support_agent_.get());
    Mock::VerifyAndClearExpectations(service_discovery_agent_.get());

    // Now set expectations for Shutdown
    EXPECT_CALL(*service_discovery_agent_,
                unregisterServiceUnitChangeListenerByType(Eq(kDtAgentUnitType),
                                                          Eq(service_unit_change_listener_)))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));
    EXPECT_CALL(*service_registration_agent_, unregisterServiceUnit(Eq(config::kServiceUnitName)))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));

    service_->Shutdown();

    Mock::VerifyAndClearExpectations(service_registration_agent_.get());
    Mock::VerifyAndClearExpectations(service_discovery_agent_.get());

    // Double shutdown does nothing.
    EXPECT_CALL(*service_discovery_agent_, unregisterServiceUnitChangeListenerByType(_, _))
            .Times(0);
    EXPECT_CALL(*service_registration_agent_, unregisterServiceUnit(_)).Times(0);
    service_->Shutdown();

    Mock::VerifyAndClearExpectations(service_registration_agent_.get());
    Mock::VerifyAndClearExpectations(service_discovery_agent_.get());
}

TEST_F(RemoteAgentServiceTest, Startup_and_Shutdown_TLSEnabled) {
    // Populates service_
    PopulateService_TLSEnabled();

    std::string connection_cert_chain;
    auto connection_manager = std::make_unique<StrictMock<MockConnectionManager>>();
    connection_manager_ = connection_manager.get();
    EXPECT_CALL(connection_manager_factory_, Call(_, _))
            .WillOnce([&](const auto, const auto& opt_ssl_config) {
                if (opt_ssl_config) {
                    connection_cert_chain = opt_ssl_config->pem_cert_chain;
                }
                return std::move(connection_manager);
            });

    EXPECT_CALL(*connection_manager_, Listen(_)).WillOnce(Return(true));

    RegistrationToken fake_registration_token;
    fake_registration_token.identity.sid = 42;
    fake_registration_token.identity.name = kDtAgentFqin;
    EXPECT_CALL(*service_registration_agent_,
                registerServiceUnit(Eq(config::kServiceUnitName), Eq(kDtAgentUnitType), _, _, _))
            .WillOnce(DoAll(SetArgPointee<4>(fake_registration_token),
                            Return(ndk::ScopedAStatus::ok())));

    TransportMetadata expected_transport_metadata{};
    auto listen_address = SocketAddressV4(connection_manager_->GetLocalIp(),
                                          connection_manager_->GetListenPort());
    listen_address.Serialize(expected_transport_metadata.value_holder);
    EXPECT_CALL(*transport_support_agent_,
                setAgentMetadata(Eq(fake_registration_token), Eq(expected_transport_metadata)))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));

    EXPECT_CALL(*service_discovery_agent_,
                registerServiceUnitChangeListenerByType(Eq(kDtAgentUnitType), _))
            .WillOnce(DoAll(SaveArg<1>(&service_unit_change_listener_),
                            Return(ndk::ScopedAStatus::ok())));

    ASSERT_TRUE(service_->Startup());

    Mock::VerifyAndClearExpectations(&connection_manager_factory_);
    Mock::VerifyAndClearExpectations(connection_manager_);
    Mock::VerifyAndClearExpectations(service_registration_agent_.get());
    Mock::VerifyAndClearExpectations(transport_support_agent_.get());
    Mock::VerifyAndClearExpectations(service_discovery_agent_.get());

    // Check data captured during startup mock calls.
    EXPECT_EQ(connection_cert_chain, kFakeCertChain);

    // Now set expectations for Shutdown
    EXPECT_CALL(*service_discovery_agent_,
                unregisterServiceUnitChangeListenerByType(Eq(kDtAgentUnitType),
                                                          Eq(service_unit_change_listener_)))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));
    EXPECT_CALL(*service_registration_agent_, unregisterServiceUnit(Eq(config::kServiceUnitName)))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));

    service_->Shutdown();

    Mock::VerifyAndClearExpectations(service_registration_agent_.get());
    Mock::VerifyAndClearExpectations(service_discovery_agent_.get());
}

TEST_F(RemoteAgentServiceTest, Startup_FailsOnListenFailure) {
    // Populates service_
    PopulateService_TLSDisabled();

    auto connection_manager = std::make_unique<StrictMock<MockConnectionManager>>();
    connection_manager_ = connection_manager.get();
    EXPECT_CALL(connection_manager_factory_, Call(_, Eq(std::nullopt)))
            .WillOnce(Return(ByMove(std::move(connection_manager))));

    RegistrationToken fake_registration_token;
    fake_registration_token.identity.sid = 42;
    fake_registration_token.identity.name = kDtAgentFqin;
    EXPECT_CALL(*service_registration_agent_,
                registerServiceUnit(Eq(config::kServiceUnitName), Eq(kDtAgentUnitType), _, _, _))
            .WillOnce(DoAll(SetArgPointee<4>(fake_registration_token),
                            Return(ndk::ScopedAStatus::ok())));

    EXPECT_CALL(*connection_manager_, Listen(_)).WillOnce(Return(false));

    // We'll also see an unregisterServiceUnit call to undo the successful registration.
    EXPECT_CALL(*service_registration_agent_, unregisterServiceUnit(_))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));

    ASSERT_FALSE(service_->Startup());
}

TEST_F(RemoteAgentServiceTest, DumpStats) {
    // Populates service_
    PopulateService_TLSDisabled();

    auto connection_manager = std::make_unique<StrictMock<MockConnectionManager>>();
    connection_manager_ = connection_manager.get();
    EXPECT_CALL(connection_manager_factory_, Call(_, Eq(std::nullopt)))
            .WillOnce(Return(ByMove(std::move(connection_manager))));

    EXPECT_CALL(*connection_manager_, Listen(_)).WillOnce(Return(true));

    RegistrationToken fake_registration_token;
    fake_registration_token.identity.sid = 42;
    fake_registration_token.identity.name = kDtAgentFqin;
    EXPECT_CALL(*service_registration_agent_,
                registerServiceUnit(Eq(config::kServiceUnitName), Eq(kDtAgentUnitType), _, _, _))
            .WillOnce(DoAll(SetArgPointee<4>(fake_registration_token),
                            Return(ndk::ScopedAStatus::ok())));

    TransportMetadata expected_transport_metadata{};
    auto listen_address = SocketAddressV4(connection_manager_->GetLocalIp(),
                                          connection_manager_->GetListenPort());
    listen_address.Serialize(expected_transport_metadata.value_holder);
    EXPECT_CALL(*transport_support_agent_,
                setAgentMetadata(Eq(fake_registration_token), Eq(expected_transport_metadata)))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));

    EXPECT_CALL(*service_discovery_agent_,
                registerServiceUnitChangeListenerByType(Eq(kDtAgentUnitType), _))
            .WillOnce(DoAll(SaveArg<1>(&service_unit_change_listener_),
                            Return(ndk::ScopedAStatus::ok())));

    ASSERT_TRUE(service_->Startup());

    Mock::VerifyAndClearExpectations(&connection_manager_factory_);
    Mock::VerifyAndClearExpectations(connection_manager_);
    Mock::VerifyAndClearExpectations(service_registration_agent_.get());
    Mock::VerifyAndClearExpectations(transport_support_agent_.get());
    Mock::VerifyAndClearExpectations(service_discovery_agent_.get());

    // Get dumpsys output.
    std::stringstream dumpsys_output;
    service_->DumpStats(dumpsys_output);
    EXPECT_THAT(dumpsys_output.str(), ::testing::HasSubstr("Outbound Remote Agent Connections"));
    EXPECT_THAT(dumpsys_output.str(), ::testing::HasSubstr("Inbound Remote Agent Connections"));

    // Now set expectations for Shutdown
    EXPECT_CALL(*service_discovery_agent_,
                unregisterServiceUnitChangeListenerByType(Eq(kDtAgentUnitType),
                                                          Eq(service_unit_change_listener_)))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));
    EXPECT_CALL(*service_registration_agent_, unregisterServiceUnit(Eq(config::kServiceUnitName)))
            .WillOnce(Return(ndk::ScopedAStatus::ok()));

    service_->Shutdown();

    Mock::VerifyAndClearExpectations(service_registration_agent_.get());
    Mock::VerifyAndClearExpectations(service_discovery_agent_.get());
}

}  // namespace
}  // namespace android::sdv::dt::agent::test
