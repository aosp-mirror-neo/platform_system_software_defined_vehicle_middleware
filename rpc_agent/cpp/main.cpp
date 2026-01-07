// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#define LOG_TAG "RpcAgent"

#include "RpcAgentServer.h"

#include <android-base/result.h>
#include <android/binder_process.h>
#include <log/log.h>
#include <utils/Looper.h>

using android::Looper;
using android::sp;
using android::sdv::rpc::RpcAgentServer;

namespace {

const size_t kMaxBinderThreadCount = 2;

}  // namespace

int main(int /*argc*/, char** /*argv*/) {
    ALOGD("rpcagent starting ...");

    // Set up the binder
    ABinderProcess_setThreadPoolMaxThreadCount(kMaxBinderThreadCount);
    ABinderProcess_startThreadPool();

    sp<Looper> looper(Looper::prepare(/*opts=*/0));

    // Start the services
    auto result = RpcAgentServer::startService();
    if (!result.ok()) {
        ALOGE("Failed to start service: %s", result.error().message().c_str());
        RpcAgentServer::terminateService();
        exit(result.error().code());
    }

    // Don't change this debug message as it is used for performance measurements.
    ALOGD("rpcagent has started successfully");

    while (true) {
        looper->pollAll(/*timeoutMillis=*/-1);
    }
    ALOGW("RpcAgent server escaped from its loop.");

    return 0;
}
