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

#define LOG_TAG "sdv_dt_agent:remote_agent"

#include "remote_agent/remote_agent_service.h"

#include "certs.h"
#include "config.h"
#include "connection_grpc/connection_manager.h"
#include "remote_agent/remote_agent_common.h"

#include <sstream>

using ::aidl::google::sdv::ca::ICertificateAuthority;
using ::aidl::google::sdv::identity::IIdentityAgent;
using ::aidl::google::sdv::identity::ServiceFqin;
using ::aidl::google::sdv::identity::ServiceIdentity;
using ::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent;
using ::aidl::google::sdv::service_discovery::discovery::IServiceRegistrationAgent;
using ::aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent;

namespace {

template <class T>
static std::shared_ptr<T> GetService() {
    static auto ptr = []() {
        std::string service_name = std::string(T::descriptor) + "/default";
        ndk::SpAIBinder service(AServiceManager_waitForService(service_name.c_str()));
        return T::fromBinder(service);
    }();

    return ptr;
}

}  // namespace

namespace android::sdv::dt::agent {

std::unique_ptr<RemoteAgentService> RemoteAgentService::CreateWithDefaults(
        TopicManager& topic_manager, ndk::SpAIBinder link_to_death, IPAddressV4 listen_address) {
    auto identity_agent = GetService<IIdentityAgent>();
    if (!identity_agent) {
        ALOGE("RemoteAgentService: Unable to connect to IIdentityAgent!");
        return nullptr;
    }

    auto certificate_authority = GetService<ICertificateAuthority>();
    if (!certificate_authority) {
        ALOGE("RemoteAgentService: Unable to connect to ICertificateAuthority!");
        return nullptr;
    }

    auto service_registration_agent = GetService<IServiceRegistrationAgent>();
    if (!service_registration_agent) {
        ALOGE("RemoteAgentService: Unable to connect to IServiceRegistrationAgent!");
        return nullptr;
    }

    auto transport_support_agent = GetService<ITransportSupportAgent>();
    if (!transport_support_agent) {
        ALOGE("RemoteAgentService: Unable to connect to ITransportSupportAgent!");
        return nullptr;
    }

    auto service_discovery_agent = GetService<IServiceDiscoveryAgent>();
    if (!service_discovery_agent) {
        ALOGE("Unable to connect to IServiceDiscoveryAgent!");
        return nullptr;
    }

    return Create(topic_manager, link_to_death, listen_address, CreateGrpcConnectionManager,
                  identity_agent, certificate_authority, service_registration_agent,
                  transport_support_agent, service_discovery_agent);
}

std::unique_ptr<RemoteAgentService> RemoteAgentService::Create(
        TopicManager& topic_manager, ndk::SpAIBinder link_to_death, IPAddressV4 listen_address,
        ConnectionManagerFactory connection_manager_factory,
        std::shared_ptr<IIdentityAgent> identity_agent,
        std::shared_ptr<ICertificateAuthority> certificate_authority,
        std::shared_ptr<IServiceRegistrationAgent> service_registration_agent,
        std::shared_ptr<ITransportSupportAgent> transport_support_agent,
        std::shared_ptr<IServiceDiscoveryAgent> service_discovery_agent) {
    auto public_key =
            ServiceIdentity::EcPublicKey{{0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1, 2, 3, 4, 5,
                                          6, 7, 8, 9, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 1}};
    auto fqin = ServiceFqin{
            /*sdvVmName*/ "",
            /*sdvPackageName*/ config::kPackageName,
            /*serviceBundleName*/ config::kServiceBundleName,
            /*serviceInstanceName*/ "default",
    };

    ServiceIdentity service_identity;
    auto call_status =
            identity_agent->createIdentity(public_key, fqin, link_to_death, &service_identity);
    if (!call_status.isOk()) {
        ALOGE("IdentityAgent::createIdentity() failed! %s", call_status.getDescription().c_str());
        return nullptr;
    }

    bool cert_agent_enabled;
    call_status = certificate_authority->isEnabled(&cert_agent_enabled);
    if (!call_status.isOk()) {
        ALOGE("ICertificateAuthority::isEnabled() failed! %s",
              call_status.getDescription().c_str());
        return nullptr;
    }

    if (!cert_agent_enabled) {
        ALOGW("ICertificateAuthority::isEnabled() = false.  Inter-agent comms unencrypted.");
        return std::unique_ptr<RemoteAgentService>(
                new RemoteAgentService(topic_manager, listen_address, service_identity, nullptr,
                                       std::nullopt, connection_manager_factory,
                                       service_registration_agent, transport_support_agent,
                                       service_discovery_agent));
    }

    ALOGI("ICertificateAuthority::isEnabled() = true.  Configuring SSL inter-agent comms.");

    // Create a new private key and a certification request to go with it.
    auto pkey_and_cert_req_result =
            BuildPrivateKeyAndCertRequest(MakeFqinString(service_identity.name));
    if (!pkey_and_cert_req_result.has_value()) {
        ALOGE("Error creating private key or certificate request!");
        return nullptr;
    }

    // Ask ICertificateAuthority to sign our certificate.
    auto [pkey, cert_req] = pkey_and_cert_req_result.value();
    std::string cert_chain;
    call_status = certificate_authority->requestCertification(cert_req, &cert_chain);
    if (!call_status.isOk()) {
        ALOGE("ICertificateAuthority::requestCertification() failed! %s",
              call_status.getDescription().c_str());
        return nullptr;
    }

    if (cert_chain.empty()) {
        ALOGE("ICertificateAuthority::requestCertification() provided empty cert chain!");
        return nullptr;
    }

    auto auth_listener_result = AuthoritiesListener::Create(certificate_authority);
    if (!auth_listener_result.has_value()) {
        std::stringstream error_msg;
        error_msg << auth_listener_result.error();
        ALOGE("Unable to create authority listener: %s", error_msg.str().c_str());
        return nullptr;
    }

    // Move AuthoritiesListener into its own heap allocation so its address doesn't change (and
    // invalidate the reference in ssl_config) when we pass ownership into the RemoteAgentService.
    auto auth_listener =
            std::make_unique<AuthoritiesListener>(std::move(auth_listener_result.value()));

    auto ssl_config = ConnectionManager::SslConfig{
            .trusted_roots_accessor = *auth_listener,
            .pem_private_key = pkey,
            .pem_cert_chain = cert_chain,
    };

    return std::unique_ptr<RemoteAgentService>(
            new RemoteAgentService(topic_manager, listen_address, service_identity,
                                   std::move(auth_listener), ssl_config, connection_manager_factory,
                                   service_registration_agent, transport_support_agent,
                                   service_discovery_agent));
}

RemoteAgentService::RemoteAgentService(
        TopicManager& topic_manager, IPAddressV4 listen_address, const ServiceIdentity& identity,
        std::unique_ptr<AuthoritiesListener> authorities_listener,
        std::optional<ConnectionManager::SslConfig> ssl_config,
        ConnectionManagerFactory connection_manager_factory,
        std::shared_ptr<IServiceRegistrationAgent> service_registration_agent,
        std::shared_ptr<ITransportSupportAgent> transport_support_agent,
        std::shared_ptr<IServiceDiscoveryAgent> service_discovery_agent) :
      topic_manager_(topic_manager),
      identity_(identity),
      authorities_listener_(std::move(authorities_listener)),
      listen_address_(listen_address),
      ssl_config_(ssl_config),
      handshaker_(/* self_vm_name= */ identity.name.sdvVmName,
                  /* require_authentication= */ !!ssl_config),
      connection_manager_factory_(connection_manager_factory),
      service_registration_agent_(service_registration_agent),
      transport_support_agent_(transport_support_agent),
      service_discovery_agent_(service_discovery_agent),
      client_manager_(identity, handshaker_, topic_manager_),
      server_(config::kServiceUnitName, handshaker_, topic_manager_) {}

RemoteAgentService::~RemoteAgentService() {
    Shutdown();
}

bool RemoteAgentService::Startup() {
    if (connection_manager_) {
        ALOGW("RemoteAgentService::Startup when already started!");
        return true;
    }

    // We construct connection manager, server, and client manager into locals
    // at first so that they're destroyed if we return early.
    auto connection_manager = connection_manager_factory_(listen_address_, ssl_config_);
    if (!connection_manager) {
        ALOGE("RemoteAgentService::Startup: Unable to create ConnectionManager");
        return false;
    }

    if (!server_.Startup(*connection_manager, service_registration_agent_,
                         transport_support_agent_)) {
        ALOGE("RemoteAgentService::Startup: Unable to create RemoteAgentServer");
        return false;
    }

    if (!client_manager_.Startup(*connection_manager, service_discovery_agent_,
                                 transport_support_agent_)) {
        ALOGE("RemoteAgentService::Startup: Unable to start RemoteAgentClientManager");
        server_.Shutdown();
        return false;
    }

    // "Commit" by moving ownership into members once everything succeeds.
    std::unique_lock guard(lock_);
    connection_manager_ = std::move(connection_manager);
    return true;
}

void RemoteAgentService::Shutdown() {
    if (!connection_manager_) {
        ALOGW("RemoteAgentService::Shutdown received when already shut down!");
        return;
    }

    // Move the members we're about to destroy into local variables so that we don't have to
    // hold the lock while they're being destroyed.
    std::unique_lock guard(lock_);
    auto connection_manager = std::exchange(connection_manager_, nullptr);
    guard.unlock();

    // client_manager_ holds a reference to connection_manager_ and calls `Connect` on it, so
    // it needs to be shut down first to avoid calling a method on a destroyed object.
    client_manager_.Shutdown();

    // Server sessions must be cancelled and underlying connections finished before connection
    // manager shutdown (which happens in delete) can complete.
    server_.Shutdown();

    // connection_manager_ can be destroyed once client_manager_ and server_ are shut down.
    connection_manager.reset();
}

const std::string& RemoteAgentService::GetLocalVmName() const {
    return identity_.name.sdvVmName;
}

::sdv::SdvStatus RemoteAgentService::RequestSubscribeToPublication(
        const std::string& vm_name, const std::string& publication_name) {
    return client_manager_.RequestSubscribe(vm_name, publication_name);
}

::sdv::SdvResult<std::vector<uint8_t>> RemoteAgentService::FetchLastPublicationMessage(
        const std::string& vm_name, const std::string& publication_name) {
    auto result_future = client_manager_.FetchLastMessage(vm_name, publication_name);

    auto status = result_future.wait_for(std::chrono::seconds(2));

    if (status == std::future_status::timeout) {
        return base::unexpected(
                ::sdv::SdvStatus(::sdv::SdvStatusCode::kDeadlineExceeded,
                                 std::format("FetchLastPublicationMessage() from {} timed out.",
                                             vm_name)));
    }
    if (status != std::future_status::ready) {
        return base::unexpected(::sdv::SdvStatus(::sdv::SdvStatusCode::kInternal,
                                                 std::format("FetchLastPublicationMessage() from "
                                                             "{} encountered unknown error.",
                                                             vm_name)));
    }
    auto& result = result_future.get();
    if (!result) {
        return base::unexpected(result.error());
    }
    return result.value();
}

bool RemoteAgentService::DumpStats(int fd) const {
    std::stringstream content;
    if (!DumpStats(content)) {
        return false;
    }
    auto view = content.view();
    auto write_result = write(fd, view.data(), view.size());
    if (write_result < 0) {
        ALOGW("Error writing dumpsys data: %s (%d)", strerror(errno), errno);
        return false;
    }
    return static_cast<size_t>(write_result) == view.size();
}

bool RemoteAgentService::DumpStats(std::ostream& out_stream) const {
    std::shared_lock guard(lock_);
    return client_manager_.DumpStats(out_stream) && server_.DumpStats(out_stream);
}

}  // namespace android::sdv::dt::agent
