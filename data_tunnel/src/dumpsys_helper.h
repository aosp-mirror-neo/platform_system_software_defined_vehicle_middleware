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

#include <stdarg.h>
#include <stdio.h>
#include <unistd.h>

namespace android::sdv::dt::agent::dumpsys {

inline void Dump(int fd, const char* fmt, ...) {
    const int BUFF_SIZE = 512;
    char buff[BUFF_SIZE];
    va_list va;
    va_start(va, fmt);
    int written = vsnprintf(buff, BUFF_SIZE, fmt, va);
    va_end(va);
    if (written > 0) write(fd, buff, written);
}

}  // namespace android::sdv::dt::agent::dumpsys