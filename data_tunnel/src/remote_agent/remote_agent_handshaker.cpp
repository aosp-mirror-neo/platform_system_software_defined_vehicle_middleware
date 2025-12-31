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

#include "remote_agent/remote_agent_handshaker.h"

#include "certs.h"
#include "proto/dt_buffer.h"
#include "proto/dt_proto.h"
#include "remote_agent/remote_agent_common.h"

#include <aidl/google/sdv/identity/ServiceFqin.h>
#include <log/log.h>

using ::aidl::google::sdv::identity::ServiceFqin;

namespace android::sdv::dt::agent {

namespace {

bool CheckAuthenticatedIdentity(const IConnection& connection, const std::string& claimed_vm_name) {
    // Get issuing VM name from peer's cert chain.
    auto opt_cert_vm_name = connection.AuthenticatedVmName();
    if (!opt_cert_vm_name) {
        ALOGE("PerformHandshake(%s, %s): No authenticated VM name from certificate chain.",
              connection.ToString().c_str(), claimed_vm_name.c_str());
        return false;
    }

    // Check that VM that issued peer's cert matches peer's claimed VM name.
    if (*opt_cert_vm_name != claimed_vm_name) {
        ALOGE("PerformHandshake(%s, %s): Incorrect VM name %s in certificate chain.",
              connection.ToString().c_str(), claimed_vm_name.c_str(), opt_cert_vm_name->c_str());
        return false;
    }

    // Get the encoded identity from the peer's certificate.
    auto opt_peer_id = connection.AuthenticatedIdentity();
    if (!opt_peer_id) {
        ALOGE("PerformHandshake(%s, %s): Certificate lacks SAN.", connection.ToString().c_str(),
              claimed_vm_name.c_str());
        return false;
    }

    // Construct expected identity string for peer with claimed_vm_name.
    ServiceFqin fqin{
            .sdvVmName = claimed_vm_name,
            .sdvPackageName = config::kPackageName,
            .serviceBundleName = config::kServiceBundleName,
            .serviceInstanceName = "default",
    };
    auto expected_peer_id = EncodeNameForCert(MakeFqinString(fqin));

    // Check that encoded identity from certificate matches expected identity string.
    if (*opt_peer_id != expected_peer_id) {
        ALOGE("PerformHandshake(%s, %s): Certificate SAN %s does not match expected %s",
              connection.ToString().c_str(), claimed_vm_name.c_str(), opt_peer_id->c_str(),
              expected_peer_id.c_str());
        return false;
    }

    return true;
}

}  // namespace

RemoteAgentHandshaker::RemoteAgentHandshaker(const std::string& self_vm_name,
                                             bool require_authentication) :
      self_vm_name_(self_vm_name), require_authentication_(require_authentication) {}

// For use by mocks that override PerformHandshake; actual member values don't matter.
RemoteAgentHandshaker::RemoteAgentHandshaker() : self_vm_name_(""), require_authentication_(true) {}

RemoteAgentHandshaker::~RemoteAgentHandshaker() {}

std::optional<std::string> RemoteAgentHandshaker::PerformHandshake(
        IConnection& connection, std::optional<std::string> expected_peer_vm_name) const {
    RawBuffer send_buffer(kDefaultBufferSize);
    RawBuffer receive_buffer(kDefaultBufferSize);
    BufferWriter receive_writer(receive_buffer);
    uint32_t received_parcel_id = 0;

    // Send our Ping message
    if (!SendPayload(connection, proto::Payload_Ping(self_vm_name_), 0, send_buffer)) {
        ALOGE("PerformHandshake(%s): Unable to send Ping.", connection.ToString().c_str());
        return std::nullopt;
    }

    // Receive what should be the counterpart's Ping message and validate it.
    auto [ping_recv_result, maybe_ping_payload] =
            ReceivePayload(connection, &received_parcel_id, receive_writer);
    if (!maybe_ping_payload) {
        ALOGE("PerformHandshake(%s): Receive failure %d while awaiting Ping.",
              connection.ToString().c_str(), ping_recv_result);
        return std::nullopt;
    }

    auto* ping_payload = maybe_ping_payload->As<proto::Payload_Ping>();
    if (!ping_payload) {
        ALOGE("PerformHandshake(%s): Unexpected payload %d instead of Ping.",
              connection.ToString().c_str(), maybe_ping_payload->type_);
        return std::nullopt;
    }

    const auto& ping_peer_vm_name = ping_payload->vm_name_;

    // Check that ping VM name matches expected vm name, if any (only for outbound connections)
    if (expected_peer_vm_name && *expected_peer_vm_name != ping_peer_vm_name) {
        ALOGW("PerformHandshake(%s): Ping VM name %s does not match expected VM name %s.",
              connection.ToString().c_str(), ping_peer_vm_name.c_str(),
              expected_peer_vm_name->c_str());
        return std::nullopt;
    }

    // If authentication is required check that certificate names match ping VM name
    if (require_authentication_ && !CheckAuthenticatedIdentity(connection, ping_peer_vm_name)) {
        // CheckAuthenticatedIdentity will have already logged a detailed error.
        return std::nullopt;
    }

    // All peer validations passed; send pong message let the counterpart know that we're ready.
    if (!SendPayload(connection, proto::Payload_Pong(), 0, send_buffer)) {
        ALOGE("PerformHandshake(%s, %s): Unable to send Pong.", connection.ToString().c_str(),
              ping_peer_vm_name.c_str());
        return std::nullopt;
    }

    // Look for peer's pong signalling that they're ready.
    auto [pong_recv_result, maybe_pong_payload] =
            ReceivePayload(connection, nullptr, receive_writer);
    if (!maybe_pong_payload) {
        ALOGE("PerformHandshake(%s, %s): Receive failure %d for Pong.",
              connection.ToString().c_str(), ping_peer_vm_name.c_str(), pong_recv_result);
        return std::nullopt;
    }

    if (maybe_pong_payload->type_ != proto::Payload::Pong) {
        ALOGE("PerformHandshake(%s, %s): Unexpected payload %d instead of Pong.",
              connection.ToString().c_str(), ping_peer_vm_name.c_str(), maybe_pong_payload->type_);
        return std::nullopt;
    }

    return ping_peer_vm_name;
}

}  // namespace android::sdv::dt::agent
