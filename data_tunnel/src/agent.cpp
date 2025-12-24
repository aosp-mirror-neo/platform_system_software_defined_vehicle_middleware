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

#define LOG_TAG "sdv_dt_agent"

#include "get_service.h"
#include "service.h"

#include <aidl/google/sdv/vpm/BnPowerStateReportListener.h>
#include <aidl/google/sdv/vpm/IPowerStateClientApi.h>
#include <android/binder_process.h>
#include <log/log.h>

#include <signal.h>
#include <string.h>

using ::aidl::google::sdv::vpm::BnPowerStateReportListener;
using ::aidl::google::sdv::vpm::IPowerStateClientApi;
using ::aidl::google::sdv::vpm::PowerStateReport;

static constexpr int kBinderThreadPoolMaxCount = 1;

namespace android::sdv::dt::agent {

class PowerStateChangeListener : public BnPowerStateReportListener {
public:
    virtual ::ndk::ScopedAStatus onPowerStateReportChanged(
            PowerStateReport new_power_state) override {
        ALOGI("Got onPowerStateReportChanged: %d", static_cast<int>(new_power_state));
        if (!android::sdv::dt::agent::HandlePowerStateChange(new_power_state)) {
            ALOGE("Failed to complete power state change %d!  Exiting.",
                  static_cast<int>(new_power_state));
            exit(EXIT_FAILURE);
        }
        return ::ndk::ScopedAStatus::ok();
    }
};

std::shared_ptr<PowerStateChangeListener> InitiatePowerStateMonitoring() {
    auto listener = ::ndk::SharedRefBase::make<PowerStateChangeListener>();
    PowerStateReport current_power_state;
    auto call_status = getCachedService<IPowerStateClientApi>()
                               ->subscribeToPowerStateReport(listener, &current_power_state);
    if (!call_status.isOk()) {
        ALOGE("IPowerStateClientApi::subscribeToPowerStateReport() failed! %s",
              call_status.getDescription().c_str());
        exit(EXIT_FAILURE);  // Fatal error. Impossible to recover.
    }

    ALOGI("Power state monitoring enabled.");

    return listener;
}

}  // namespace android::sdv::dt::agent

int main(int, char**) {
    ALOGD("dt_agent starting ...");

    // Ignore SIGPIPE so we don't crash if we try to write to a dead client.
    // TODO(b/355941932): Add proper handling for SIGPIPE
    signal(SIGPIPE, SIG_IGN);

    // Set up SIGTERM blocking before startup so that we don't terminate while in a
    // partially-initialized state, and so that all threads inherit mask.

    // Set up mask containing only SIGTERM.
    sigset_t sigterm_mask;
    sigemptyset(&sigterm_mask);
    sigaddset(&sigterm_mask, SIGTERM);

    // Block normal SIGTERM handler; all threads will inherit this.
    if (pthread_sigmask(SIG_BLOCK, &sigterm_mask, NULL) == -1) {
        ALOGE("Failed to block SIGTERM: %s", strerror(errno));
        return EXIT_FAILURE;
    }

    // Start up binder threads.  This must happen before StartService as some components of the
    // service need to set up an AIDL listener and get an event back from it to complete startup.
    ABinderProcess_setThreadPoolMaxThreadCount(kBinderThreadPoolMaxCount);
    ABinderProcess_startThreadPool();

    // Starting Data Tunnel service
    int result = android::sdv::dt::agent::StartService();
    if (result != 0) {
        ALOGE("Unable to start Data Tunnel service! Error: %d", result);
        return EXIT_FAILURE;
    }

    ALOGD("dt_agent has started successfully");

    // Set up power state listener to send state change signals to service.
    // The VPM service depends on DT Agent service, so we have to wait until we've started our
    // own binder interface to set up a listener or we'll be stuck in a dependency cycle.
    auto power_state_listener = android::sdv::dt::agent::InitiatePowerStateMonitoring();

    // Wait for SIGTERM
    while (true) {
        auto signum = sigwaitinfo(&sigterm_mask, nullptr);
        if (signum == SIGTERM) {
            break;
        } else if (signum < 0) {
            ALOGE("Error %d from sigwaitinfo: %s", errno, strerror(errno));
        } else {
            ALOGW("Got unexpected signal %d from sigwaitinfo; ignoring.", signum);
        }
    }

    // Do graceful shutdown
    ALOGI("Data Tunnel agent got SIGTERM; shutting down.");
    result = android::sdv::dt::agent::ShutdownService();
    if (result != 0) {
        ALOGE("Unable to shutdown Data Tunnel service! Error: %d", result);
        return EXIT_FAILURE;
    }

    return EXIT_SUCCESS;
}
