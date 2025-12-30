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

#include "config.h"

#include <android/binder_interface_utils.h>
#include <android/binder_manager.h>
#include <log/log.h>

#include <thread>

namespace android::sdv::dt::agent {

template <typename InterfaceType>
static std::shared_ptr<InterfaceType> getCachedService() {
    static std::shared_ptr<InterfaceType> cachedService = []() {
        std::string name = std::string(InterfaceType::descriptor) + "/default";
        auto delay = config::kServiceWaitRetryDelay;
        for (size_t i = 0; i < config::kMaxServiceRetries; ++i) {
            ::ndk::SpAIBinder binder(AServiceManager_checkService(name.c_str()));
            auto cachedService = InterfaceType::fromBinder(binder);
            if (cachedService != nullptr) return cachedService;
            ALOGW("%s is not awake yet, trying again in %lld milliseconds...", name.c_str(),
                  delay.count());
            std::this_thread::sleep_for(delay);
            delay += config::kServiceWaitRetryBackoff;
        }
        // Should never get here. In case we did, it means we failed to acquire the service.
        ALOGE("Failed to bind to %s service in %zu attempts. Crashing!", name.c_str(),
              config::kMaxServiceRetries);
        abort();
    }();
    return cachedService;
}

}  // namespace android::sdv::dt::agent
