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

// clang-format off
// sys/socket.h must be included before linux/vm_socket.h due to
// incomplete type 'struct sockaddr' and 'sa_family_t' compilation errors
#include <sys/select.h>
#include <sys/socket.h>
#include <linux/socket.h>
#include <linux/vm_sockets.h>
#include <netinet/in.h>
// clang-format on

#define LOG_TAG "sdv_dt_agent"

#include "service.h"

#include "common.h"
#include "config.h"
#include "dumpsys_helper.h"
#include "get_service.h"
#include "proto/dt_proto.h"
#include "remote_agent/remote_agent_service.h"
#include "remote_agent_service_interface.h"
#include "topic_manager.h"

#include <aidl/google/sdv/authz/IAuthzService.h>
#include <aidl/google/sdv/data_tunnel/BnAgentService.h>
#include <aidl/google/sdv/identity/IIdentityAgent.h>
#include <aidl/google/sdv/rpc/IRpcAgent.h>
#include <aidl/google/sdv/service_discovery/discovery/IServiceDiscoveryAgent.h>
#include <aidl/google/sdv/service_discovery/discovery/ITransportSupportAgent.h>
#include <android/binder_ibinder_platform.h>
#include <android/binder_libbinder.h>
#include <android/binder_manager.h>
#include <android/binder_process.h>
#include <fmq/AidlMessageQueue.h>
#include <log/log.h>
#include <sdv/logging.h>

#include <inttypes.h>
#include <libsdv_dt_core.h>

#include <chrono>
#include <ctime>
#include <iostream>
#include <map>
#include <memory>
#include <mutex>
#include <thread>
#include <vector>

namespace android::sdv::dt::agent {
using namespace std::chrono_literals;

using ::aidl::android::hardware::common::fmq::UnsynchronizedWrite;

using ::aidl::google::sdv::authz::IAuthzService;
using ::aidl::google::sdv::rpc::IRpcAgent;
using ::aidl::google::sdv::vpm::PowerStateReport;

using AidlMQ = AidlMessageQueue<int8_t, UnsynchronizedWrite>;
using AidlMQDescriptor =
        ::aidl::android::hardware::common::fmq::MQDescriptor<int8_t, UnsynchronizedWrite>;

using CommsClientDescriptor = ::aidl::google::sdv::data_tunnel::ClientDescriptor;
using CommsAgentService = ::aidl::google::sdv::data_tunnel::IAgentService;
using CommsPublicationDescriptor = ::aidl::google::sdv::data_tunnel::PublicationDescriptor;
using CommsServiceIdentity = ::aidl::google::sdv::identity::ServiceIdentity;
using CommsServiceFqin = ::aidl::google::sdv::identity::ServiceFqin;
using CommsUnitIdentifier = ::aidl::google::sdv::identity::UnitIdentifier;
using CommsProcessIdentifier = ::aidl::google::sdv::identity::ProcessIdentifier;
using CommsIIdentityAgent = ::aidl::google::sdv::identity::IIdentityAgent;
using CommsRegistrationToken = ::aidl::google::sdv::service_discovery::common::RegistrationToken;
using CommsIServiceDiscoveryAgent =
        aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent;
using CommsServiceUnitDefinition =
        ::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition;
using CommsITransportSupportAgent =
        aidl::google::sdv::service_discovery::discovery::ITransportSupportAgent;
using CommsTransportMetadata =
        ::aidl::google::sdv::service_discovery::common::metadata::TransportMetadata;
using CommsApplicationMetadata =
        aidl::google::sdv::service_discovery::common::metadata::ApplicationMetadata;
using aidl::google::sdv::data_tunnel::ISubscriptionReadinessListener;
using aidl::google::sdv::data_tunnel::SubscriptionParams;
using aidl::google::sdv::data_tunnel::SubscriptionResult;

typedef std::vector<uint8_t> RawGenericMetadata;

// Locks down the shm segment associated with an FMQ so that any future
// mappings for it are read-only.  Does not affect existing mappings.
inline bool SealMessageQueueForWriting(const AidlMQDescriptor& mq_desc) {
    if (mq_desc.handle.fds.size() != 1) {
        ALOGE("SealMessageQueueForWriting: MQDescriptor has %zu fds, not the expected 1.",
              mq_desc.handle.fds.size());
        return false;
    }

    if (ashmem_set_prot_region(mq_desc.handle.fds[0].get(), PROT_READ) != 0) {
        ALOGE("Error from ashmem_set_prot_region: %s", strerror(errno));
        return false;
    }

    return true;
}

namespace InternalCommsSD {

static std::string GenPublicationName(int64_t sid, const std::string& unit_name) {
    char sid_str[18];
    snprintf(sid_str, 18, "%016" PRIX64 "-", sid);
    return std::string(std::string(sid_str) + unit_name);
}

static std::optional<CommsServiceIdentity> GetProcessServiceIdentity(int64_t pid, int64_t uid) {
    CommsProcessIdentifier process_identifier = {pid, uid};
    std::optional<CommsServiceIdentity> service_identity;
    auto res = getCachedService<CommsIIdentityAgent>()->findIdentityByProcess(process_identifier,
                                                                              &service_identity);
    if (!res.isOk()) {
        ALOGE("IIdentityAgent::findIdentityByProcess() failed! Error: %s",
              res.getDescription().c_str());
        return std::nullopt;
    }
    return service_identity;
}

static std::optional<int64_t> GetProcessSid(int64_t pid, int64_t uid) {
    auto service_identity = GetProcessServiceIdentity(pid, uid);
    if (!service_identity.has_value()) {
        ALOGE("Unable to get sid for process with pid=%016" PRIX64 ", uid=%016" PRIX64 "!", pid,
              uid);
        return std::nullopt;
    }
    return service_identity->sid;
}

}  // namespace InternalCommsSD

static void SubscriberCallback(core::CommChannel::Message::Type event_type, core::TopicId topic_id,
                               core::CommChannel* comm_channel) {
    if (!comm_channel) {
        ALOGE("%s: Expected non-null core::CommChannel pointer as ptr argument.", __FUNCTION__);
        return;
    }
    if (event_type == core::CommChannel::Message::DataAvailable) {
        comm_channel->SendMessage(core::CommChannel::Message::DataAvailableMessage(topic_id, 1));
    } else if (event_type == core::CommChannel::Message::Exception) {
        // Topic subscription related exception treated as Exception_TopicDestroyed
        comm_channel->SendMessage(
                core::CommChannel::Message::
                        ExceptionMessage(topic_id,
                                         core::CommChannel::Message::Exception_TopicDestroyed));
    } else {
        ALOGE("%s: Invalid callback type %d for topic_id %d", __FUNCTION__, event_type, topic_id);
    }
}

class ClientManager {
private:
    static void ReadCallback(const core::CommChannel::Message& message, void* ptr_arg,
                             int32_t client_id) {
        if (!ptr_arg) {
            ALOGE("%s: Received a message with ptr_arg==0 %X [%d, %d, %d]!", __FUNCTION__,
                  message.type_, message.args_[0], message.args_[1], message.args_[2]);
            return;
        }

        auto self = static_cast<ClientManager*>(ptr_arg);
        if (message.type_ == core::CommChannel::Message::DataAvailable) {
            self->topic_manager_->NotifyWritten(message.args_[0], message.args_[1]);
        } else if (message.type_ == core::CommChannel::Message::Exception) {
            ALOGD("Disconnecting client %08X: Received Exception message [%d, %d, %d].", client_id,
                  message.args_[0], message.args_[1], message.args_[2]);
            // TODO(b/395534244): We don't delete the element itself, but leave it in an inactive
            // state to preserve statistics and allow for its use without explicit lock.
            self->topic_manager_->ClientDisconnected(client_id);
        } else if (message.type_ == core::CommChannel::Message::SetNotificationState) {
            auto topic_id = message.args_[0];
            auto* topic = self->topic_manager_->FindTopicById(topic_id);
            if (!topic) {
                ALOGW("Got EnableNotifications for unknown topic %d from client %d. Ignoring",
                      topic_id, client_id);
                return;
            }
            std::lock_guard client_guard(self->clients_lock_);
            CHECK(client_id >= 0 && client_id < static_cast<int32_t>(self->clients_.size()))
                    << "ClientManager::ReadCallback invoked with invalid client_id!";
            Entry& entry = self->clients_[client_id];

            bool enable = (message.args_[1] != 0);

            // Whether enabling or disabling we need to remove any existing entry.
            if (entry.topic_callbacks_.contains(topic_id)) {
                topic->UnregisterSubscriberCallback(entry.topic_callbacks_[topic_id]);
                entry.topic_callbacks_.erase(topic_id);
            }

            if (enable) {
                entry.topic_callbacks_[topic_id] = topic->RegisterSubscriberCallback(
                        [comm_channel = entry.comm_channel_.get()](auto event_type, auto topic_id) {
                            SubscriberCallback(event_type, topic_id, comm_channel);
                        });
            }
        } else {
            ALOGE("%s: Received undefined message %X [%d, %d, %d]!", __FUNCTION__, message.type_,
                  message.args_[0], message.args_[1], message.args_[2]);
        }
    }

