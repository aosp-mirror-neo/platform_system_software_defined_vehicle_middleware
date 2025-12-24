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

#define LOG_TAG "libsdv_dt_core"

#include "libsdv_dt_core.h"

#include <log/log.h>

#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <sys/inotify.h>
#include <sys/stat.h>
#include <unistd.h>

#include <atomic>
#include <mutex>
#include <thread>

#define CLOSE_AND_RESET(fd) \
    {                       \
        if (fd != -1) {     \
            close(fd);      \
            fd = -1;        \
        }                   \
    }

namespace android::sdv::dt::core {

class CommChannelImpl : public CommChannel {
    std::atomic<int> read_pipe_fd_ = -1;
    std::atomic<int> write_pipe_fd_ = -1;
    std::atomic<int> termination_pipe_read_ = -1;
    std::atomic<int> termination_pipe_write_ = -1;
    std::thread read_thread_;
    ReadCallback callback_;
    void* callback_ptr_arg_;
    int32_t callback_int_arg_;

private:
    std::mutex fd_mutex_;

    static bool ReadMessageFromFd(int fd, Message& message) {
        if (fd == -1) {
            return false;
        }
        auto nread = read(fd, &message, sizeof(Message));
        return nread == sizeof(Message);
    }

    static bool WriteMessageToFd(int fd, const Message& message) {
        if (fd == -1) return false;
        auto nwritten = write(fd, &message, sizeof(Message));
        return nwritten == sizeof(Message);
    }

    void Disconnect() {
        std::lock_guard lg(fd_mutex_);

        CLOSE_AND_RESET(read_pipe_fd_);
        CLOSE_AND_RESET(write_pipe_fd_);
        CLOSE_AND_RESET(termination_pipe_read_);
        CLOSE_AND_RESET(termination_pipe_write_);
    }

    static void ReadThread(CommChannelImpl* self) {
        Message message;
        const int N_FILE_DESCRIPTORS = 2;
        pollfd pfd[N_FILE_DESCRIPTORS] = {
                {self->read_pipe_fd_, POLLIN, 0},
                {self->termination_pipe_read_, POLLIN, 0},
        };
        pollfd& readfd = pfd[0];
        pollfd& terminationfd = pfd[1];

        bool send_error_callback = false;
        while (true) {
            int retval = poll(pfd, N_FILE_DESCRIPTORS, -1 /* indefinite */);

            if (retval < 0) {
                if (errno == EINTR) {
                    continue; /* ignore EINTR, and restart the system call */
                }

                /* all other cases (i.e. EFAULT, EINVAL, ENOMEM) are fatal.  */
                ALOGE("Fatal: poll() %s", strerror(errno));
                send_error_callback = true;
                break;
            }

            /* poll must not timeout - waits indefinitely. consider as fatal. */
            LOG_ALWAYS_FATAL_IF(retval == 0, "Fatal: poll() timedout");

            // if termination pipe has a read event, disconnect. A termination event was sent.
            if (terminationfd.revents & POLLIN) {
                break;
            }

            // some unexpected generic error happened in terminate file descriptor. Always treat as
            // fatal.
            LOG_ALWAYS_FATAL_IF(terminationfd.revents & (POLLERR | POLLHUP),
                                "Fatal:poll() terminationfd error %x", terminationfd.revents);

            // if there is an error in readfd, break the loop before reading any more data.
            if (readfd.revents & (POLLERR | POLLNVAL)) {
                ALOGE("Fatal:poll() readfd error %x", readfd.revents);
                send_error_callback = true;
                break;
            }

            // if POLLHUP is set, there may still be some data to read. One way to handle this is
            // reading until read returns 0 if POLLIN is set. The other way is -done here- reading
            // a maximum number of bytes (aka, "message") however if there is exactly enough number
            // of bytes available to fill buffer, and sender does not write new data, we find
            // ourselves in an infinite loop. Instead we monitor this separately below.
            if (readfd.revents & POLLIN) {
                if (ReadMessageFromFd(self->read_pipe_fd_, message)) {
                    self->callback_(message, self->callback_ptr_arg_, self->callback_int_arg_);
                } else {
                    ALOGE("%s: ReadMessageFromFd failed [%p]!", __FUNCTION__, self);
                    send_error_callback = true;
                    break;
                }
            } else if (readfd.revents & POLLHUP) {
                // remote side closed connection after finishing writes.
                // Note: even though the way it is coded sounds like this is an error, it is really
                // used to signal to resource cleanup after disconnection. This logic needs
                // improvement to better handle graceful shutdowns.
                send_error_callback = true;
                break;
            }
        }

        self->Disconnect();
        if (send_error_callback) {
            self->callback_(Message::ExceptionMessage(InvalidTopicId,
                                                      Message::Exception_ClientDisconnected),
                            self->callback_ptr_arg_, self->callback_int_arg_);
        }
    }

public:
    CommChannelImpl(int read_pipe_fd, int write_pipe_fd, ReadCallback callback,
                    void* callback_ptr_arg, int32_t callback_int_arg) :
          read_pipe_fd_(read_pipe_fd),
          write_pipe_fd_(write_pipe_fd),
          callback_(callback),
          callback_ptr_arg_(callback_ptr_arg),
          callback_int_arg_(callback_int_arg) {
        // Create the pipe for signaling termination
        int termination_pipe[2];
        if (pipe(termination_pipe) == -1) {
            // TODO: Handle pipe creation error
            ALOGE("Failed to create termination pipe!");
        } else {
            termination_pipe_read_ = termination_pipe[0];
            termination_pipe_write_ = termination_pipe[1];
            read_thread_ = std::thread(ReadThread, this);
        }
    }

    bool IsConnected() const override { return read_pipe_fd_ != -1 && write_pipe_fd_ != -1; }

    bool SendMessage(const Message& message) override {
        return WriteMessageToFd(write_pipe_fd_, message);
    }

    ~CommChannelImpl() {
        {
            std::lock_guard lg(fd_mutex_);
            // write a byte to signal termination for reader thread
            if (termination_pipe_write_ != -1) {
                write(termination_pipe_write_, "q", 1);
            }
        }
        read_thread_.join();
    }
};

std::unique_ptr<CommChannel> CommChannel::FromPipeFds(int read_pipe_fd, int write_pipe_fd,
                                                      ReadCallback callback, void* callback_ptr_arg,
                                                      int32_t callback_int_arg) {
    ALOGD("Creating Channel from pipes %d, %d", read_pipe_fd, write_pipe_fd);
    return std::make_unique<CommChannelImpl>(read_pipe_fd, write_pipe_fd, callback,
                                             callback_ptr_arg, callback_int_arg);
}

}  // namespace android::sdv::dt::core
