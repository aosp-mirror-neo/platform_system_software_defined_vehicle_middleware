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

#include <vector>

#include <sdv/status.hpp>

// Abstract interface for all remote agent communications needed by the rest of DT Agent.
class IRemoteAgentService {
public:
    virtual bool Startup() = 0;
    virtual void Shutdown() = 0;

    virtual const std::string& GetLocalVmName() const = 0;

    virtual ::sdv::SdvStatus RequestSubscribeToPublication(const std::string& vm_name,
                                                           const std::string& publication_name) = 0;
    virtual ::sdv::SdvResult<std::vector<uint8_t>> FetchLastPublicationMessage(
            const std::string& vm_name, const std::string& publication_name) = 0;

    virtual bool DumpStats(int fd) const = 0;

    virtual ~IRemoteAgentService() {}
};
