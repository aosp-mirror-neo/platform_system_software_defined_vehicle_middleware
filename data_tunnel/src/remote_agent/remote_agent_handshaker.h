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

#include "connection.h"

#include <optional>

namespace android::sdv::dt::agent {

// RemoteAgentHandshaker
//
// Each client or server connection gets a reference to an instance of this class and uses
// it execute the handshake process at the appropriate point in that connection's lifecycle.
//
//   - Avoids duplication of handshake logic between client and server connections.
//   - Hides security details from client and server connection classes.
//   - Simplifies testing of client and server connection classes since it's mockable.
class RemoteAgentHandshaker {
public:
    // Constructor
    //   - self_vm_name: VM name sent to peers as self identity
    //   - Indicates whether to check authenticated names from connection certs against
    //     peer name given during handshake.
    RemoteAgentHandshaker(const std::string& self_vm_name, bool require_authentication);
    virtual ~RemoteAgentHandshaker();

    // Virtual to allow mocking.
    virtual std::optional<std::string> PerformHandshake(
            IConnection& connection, std::optional<std::string> expected_peer_vm_name) const;

protected:
    // Default constructor for use only by mocks in testing.
    RemoteAgentHandshaker();

private:
    std::string self_vm_name_;
    bool require_authentication_;
};

}  // namespace android::sdv::dt::agent