    bool PrivAddClient(int64_t sid, core::ClientId& client_id) {
        std::lock_guard<std::mutex> clients_lock_guard(clients_lock_);
        Entry new_entry;
        client_id = new_entry.client_id_ = static_cast<int32_t>(clients_.size());
        new_entry.sid_ = sid;
        // Create two pipes for full duplex Client <-> Agent communication
        if (pipe(&new_entry.pipe_fds_[FD_Client2Agent_Read]) == -1 ||
            pipe(&new_entry.pipe_fds_[FD_Agent2Client_Read]) == -1) {
            ALOGE("Unable to create comm pipe!!!");
            return false;
        }

        // Init agent side CommChannel
        auto [it, was_inserted] = sid2client_id_map_.insert({sid, client_id});
        if (!was_inserted) {
            ALOGI("Client with sid %" PRId64 " already exists, check liveness", new_entry.sid_);
            if (!IsClientAliveUnsafe(/* client_id */ it->second, clients_lock_guard)) {
                // The client with this SID might restarted, thus allocate the new client id to it.
                ALOGI("Client with sid %" PRId64 ": new client id %d (was %d)!", new_entry.sid_,
                      client_id, it->second);
                sid2client_id_map_[sid] = client_id;
            } else {
                ALOGE("Client with sid %" PRId64 " already exists and existing client is active!",
                      new_entry.sid_);
                return false;
            }
        }
        new_entry.comm_channel_ =
                core::CommChannel::FromPipeFds(new_entry.pipe_fds_[FD_Client2Agent_Read],
                                               new_entry.pipe_fds_[FD_Agent2Client_Write],
                                               ReadCallback, this, client_id);
        clients_.emplace_back(std::move(new_entry));

        return true;
    }

    enum FdType {
        FD_Client2Agent_Read = 0,
        FD_Client2Agent_Write,
        FD_Agent2Client_Read,
        FD_Agent2Client_Write,
        FD_COUNT
    };

