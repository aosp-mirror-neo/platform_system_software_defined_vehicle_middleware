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

#pragma once

#include <aidl/google/sdv/ca/ICertificateAuthority.h>
#include <aidl/google/sdv/identity/IIdentityAgent.h>
#include <aidl/google/sdv/service_discovery/discovery/IServiceDiscoveryAgent.h>
#include <aidl/google/sdv/service_discovery/discovery/IServiceRegistrationAgent.h>
#include <gmock/gmock.h>

namespace android::sdv::dt::agent::test {

class MockIdentityAgent : public ::aidl::google::sdv::identity::IIdentityAgent {
public:
    MockIdentityAgent() = default;
    virtual ~MockIdentityAgent() = default;

    MOCK_METHOD(ndk::ScopedAStatus, createIdentity,
                (const aidl::google::sdv::identity::ServiceIdentity::EcPublicKey& in_publicKey,
                 const aidl::google::sdv::identity::ServiceFqin& in_fqin,
                 const ::ndk::SpAIBinder& in_linkToDeath,
                 aidl::google::sdv::identity::ServiceIdentity* _aidl_return),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, verifyIdentity,
                (const aidl::google::sdv::identity::ServiceIdentity& in_identity,
                 bool* _aidl_return),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, findIdentityByUid,
                (int64_t in_sid,
                 std::optional<aidl::google::sdv::identity::ServiceIdentity>* _aidl_return),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, findIdentityByName,
                (const aidl::google::sdv::identity::ServiceFqin& in_name,
                 std::optional<aidl::google::sdv::identity::ServiceIdentity>* _aidl_return),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, findIdentityByProcess,
                (const aidl::google::sdv::identity::ProcessIdentifier& in_process_identifier,
                 std::optional<aidl::google::sdv::identity::ServiceIdentity>* _aidl_return),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, getInterfaceVersion, (int32_t* _aidl_return), (override));
    MOCK_METHOD(ndk::ScopedAStatus, getInterfaceHash, (std::string * _aidl_return), (override));

protected:
    MOCK_METHOD(ndk::SpAIBinder, asBinder, (), (override));
    MOCK_METHOD(bool, isRemote, (), (override));
};

class MockCertificateAuthority : public ::aidl::google::sdv::ca::ICertificateAuthority {
public:
    MockCertificateAuthority() = default;
    virtual ~MockCertificateAuthority() = default;

    MOCK_METHOD(ndk::ScopedAStatus, requestCertification,
                (const std::string& request, std::string* outResult), (override));

    MOCK_METHOD(ndk::ScopedAStatus, addAuthoritiesListener,
                (const std::shared_ptr<aidl::google::sdv::ca::IAuthoritiesListener>& listener),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, removeAuthoritiesListener,
                (const std::shared_ptr<aidl::google::sdv::ca::IAuthoritiesListener>& listener),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, isEnabled, (bool* _aidl_return), (override));

