// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
#pragma once

#include <functional>
#include <future>

#include <sdv/context.hpp>
#include <sdv/identity.hpp>
#include <sdv/sd_common.hpp>
#include <sdv/status.hpp>

struct ASDV_SubscriptionHandle;

namespace sdv_comms::dt {

/**
 *  NOTE: All public methods of Subscriber are thread-safe and can be
 *  called from different threads without risk of data races.
 */
class Subscriber {
public:
    // Moveable, not copyable.
    Subscriber(Subscriber&&);
    Subscriber(const Subscriber&) = delete;

    // Destructor unsubscribes.
    ~Subscriber();

    // Test whether an instance contains a valid reference.
    // An instance will become invalid when moved-from.
    bool IsValid() const;

    struct CreateOptions {
        /**
         *  If true, the subscriber will receive the last message that was
         *  published to the topic. Otherwise, the subscriber will only
         *  receive messages published after the subscription was created.
         */
        bool fetch_last_message;
    };

    using CreateResult = sdv::SdvResult<Subscriber>;

    /**
     * Non-blocking.
     * Subscribes to a publication specified in unit_id and creates
     * a DataTunnel subscription. Creating more than one Subscriber instances for
     * the same UnitIdentifier will create independent objects with separate
     * subscription related resources.
     *
     * NOTE: The callback must not block or engage in long-running computation.
     *
     * Parameters:
     *    [IN] context: Reference to the local SDV Comm Stack Context object
     *    [IN] unit_id: UnitIdentifier of the publication to subscribe to
     *    [IN] options: See CreateOptions struct above for details.
     *    [IN] callback: Callback that will be invoked with Create result.
     *
     * Return value:
     *    None; Callback will be invoked exactly once with SdvResult containing
     *    either a valid Publisher or a non-OK SdvStatus.
     */
    static void CreateAsync(const ctx::Context& context, const id::UnitIdentifier& unit_id,
                            CreateOptions options, std::function<void(CreateResult)> callback);

    /**
     * Non-blocking.
     * Subscribes to a publication specified in unit_id and creates
     * a DataTunnel subscription. Creating more than one Subscriber instances for
     * the same UnitIdentifier will create independent objects with separate
     * subscription related resources.
     *
     * Parameters:
     *    [IN] context: Reference to the local SDV Comm Stack Context object
     *    [IN] unit_id: UnitIdentifier of the publication to subscribe to
     *    [IN] options: See Subscriber::Options struct for details.
     *
     * Return value:
     *    Future for SdvResult that will contain a valid Subscriber or a non-OK
     *    SdvStatus.
     */
    static std::future<CreateResult> CreateFuture(const ctx::Context& context,
                                                  const id::UnitIdentifier& unit_id,
                                                  CreateOptions options);

    /**
     * Remote blocking.
     * Subscribes to a publication specified in unit_id and creates
     * a DataTunnel subscription. Creating more than one Subscriber instances for
     * the same UnitIdentifier will create independent objects with separate
     * subscription related resources.
     *
     * Parameters:
     *    [IN] context: Reference to the local SDV Comm Stack Context object
     *    [IN] unit_id: UnitIdentifier of the publication to subscribe to
     *    [IN] options: See Subscriber::Options struct for details.
     *
     * Return value:
     *    SdvResult containing either a valid Subscriber or a non-OK SdvStatus.
     */
    static CreateResult Create(const ctx::Context& context, const id::UnitIdentifier& unit_id,
                               CreateOptions options);

    /**
     * Non blocking.
     * Returns the publication metadata that contains message size in
     * bytes along with other publication related metadata.
     *
     * Parameters:
     *
     * Return value:
     *    SdvResult that contains SdvStatus of the method call and, if
     *    method succeeded, a constant reference to the publication metadata.
     */
    sd::PublisherUnitMetadata GetMetadata() const;

    /**
     * Non blocking.
     * Reports the number of new messages available to read.
     *
     * Parameters:
     *
     * Return value:
     *    SdvResult that contains SdvStatus of the method call and, if
     *    method succeeded, the amount of messages available to read.
     *    See ASDV_AvailableToRead() in libsdv_dt API for more details
     *    regarding buffer overflow handling.
     */
    sdv::SdvResult<uint32_t> AvailableToRead() const;

    /**
     * Non blocking.
     * Reads messages from the publication's buffer.
     *
     * Parameters:
     *    [OUT] messages: Pointer to the buffer to which the data will be copied
     *                    Must be large enough to store the requested # of messages.
     *    [IN] buffer_size: Size of the buffer in bytes.
     *
     * Return value:
     *    SdvResult that contains SdvStatus of the method call and, if method
     *    succeeded, the number of messages that were read from the publication
     *    and stored to the "messages" buffer.
     *    See ASDV_ReadMessages() in libsdv_dt API for more details regarding
     *    buffer overflow handling.
     */
    sdv::SdvResult<uint32_t> ReadMessages(void* _Nonnull messages, uint32_t buffer_size);

    // Callback type for notifications.
    enum class NotificationType { Unknown, DataAvailable, Exception };
    using NotificationCallback = std::function<sdv::SdvStatusCode(std::optional<NotificationType>)>;

    /**
     * Non blocking.
     * Sets callback for all notifications for the current subscription.
     *
     * If this method is called when a notification handler is already set, then
     * the existing notification handler will be destroyed and replaced with
     * the new handler provided.
     *
     * Any time a notification handler is about to be destroyed, it will receive a final
     * call with std::nullopt as its parameter.  Causes for destruction include:
     *
     *   - Notification handler is replaced via another call to this method.
     *   - Notification handler is removed by a call to ClearNotificationHandler.
     *   - Subscriber is destroyed.
     *
     * Parameters:
     *    [IN] callback: The specified callback function will be invoked
     *                   whenever there is new data to read or an error occurs.
     *    [IN] min_interval_ms: Specifies the minimum interval between notifications
     *                          in milliseconds.  When new messages arrive within
     *                          min_interval_ms of the last notification, the callback
     *                          will only be triggered after the interval expires.
     *                          Specify 0 to notify on every message.
     *
     * Return value:
     *    Returns SdvStatusCode::kOk on success, otherwise an error code.
     */
    sdv::SdvStatus SetNotificationHandler(NotificationCallback callback,
                                          const uint32_t min_interval_ms = 0);

    /**
     * Non blocking.
     * Clears any existing notification callback for the current subscription.
     *
     * The existing callback will receive a final invocation (with std::nullopt
     * as its parameter) immediately before it is destroyed.  This call may occur
     * before or after ClearNotificationHandler returns.
     *
     * Return value:
     *    Returns SdvStatusCode::kOk on success, otherwise an error code.
     */
    sdv::SdvStatus ClearNotificationHandler();

private:
    explicit Subscriber(ASDV_SubscriptionHandle* _Nullable);
    Subscriber() = delete;

    ASDV_SubscriptionHandle* _Nullable handle_ = nullptr;
};

}  // namespace sdv_comms::dt