    struct Entry {
        core::ClientId client_id_{};
        int64_t sid_;
        std::unique_ptr<core::CommChannel> comm_channel_;
        int pipe_fds_[FD_COUNT]{};
        std::unordered_map<core::TopicId, uint32_t> topic_callbacks_;
    };

public:
    explicit ClientManager(TopicManager* topic_manager) : topic_manager_(topic_manager) {}

    bool GetCounterpartFds(core::ClientId client_id, int& read_fd, int& write_fd) const {
        std::lock_guard<std::mutex> clients_lock_guard(clients_lock_);
        if (client_id < 0 || client_id >= static_cast<ssize_t>(clients_.size())) {
            ALOGE("%s: Invalid client_id: %d!", __FUNCTION__, client_id);
            return false;
        }
        read_fd = clients_[client_id].pipe_fds_[FD_Agent2Client_Read];
        write_fd = clients_[client_id].pipe_fds_[FD_Client2Agent_Write];
        return true;
    }

    bool AddCommsClient(int64_t sid, core::ClientId& client_id) {
        return PrivAddClient(sid, client_id);
    }

    core::CommChannel* GetClientCommChannel(core::ClientId client_id) {
        std::lock_guard<std::mutex> clients_lock_guard(clients_lock_);
        return GetClientCommChannelUnsafe(client_id, clients_lock_guard);
    }

    bool IsClientAlive(const core::ClientId& client_id) const {
        std::lock_guard<std::mutex> clients_lock_guard(clients_lock_);
        return IsClientAliveUnsafe(client_id, clients_lock_guard);
    }

    bool FindCliendId(int64_t sid, core::ClientId& client_id) const {
        std::lock_guard<std::mutex> clients_lock_guard(clients_lock_);
        if (auto it = sid2client_id_map_.find(sid); it != sid2client_id_map_.end()) {
            client_id = it->second;
            return true;
        }
        return false;
    }

    bool DumpStats(int fd) {
        std::lock_guard<std::mutex> clients_lock_guard(clients_lock_);
        dumpsys::Dump(fd, "\nClient Manager. Total clients: %zu\n", clients_.size());
        dumpsys::Dump(fd, "\tactive,client_id,client_sid\n");
        for (auto& client : clients_) {
            dumpsys::Dump(fd, "\t%d,%08X,%016" PRIX64 "\n", client.comm_channel_->IsConnected(),
                          client.client_id_, client.sid_);
        }
        return true;
    }

private:
    core::CommChannel* GetClientCommChannelUnsafe(core::ClientId client_id,
                                                  const std::lock_guard<std::mutex>&) const {
        if (client_id < 0 || client_id >= static_cast<ssize_t>(clients_.size())) {
            ALOGE("%s: Invalid client_id: %d!", __FUNCTION__, client_id);
            return nullptr;
        }
        return clients_[client_id].comm_channel_.get();
    }

    bool IsClientAliveUnsafe(const core::ClientId& client_id,
                             const std::lock_guard<std::mutex>& lock_guard) const {
        auto comm_channel = GetClientCommChannelUnsafe(client_id, lock_guard);
        if (comm_channel == nullptr) {
            return false;
        }
        return comm_channel->IsConnected();
    }

    TopicManager* topic_manager_;
    std::vector<Entry> clients_;
    std::map<int64_t, core::ClientId> sid2client_id_map_;
    mutable std::mutex clients_lock_;
};

// Serves remote InterAgentComClients
class Server {
private:
    /** Connection manager instance */
    std::unique_ptr<IRemoteAgentService> remote_agent_service_;

    /** TopicManager */
    std::unique_ptr<TopicManager> topic_manager_;

    /** ClientManager */
    std::unique_ptr<ClientManager> client_manager_;

public:
    Server() {
        topic_manager_ = std::make_unique<TopicManager>();
        client_manager_ = std::make_unique<ClientManager>(topic_manager_.get());
    }

    bool StartRemoteAgentManager(ndk::SpAIBinder link_to_death, IPAddressV4 listen_address) {
        ALOGD("Starting Data Tunnel Remote Agent Manager...");
        remote_agent_service_ =
                RemoteAgentService::CreateWithDefaults(*topic_manager_, link_to_death,
                                                       listen_address);
        return remote_agent_service_->Startup();
    }

    Server(const Server&) = delete;

    void Suspend() {
        ALOGD("Suspending...");
        remote_agent_service_->Shutdown();
        ALOGD("Suspend complete.");
    }

    bool Resume() {
        ALOGD("Resuming...");
        auto result = remote_agent_service_->Startup();
        if (result) {
            ALOGD("Resume complete.");
        } else {
            ALOGE("DT Agent Resume failed!");
        }
        return result;
    }

    void Shutdown() {
        ALOGD("Shutting down...");
        if (remote_agent_service_) {
            remote_agent_service_->Shutdown();
        }
    }

    TopicManager* GetTopicManager() { return topic_manager_.get(); }

    bool FindCliendId(int64_t sid, core::ClientId& client_id) const {
        return client_manager_->FindCliendId(sid, client_id);
    }