    MOCK_METHOD(ndk::ScopedAStatus, getInterfaceVersion, (int32_t* _aidl_return), (override));
    MOCK_METHOD(ndk::ScopedAStatus, getInterfaceHash, (std::string * _aidl_return), (override));

protected:
    MOCK_METHOD(ndk::SpAIBinder, asBinder, (), (override));
    MOCK_METHOD(bool, isRemote, (), (override));
};

class MockServiceDiscoveryAgent :
      public ::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent {
public:
    MockServiceDiscoveryAgent() = default;
    virtual ~MockServiceDiscoveryAgent() = default;

    MOCK_METHOD(
            ndk::ScopedAStatus, listServiceUnitsByType,
            (const ::aidl::google::sdv::service_discovery::common::UnitType& in_unitType,
             std::vector<::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition>*
                     _aidl_return),
            (override));

    MOCK_METHOD(
            ndk::ScopedAStatus, listServiceUnitsByName,
            (const aidl::google::sdv::service_discovery::discovery::UnitNameParameters& in_query,
             std::vector<::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition>*
                     _aidl_return),
            (override));

    MOCK_METHOD(ndk::ScopedAStatus, fetchServiceUnitsByType,
                (const aidl::google::sdv::service_discovery::common::UnitType& in_unitType,
                 const std::shared_ptr<aidl::google::sdv::service_discovery::discovery::
                                               IServiceUnitFetchListener>& in_listener),
                (override));

    MOCK_METHOD(
            ndk::ScopedAStatus, fetchServiceUnitsByName,
            (const aidl::google::sdv::service_discovery::discovery::UnitNameParameters& in_query,
             const std::shared_ptr<
                     ::aidl::google::sdv::service_discovery::discovery::IServiceUnitFetchListener>&
                     in_listener),
            (override));

    MOCK_METHOD(ndk::ScopedAStatus, getServiceUnit,
                (const ::aidl::google::sdv::identity::ServiceFqin& in_fqin,
                 const std::string& in_unitName,
                 std::optional<
                         ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition>*
                         _aidl_return),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, registerServiceUnitChangeListenerByType,
                (const ::aidl::google::sdv::service_discovery::common::UnitType& in_unitType,
                 const std::shared_ptr<::aidl::google::sdv::service_discovery::discovery::
                                               IServiceUnitChangeListener>& in_listener),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, unregisterServiceUnitChangeListenerByType,
                (const ::aidl::google::sdv::service_discovery::common::UnitType& in_unitType,
                 const std::shared_ptr<::aidl::google::sdv::service_discovery::discovery::
                                               IServiceUnitChangeListener>& in_listener),
                (override));

    MOCK_METHOD(
            ndk::ScopedAStatus, registerServiceUnitChangeListenerByName,
            (const ::aidl::google::sdv::service_discovery::discovery::UnitNameParameters& in_query,
             const std::shared_ptr<
                     ::aidl::google::sdv::service_discovery::discovery::IServiceUnitChangeListener>&
                     in_listener),
            (override));

    MOCK_METHOD(
            ndk::ScopedAStatus, unregisterServiceUnitChangeListenerByName,
            (const ::aidl::google::sdv::service_discovery::discovery::UnitNameParameters& in_query,
             const std::shared_ptr<
                     ::aidl::google::sdv::service_discovery::discovery::IServiceUnitChangeListener>&
                     in_listener),
            (override));

    MOCK_METHOD(ndk::ScopedAStatus, unregisterServiceUnitChangeListener,
                (const std::shared_ptr<::aidl::google::sdv::service_discovery::discovery::
                                               IServiceUnitChangeListener>& in_listener),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, registerServiceUnitChangeListenerByChannel,
                (const ::aidl::google::sdv::service_discovery::common::UnitType& in_unitType,
                 const std::string& in_channelName,
                 const std::shared_ptr<::aidl::google::sdv::service_discovery::discovery::
                                               IServiceUnitChangeListener>& in_listener),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, unregisterServiceUnitChangeListenerByChannel,
                (const ::aidl::google::sdv::service_discovery::common::UnitType& in_unitType,
                 const std::string& in_channelName,
                 const std::shared_ptr<::aidl::google::sdv::service_discovery::discovery::
                                               IServiceUnitChangeListener>& in_listener),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, fetchPublishersByChannelName,
                (const std::string& in_channelName,
                 const std::shared_ptr<aidl::google::sdv::service_discovery::discovery::
                                               IServiceUnitFetchListener>& in_listener),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, listChannels,
                (const std::shared_ptr<
                        ::aidl::google::sdv::service_discovery::discovery::IChannelFetchListener>&
                         in_listener),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, listChannelsByType,
                (const aidl::google::sdv::service_discovery::common::UnitType& in_unitType,
                 const std::shared_ptr<
                         ::aidl::google::sdv::service_discovery::discovery::IChannelFetchListener>&
                         in_listener),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, getInterfaceVersion, (int32_t* _aidl_return), (override));
    MOCK_METHOD(ndk::ScopedAStatus, getInterfaceHash, (std::string * _aidl_return), (override));

protected:
    MOCK_METHOD(ndk::SpAIBinder, asBinder, (), (override));
    MOCK_METHOD(bool, isRemote, (), (override));
};

class MockServiceRegistrationAgent :
      public ::aidl::google::sdv::service_discovery::discovery::IServiceRegistrationAgent {
public:
    MockServiceRegistrationAgent() = default;
    virtual ~MockServiceRegistrationAgent() = default;

    MOCK_METHOD(ndk::ScopedAStatus, registerServiceUnit,
                (const std::string& in_serviceUnitName,
                 const aidl::google::sdv::service_discovery::common::UnitType& in_unitType,
                 const aidl::google::sdv::service_discovery::common::AccessControl& in_acls,
                 const aidl::google::sdv::service_discovery::common::metadata::ApplicationMetadata&
                         in_appMetadata,
                 aidl::google::sdv::service_discovery::common::RegistrationToken* _aidl_return),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, unregisterServiceUnit, (const std::string& in_serviceUnitName),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, getInterfaceVersion, (int32_t* _aidl_return), (override));
    MOCK_METHOD(ndk::ScopedAStatus, getInterfaceHash, (std::string * _aidl_return), (override));

protected:
    MOCK_METHOD(ndk::SpAIBinder, asBinder, (), (override));
    MOCK_METHOD(bool, isRemote, (), (override));
};

class MockTransportSupportAgent :
      public ::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent {
public:
    MockTransportSupportAgent() = default;
    virtual ~MockTransportSupportAgent() = default;

    MOCK_METHOD(ndk::ScopedAStatus, getServiceUnitForToken,
                (const ::aidl::google::sdv::service_discovery::common::RegistrationToken& in_token,
                 ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition*
                         _aidl_return),
                (override));

    MOCK_METHOD(::ndk::ScopedAStatus, setAgentMetadata,
                (const ::aidl::google::sdv::service_discovery::common::RegistrationToken& in_token,
                 const ::aidl::google::sdv::service_discovery::common::metadata::TransportMetadata&
                         in_agentMetadata),
                (override));

    MOCK_METHOD(::ndk::ScopedAStatus, getAgentMetadata,
                (int64_t in_sid, const std::string& in_unitName,
                 ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition* out_sud,
                 ::aidl::google::sdv::service_discovery::common::metadata::TransportMetadata*
                         out_agentMetadata),
                (override));

    MOCK_METHOD(ndk::ScopedAStatus, getInterfaceVersion, (int32_t* _aidl_return), (override));
    MOCK_METHOD(ndk::ScopedAStatus, getInterfaceHash, (std::string * _aidl_return), (override));

protected:
    MOCK_METHOD(ndk::SpAIBinder, asBinder, (), (override));
    MOCK_METHOD(bool, isRemote, (), (override));
};

}  // namespace android::sdv::dt::agent::test
