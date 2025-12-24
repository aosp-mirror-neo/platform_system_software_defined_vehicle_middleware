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

#include <chrono>
#include <string>

namespace android::sdv::dt::agent::config {

using namespace std::chrono_literals;

extern const std::string kPackageName;
extern const std::string kServiceBundleName;
extern const std::string kServiceUnitName;
constexpr size_t kMaxServiceRetries = 10;
constexpr std::chrono::milliseconds kServiceWaitRetryDelay = 200ms;
constexpr std::chrono::milliseconds kServiceWaitRetryBackoff = 200ms;

}  // namespace android::sdv::dt::agent::config