    bool RegisterCommsClient(int64_t in_sid, CommsClientDescriptor* out_client_desc) {
        ALOGD("Registering V2 (Comms) client for sid==%016" PRIX64 ".", in_sid);
        core::ClientId client_id;
        if (!client_manager_->AddCommsClient(in_sid, client_id)) {
            return false;
        }

        int client_rfd, client_wfd;
        if (!client_manager_->GetCounterpartFds(client_id, client_rfd, client_wfd)) {
            ALOGE("%s: Unable to get pipe fds for client %d", __FUNCTION__, client_id);
            return false;
        }

        std::vector<ndk::ScopedFileDescriptor> client_fds;
        std::vector<int> client_ints;
        client_fds.emplace_back(client_rfd);
        client_fds.emplace_back(client_wfd);
        *out_client_desc =
                CommsClientDescriptor{client_id, {std::move(client_fds), std::move(client_ints)}};

        return true;
    }

    ndk::ScopedAStatus RegisterCommsPublication(const CommsRegistrationToken& token,
                                                const AidlMQDescriptor& mq_desc,
                                                CommsPublicationDescriptor* publication_desc) {
        const auto sid = token.identity.sid;
        CommsServiceUnitDefinition service_unit_definition;
        auto res = getCachedService<CommsITransportSupportAgent>()
                           ->getServiceUnitForToken(token, &service_unit_definition);
        if (!res.isOk()) {
            ALOGE("ITransportSupportAgent::getServiceUnitForToken() failed. Unable to get "
                  "ServiceUnitDefinition from RegistrationToken. Error: %s",
                  res.getMessage());
            return ndk::ScopedAStatus::
                    fromExceptionCodeWithMessage(EX_ILLEGAL_ARGUMENT,
                                                 "Unable to get service unit definition for "
                                                 "provided registration token.");
        }

        const auto& unit_name = service_unit_definition.serviceUnitName;
        const auto& publisher_metadata =
                service_unit_definition.appMetadata.typedMetadata
                        .get<CommsApplicationMetadata::TypedMetadata::Tag::publisherUnit>();

        if (publisher_metadata.message_size <= 0) {
            ALOGE("Invalid publication message size %d! The value must be >0.",
                  static_cast<int32_t>(publisher_metadata.message_size));
            return ndk::ScopedAStatus::
                    fromExceptionCodeWithMessage(EX_ILLEGAL_ARGUMENT,
                                                 "Invalid publication message size.  Must be >0.");
        }

        if (publisher_metadata.message_count < 2 || (publisher_metadata.message_count & 1) != 0) {
            ALOGE("Invalid publication message count %d! The value must be even and >= 2.",
                  static_cast<int32_t>(publisher_metadata.message_count));
            return ndk::ScopedAStatus::
                    fromExceptionCodeWithMessage(EX_ILLEGAL_ARGUMENT,
                                                 "Invalid publication message count.  Must be even "
                                                 "and >= 2.");
        }

        // Check if topic is already registered (it should be impossible with the new CommStack)
        auto publication_name = InternalCommsSD::GenPublicationName(sid, unit_name);
        auto topic = topic_manager_->FindTopic(publication_name, /*local_only*/ true);
        if (topic) {
            ALOGE("Publication %s already has been published by client(%d)",
                  publication_name.c_str(), topic->GetClientId());
            return ndk::ScopedAStatus::fromExceptionCodeWithMessage(EX_ILLEGAL_ARGUMENT,
                                                                    "Publication already exists.");
        }

        core::ClientId client_id;
        if (!FindCliendId(sid, client_id)) {
            ALOGE("Unable to find a client with sid==%016" PRIX64 "!", sid);
            return ndk::ScopedAStatus::fromExceptionCodeWithMessage(EX_ILLEGAL_ARGUMENT,
                                                                    "Unknown client SID.");
        }

        // Publishing process should have already done this, but ensure that publisher-provided
        // MQ can't be written to via any new mappings after this point.  DT Agent doesn't need
        // write access so we lock it down before constructing the AidlMQ instance which is what
        // creates the shm mapping within this process.
        if (!SealMessageQueueForWriting(mq_desc)) {
            return ndk::ScopedAStatus::
                    fromExceptionCodeWithMessage(EX_ILLEGAL_ARGUMENT,
                                                 "Error write-protecting MQ.  Bad descriptor?");
        }

        // Map the FMQ into this process.
        auto mq = std::make_unique<AidlMQ>(mq_desc, /* resetPointers= */ false);
        if (!mq->isValid()) {
            ALOGE("Invalid FMQ descriptor passed to RegisterPublication!");
            return ndk::ScopedAStatus::fromExceptionCodeWithMessage(EX_ILLEGAL_ARGUMENT,
                                                                    "Invalid FMQ descriptor.");
        }

        if (mq->getQuantumSize() != sizeof(int8_t)) {
            ALOGE("FMQ provided to RegisterPublication has incorrect quantum size %zu!",
                  mq->getQuantumSize());
            return ndk::ScopedAStatus::
                    fromExceptionCodeWithMessage(EX_ILLEGAL_ARGUMENT,
                                                 "Provided FMQ has incorrect quantum size; "
                                                 "must be 1.");
        }

        if (mq->getQuantumCount() !=
            static_cast<size_t>(publisher_metadata.message_size *
                                publisher_metadata.message_count)) {
            ALOGE("Size of FMQ provided to RegisterPublication (%zu) does not match required"
                  "size %d!",
                  mq->getQuantumCount(),
                  publisher_metadata.message_size * publisher_metadata.message_count);
            return ndk::ScopedAStatus::
                    fromExceptionCodeWithMessage(EX_ILLEGAL_ARGUMENT,
                                                 "Provided FMQ quantum count does not match "
                                                 "message_count * message_size.");
        }

        Topic::Metadata metadata = {publication_name,
                                    static_cast<size_t>(publisher_metadata.message_size),
                                    static_cast<size_t>(publisher_metadata.message_count)};

        auto new_topic = topic_manager_->RegisterTopic(metadata, client_id, false, std::move(mq));
        if (!new_topic) {
            ALOGE("Unable create a local topic entry %s!", publication_name.c_str());
            return ndk::ScopedAStatus::
                    fromServiceSpecificErrorWithMessage(CommsAgentService::ERROR_UNKNOWN,
                                                        "Internal error.");
        }

        // CommsITransportSupportAgent::setAgentMetadata() completes the publication registration
        // process and informs ServiceDiscovery that the publication is fully initialized and
        // DataTunnel is ready to accept subscribers.
        auto transport_metadata = CommsTransportMetadata{{0, 1, 2, 3}};
        res = getCachedService<CommsITransportSupportAgent>()->setAgentMetadata(token,
                                                                                transport_metadata);
        if (!res.isOk()) {
            topic_manager_->UnregisterTopic(new_topic->GetId());
            ALOGE("ITransportAgentMetadata::setAgentMetadata() failed. Error: %s",
                  res.getMessage());
            return ndk::ScopedAStatus::
                    fromServiceSpecificErrorWithMessage(CommsAgentService::ERROR_UNKNOWN,
                                                        "Internal error.");
        }

        // This could fail if the topic is destroyed between the RegisterTopic call and now, due
        // to an exception or connection failure on the client CommChannel.
        if (!new_topic->MakeDescriptor(publication_desc)) {
            return ndk::ScopedAStatus::
                    fromServiceSpecificErrorWithMessage(CommsAgentService::ERROR_UNKNOWN,
                                                        "Internal error.");
        }

        return ndk::ScopedAStatus::ok();
    }

