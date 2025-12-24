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

#include <aidl/google/sdv/data_tunnel/IAgentService.h>
#include <aidl/google/sdv/vpm/PowerStateReport.h>
#include <netinet/in.h>

#include <vector>
namespace android::sdv::dt::agent {

// NOTE: The binder thread pool must be started before StartService is run, or startup can hang
// waiting on incoming events needed for some configuration.

int StartService(in_addr listen_address);
int StartService();  // Uses first available address

int ShutdownService();

bool HandlePowerStateChange(::aidl::google::sdv::vpm::PowerStateReport new_power_state);

// For test purposes; the returned instance will not be registered with binder's service manager
// and will not have a running RemoteAgentManager.
std::shared_ptr<::aidl::google::sdv::data_tunnel::IAgentService> StartTestService();

}  // namespace android::sdv::dt::agent