    bool DestroyCommsPublication(int64_t /*sid*/, int32_t publication_id) {
        return topic_manager_->UnregisterTopic(publication_id);
    }

    bool SubscribeToCommsPublication(int64_t client_sid, const CommsUnitIdentifier& unit_identifier,
                                     CommsPublicationDescriptor& publication_desc,
                                     Topic::OnReadyCallback on_ready) {
        core::ClientId client_id;
        if (!FindCliendId(client_sid, client_id)) {
            ALOGE("Unable to find a client with sid==%016" PRIX64 "!", client_sid);
            return false;
        }

        const auto& vm_name = unit_identifier.service_identity.name.sdvVmName;
        const std::string publication_name =
                InternalCommsSD::GenPublicationName(unit_identifier.service_identity.sid,
                                                    unit_identifier.unit_name);
        ALOGD("SubscribeToCommsPublication(%s @ %s)", publication_name.c_str(), vm_name.c_str());
        auto topic = topic_manager_->FindTopic(publication_name, /*local_only*/ false);

        if (!topic) {
            // Missing local topic, return error.
            if (!remote_agent_service_ || vm_name == remote_agent_service_->GetLocalVmName()) {
                ALOGE("Unable to subscribe to the local publication %s @ %s!",
                      publication_name.c_str(), vm_name.c_str());
                return false;
            }

            // First subscription to remote topic:  Look it up in SD, do local setup, and request
            // subscription from peer VM DT Agent.

            // Use TransportSupportAgent to check if the requested publication exists and to
            // retrieve the given unit's metadata.
            CommsServiceUnitDefinition service_unit_definition;
            CommsTransportMetadata transport_metadata;
            auto res = getCachedService<CommsITransportSupportAgent>()
                               ->getAgentMetadata(unit_identifier.service_identity.sid,
                                                  unit_identifier.unit_name,
                                                  &service_unit_definition, &transport_metadata);
            if (!res.isOk()) {
                ALOGE("CommsIServiceDiscoveryAgent::getServiceUnit() failed. Unable to get "
                      "ServiceUnitDefinition for %s. Error: %s",
                      unit_identifier.unit_name.c_str(), res.getMessage());
                return false;
            }

            if (service_unit_definition.appMetadata.typedMetadata.getTag() !=
                CommsApplicationMetadata::TypedMetadata::Tag::publisherUnit) {
                ALOGE("CommsIServiceDiscoveryAgent::getServiceUnit() succeeds, but returns invalid "
                      "value. Unable to get ServiceUnitDefinition for %s",
                      unit_identifier.unit_name.c_str());
                return false;
            }

            const auto& publisher_metadata =
                    service_unit_definition.appMetadata.typedMetadata
                            .get<CommsApplicationMetadata::TypedMetadata::Tag::publisherUnit>();

            ALOGD("Creating a local instance for the remote publication %s with message_size %d, "
                  "message_count %d",
                  publication_name.c_str(), publisher_metadata.message_size,
                  publisher_metadata.message_count);

            Topic::Metadata metadata(publication_name, publisher_metadata.message_size,
                                     publisher_metadata.message_count);

            // Create the MQ; this is the mapping that will be stored in Topic manager; it
            // needs to be writeable so that we can write messages received from remote agents to it
            // using Topic::Write.
            auto mq = std::make_unique<AidlMQ>(publisher_metadata.message_size *
                                               publisher_metadata.message_count);
            if (!mq->isValid()) {
                ALOGE("Failed to create valid FMQ for remote topic %s.", publication_name.c_str());
                return false;
            }

            // Now that we've created the writeable FMQ handle that will be stored in the topic,
            // prevent MQ from being written via future mappings (i.e. those held by subscribers).
            if (!SealMessageQueueForWriting(mq->dupeDesc())) {
                ALOGE("Failed to write-protect FMQ for remote topic %s.", publication_name.c_str());
                return false;
            }

            topic = topic_manager_->RegisterTopic(metadata, client_id,
                                                  /*is_remote*/ true, std::move(mq));
            if (topic) {
                // We just registered the topic, so we need to send a remote subscribe request.
                auto subscribe_result =
                        remote_agent_service_->RequestSubscribeToPublication(vm_name,
                                                                             publication_name);
                if (!subscribe_result.isOk()) {
                    ALOGE("RemoteAgentManager::RequestSubscribeToPublication"
                          "(vm: %s, publication: %s) failed!",
                          vm_name.c_str(), publication_name.c_str());
                    topic_manager_->UnregisterRemoteTopic(topic->GetMetadata().name);
                    return false;
                }
            } else {
                // The registration might have failed because a concurrent request succeeded first;
                // Try to find again and see if we find a matching topic.
                topic = topic_manager_->FindTopic(publication_name, /*local_only*/ false);
                if (!topic || !topic->GetIsRemote()) {
                    ALOGE("Unable to register remote publication %s @ %s!",
                          publication_name.c_str(), vm_name.c_str());
                    return false;
                }
            }
        }

        if (!topic->MakeDescriptor(&publication_desc)) {
            return false;
        }

        if (on_ready) {
            topic->RegisterOnReadyCallback(std::move(on_ready));
        }

        return true;
    }

    std::optional<std::vector<uint8_t>> GetLastPublicationMessage(
            int64_t /*client_sid*/, const CommsUnitIdentifier& unit_identifier) {
        const std::string publication_name =
                InternalCommsSD::GenPublicationName(unit_identifier.service_identity.sid,
                                                    unit_identifier.unit_name);
        // TODO: Inform ServiceDiscovery to enable AuthZ by calling
        //       TransportSupportAgent::get_transport_metadata()

        // Try to look in the local publications and in the remote publications, created by import
        // brokers
        if (auto topic = topic_manager_->FindTopic(publication_name, /* local_only = */ false)) {
            return topic->GetLastMessage();
        }

        const auto& vm_name = unit_identifier.service_identity.name.sdvVmName;
        if (!remote_agent_service_ || vm_name == remote_agent_service_->GetLocalVmName()) {
            return std::nullopt;
        }

        if (auto remote_fetch_result =
                    remote_agent_service_->FetchLastPublicationMessage(vm_name, publication_name)) {
            return remote_fetch_result.value();
        }
        return std::nullopt;
    }

    bool DumpStats(int fd, const char** args, uint32_t num_args) {
        // Show all the stats if no arguments specified
        bool dump_remote_agent_manager = true;
        bool dump_client_manager = true;
        bool dump_topic_manager = true;
        int32_t topic_id_to_dump = -1;
        uint32_t cur_arg = 0;

        auto show_usage = [fd] {
            dumpsys::Dump(fd,
                          "\t--help: show this help\n"
                          "\t--all-stats | -a: dumps all the stats [default]\n"
                          "\t--all-publications: dumps only the publication manager\n"
                          "\t--publication=<pub_id>: dumps only the publication with the specified "
                          "id\n");
        };

        while (cur_arg < num_args) {
            if (strcmp(args[cur_arg], "--help") == 0) {
                show_usage();
                return true;
            } else if (strcmp(args[cur_arg], "--all-stats") == 0 ||
                       strcmp(args[cur_arg], "-a") == 0) {
                // This is the default behavior and doesn't require any additional arguments, so we
                // just break.
                break;
            } else if (strcmp(args[cur_arg], "--all-publications") == 0) {
                dump_remote_agent_manager = false;
                dump_client_manager = false;
                break;
            } else if (strncmp(args[cur_arg], "--publication=", 14) == 0) {
                const char* v = args[cur_arg] + 14;
                char* p = 0;
                errno = 0;
                long val = strtol(v, &p, 10);
                if (errno != 0 || p == v || val < 0) {
                    dumpsys::Dump(fd,
                                  "Invalid value for --publication [%s]! Expected unsigned "
                                  "integer.\n",
                                  v);
                    return false;
                }
                dump_remote_agent_manager = false;
                dump_client_manager = false;
                topic_id_to_dump = static_cast<int32_t>(val);
                break;
            } else {
                dumpsys::Dump(fd, "Invalid argument [%s]!\n", args[cur_arg]);
                return false;
            }
        }

        return (!(dump_remote_agent_manager && remote_agent_service_) ||
                remote_agent_service_->DumpStats(fd)) &&
                (!dump_client_manager || client_manager_->DumpStats(fd)) &&
                (!dump_topic_manager || topic_manager_->DumpStats(fd, topic_id_to_dump));
    }
};

class CommsDataTunnelAgent :
      public ::aidl::google::sdv::data_tunnel::BnAgentService,
      public ServiceStatus {
public:
    CommsDataTunnelAgent(std::shared_ptr<Server> instance) : instance_(instance) {
        ALOGD("Starting Data Tunnel agent [V2 Comms]...");
        SetStatus(Running);
    }

    CommsDataTunnelAgent(const CommsDataTunnelAgent&) = delete;
    CommsDataTunnelAgent& operator=(CommsDataTunnelAgent&) = delete;

    ~CommsDataTunnelAgent() { Shutdown(); }

    void Shutdown() {
        ALOGD("Shutting down Data Tunnel agent [V2 Comms]...");
        instance_->Shutdown();
    }

    void Suspend() { instance_->Suspend(); }

    bool Resume() { return instance_->Resume(); }

    binder_status_t dump(int fd, const char** args, uint32_t numArgs) override {
        return instance_->DumpStats(fd, args, numArgs) ? STATUS_OK : STATUS_UNKNOWN_ERROR;
    }

    /**
     * IAgentService interface implementation
     */
    ::ndk::ScopedAStatus Connect(int64_t in_sid, CommsClientDescriptor* out_client_desc) override {
        ALOGD("IAgentService::Connect(sid==%016" PRIX64 ")", in_sid);
        if (!out_client_desc) {
            return ::ndk::ScopedAStatus::
                    fromExceptionCodeWithMessage(EX_NULL_POINTER,
                                                 "out_client_desc can't be nullptr!");
        }
        if (!instance_->RegisterCommsClient(in_sid, out_client_desc)) {
            return ndk::ScopedAStatus::
                    fromServiceSpecificErrorWithMessage(CommsAgentService::ERROR_UNKNOWN,
                                                        "Failed to register DT Comms client!");
        }
        return ::ndk::ScopedAStatus::ok();
    }

    ::ndk::ScopedAStatus RegisterPublication(
            const CommsRegistrationToken& in_registration_token,
            const MQDescriptor<int8_t, UnsynchronizedWrite>& mq_desc,
            CommsPublicationDescriptor* out_publication_desc) override {
        const auto sid = in_registration_token.identity.sid;
        ALOGD("IAgentService::RegisterPublication(sid==%016" PRIX64 ")", sid);

        if (!out_publication_desc) {
            return ::ndk::ScopedAStatus::
                    fromExceptionCodeWithMessage(EX_NULL_POINTER,
                                                 "out_publication_desc can't be nullptr!");
        }

        return instance_->RegisterCommsPublication(in_registration_token, mq_desc,
                                                   out_publication_desc);
    }

    ::ndk::ScopedAStatus DestroyPublication(int in_publication_id) {
        auto client_sid =
                InternalCommsSD::GetProcessSid(AIBinder_getCallingPid(), AIBinder_getCallingUid());
        ALOGD("IAgentService::DestroyPublication(sid==%016" PRIX64 ", publication_id==%08X)",
              *client_sid, in_publication_id);

        if (!instance_->DestroyCommsPublication(*client_sid, in_publication_id)) {
            ALOGE("DestroyPublication() failed!");
            return ndk::ScopedAStatus::
                    fromServiceSpecificErrorWithMessage(CommsAgentService::ERROR_UNKNOWN,
                                                        "DestroyPublication() failed!");
        }

        return ::ndk::ScopedAStatus::ok();
    }

    ::ndk::ScopedAStatus SubscribeToPublicationCommon(
            const CommsUnitIdentifier& in_unit_identifier,
            const std::shared_ptr<ISubscriptionReadinessListener>& in_ready_listener,
            CommsPublicationDescriptor* out_publication_desc) {
        auto client_service_identity =
                InternalCommsSD::GetProcessServiceIdentity(AIBinder_getCallingPid(),
                                                           AIBinder_getCallingUid());
        if (!client_service_identity.has_value()) {
            return ndk::ScopedAStatus::
                    fromServiceSpecificErrorWithMessage(CommsAgentService::ERROR_UNKNOWN,
                                                        "Unable to retrieve client service "
                                                        "identity!");
        }

        // AuthZ
        bool authz_result;
        auto res = getCachedService<IAuthzService>()
                           ->isAuthorized(client_service_identity->name,
                                          in_unit_identifier.service_identity.name,
                                          in_unit_identifier.unit_name, &authz_result);
        if (!res.isOk()) {
            ALOGE("IAuthzService::isAuthorized() failed");
            return res;
        }

        if (!authz_result) {
            return ndk::ScopedAStatus::
                    fromServiceSpecificErrorWithMessage(CommsAgentService::ERROR_ACCESS_DENIED,
                                                        "You aren't authorized to access the "
                                                        "specifiedpublication!");
        }

        if (!out_publication_desc) {
            return ::ndk::ScopedAStatus::
                    fromExceptionCodeWithMessage(EX_NULL_POINTER,
                                                 "out_publication_desc can't be nullptr!");
        }

        Topic::OnReadyCallback on_ready;  // Ignored by SubscribeToCommsPublication if unpopulated
        if (in_ready_listener) {
            on_ready = [in_ready_listener](auto status_code) {
                in_ready_listener->onSubscriptionReady(static_cast<int32_t>(status_code));
            };
        }

        if (!instance_->SubscribeToCommsPublication(client_service_identity->sid,
                                                    in_unit_identifier, *out_publication_desc,
                                                    on_ready)) {
            ALOGE("SubscribeToPublication() failed!");
            return ndk::ScopedAStatus::
                    fromServiceSpecificErrorWithMessage(CommsAgentService::ERROR_UNKNOWN,
                                                        "SubscribeToPublication() failed!");
        }

        return ::ndk::ScopedAStatus::ok();
    }

    ::ndk::ScopedAStatus SubscribeToPublicationExtended(const SubscriptionParams& in_params,
                                                        SubscriptionResult* out_result) {
        return SubscribeToPublicationCommon(in_params.unit_identifier, in_params.readiness_listener,
                                            &out_result->publication_descriptor);
    }

    ::ndk::ScopedAStatus SubscribeToPublication(
            const CommsUnitIdentifier& in_unit_identifier,
            CommsPublicationDescriptor* out_publication_desc) override {
        ALOGD("IAgentService::SubscribeToPublication(sid==%016" PRIX64 ")",
              in_unit_identifier.service_identity.sid);
        return SubscribeToPublicationCommon(in_unit_identifier, nullptr, out_publication_desc);
    }

    ::ndk::ScopedAStatus UnsubscribeFromPublication(int in_publication_id) {
        auto client_sid =
                InternalCommsSD::GetProcessSid(AIBinder_getCallingPid(), AIBinder_getCallingUid());
        ALOGD("IAgentService::UnsubscribeFromPublication(sid==%016" PRIX64
              ", publication_id==%08X)",
              *client_sid, in_publication_id);
        return ::ndk::ScopedAStatus::ok();
    }

    ::ndk::ScopedAStatus GetLastPublicationMessage(const CommsUnitIdentifier& in_unit_identifier,
                                                   std::vector<uint8_t>* out_message) override {
        ALOGD("IAgentService::GetLastMessage(sid==%016" PRIX64 ")",
              in_unit_identifier.service_identity.sid);

        auto client_service_identity =
                InternalCommsSD::GetProcessServiceIdentity(AIBinder_getCallingPid(),
                                                           AIBinder_getCallingUid());
        if (!client_service_identity.has_value()) {
            return ndk::ScopedAStatus::
                    fromServiceSpecificErrorWithMessage(CommsAgentService::ERROR_UNKNOWN,
                                                        "Unable to retrieve client service "
                                                        "identity!");
        }

        // AuthZ
        bool authz_result;
        auto res = getCachedService<IAuthzService>()
                           ->isAuthorized(client_service_identity->name,
                                          in_unit_identifier.service_identity.name,
                                          in_unit_identifier.unit_name, &authz_result);
        if (!res.isOk()) {
            ALOGE("IAuthzService::isAuthorized() failed");
            return res;
        }

        if (!authz_result) {
            return ndk::ScopedAStatus::
                    fromServiceSpecificErrorWithMessage(CommsAgentService::ERROR_ACCESS_DENIED,
                                                        "You aren't authorized to access the "
                                                        "specifiedpublication!");
        }

        if (!out_message) {
            return ::ndk::ScopedAStatus::
                    fromExceptionCodeWithMessage(EX_NULL_POINTER, "out_message can't be nullptr!");
        }
        if (auto message = instance_->GetLastPublicationMessage(client_service_identity->sid,
                                                                in_unit_identifier)) {
            *out_message = std::move(*message);
        } else {
            ALOGE("GetLastMessage() failed!");
            return ndk::ScopedAStatus::
                    fromServiceSpecificErrorWithMessage(CommsAgentService::ERROR_UNKNOWN,
                                                        "GetLastMessage() failed!");
        }

        return ::ndk::ScopedAStatus::ok();
    }

    ::ndk::ScopedAStatus GetLastMessageByTopic(
            const ::aidl::google::sdv::service_discovery::common::UnitType& /*in_unit_type*/,
            const std::string& /*in_topic_name*/, std::vector<uint8_t>* /*out_message*/) override {
        // TODO: Add implementation for GetLastMessageByTopic
        return ::ndk::ScopedAStatus::
                fromExceptionCodeWithMessage(EX_UNSUPPORTED_OPERATION,
                                             "GetLastMessageByTopic is not implemented");
    }

private:
    std::shared_ptr<Server> instance_;
};

static std::shared_ptr<CommsDataTunnelAgent> gcomms_dt_service = nullptr;

int StartService(IPAddressV4 listen_address) {
    if (!ABinderProcess_isThreadPoolStarted()) {
        ALOGE("Attempt to start Data Tunnel service with no binder thread pool started!");
        return -1;
    }

    auto server_instance = std::make_shared<Server>();
    gcomms_dt_service = ndk::SharedRefBase::make<CommsDataTunnelAgent>(server_instance);
    auto comms_dt_binder = gcomms_dt_service->asBinder();
    AIBinder_setRequestingSid(comms_dt_binder.get(), true);
    const auto commsInstanceName =
            std::string(::aidl::google::sdv::data_tunnel::IAgentService::descriptor) + "/default";
    binder_exception_t exception =
            AServiceManager_addService(comms_dt_binder.get(), commsInstanceName.data());

    if (exception != EX_NONE) {
        ALOGE("Unable to register Comms Data Tunnel service [V2]!");
        return -1;
    }

    if (!server_instance->StartRemoteAgentManager(gcomms_dt_service->asBinder(), listen_address)) {
        ALOGE("RemoteAgentManager startup failed!");
        return -1;
    }

    return 0;
}

int StartService(in_addr listen_address) {
    return StartService(IPAddressV4(listen_address));
}

int StartService() {
    auto local_ip = IPAddressV4::GetLocal();
    if (!local_ip) {
        ALOGE("Error getting local IP address: %s", local_ip.error().toString().c_str());
        return -1;
    }
    return StartService(*local_ip);
}

int ShutdownService() {
    gcomms_dt_service->Shutdown();
    return 0;
}

bool HandlePowerStateChange(::aidl::google::sdv::vpm::PowerStateReport new_power_state) {
    if (new_power_state == PowerStateReport::SUSPEND_TO_RAM_POST_FINISH ||
        new_power_state == PowerStateReport::SUSPEND_TO_DISK_POST_FINISH) {
        gcomms_dt_service->Suspend();
        return true;
    } else if (new_power_state == PowerStateReport::SUSPEND_TO_RAM_EXIT ||
               new_power_state == PowerStateReport::SUSPEND_TO_DISK_EXIT) {
        return gcomms_dt_service->Resume();
    }
    // Ignored events always "succeed".
    return true;
}

std::shared_ptr<CommsAgentService> StartTestService() {
    auto server_instance = std::make_shared<Server>();
    return ndk::SharedRefBase::make<CommsDataTunnelAgent>(server_instance);
}

}  // namespace android::sdv::dt::agent
