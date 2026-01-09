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

#include <fmq/AidlMessageQueue.h>

using aidl::android::hardware::common::NativeHandle;
using aidl::android::hardware::common::fmq::GrantorDescriptor;
using aidl::android::hardware::common::fmq::MQDescriptor;
using aidl::android::hardware::common::fmq::UnsynchronizedWrite;
using ::android::AidlMessageQueue;

using namespace android;

using AidlMQ = AidlMessageQueue<int8_t, UnsynchronizedWrite>;
using AidlMQDesc = MQDescriptor<int8_t, UnsynchronizedWrite>;

GrantorDescriptor convertGrantor(int32_t fdIndex, int32_t offset, int64_t extent);

/**
 * Construct a C++ AIDL MQDescriptor<int8_t, UnsynchronizedWrite> (aka an
 * AidlMQDesc) from the fields of a Rust AIDL
 * MQDescriptor<int8_t, UnsynchronizedWrite>.
 *
 * These two types are semantically equivalent but come from separate AIDL
 * codegen backends, so we must convert between them. To convert in the opposite
 * direction, use the descFoo methods to access each field, and manually build
 * the Rust AIDL MQDescriptor<int8_t, UnsynchronizedWrite> instance;
 * see the Rust MessageQueue<T>::dupe_desc method.
 *
 * @param grantors Pointer to the start of the MQDescriptor's GrantorDescriptor
 * array.
 * @param n_grantors The length of the MQDescriptor's GrantorDescriptor array.
 * @param handle_fds Pointer to the start of the MQDescriptor's NativeHandle's
 * file-descriptor array. Ownership of array and contents is not transferred.
 * @param handle_n_fds The corresponding length.
 * @param handle_ints Pointer to the start of the MQDescriptor's NativeHandle's
 * integer array. Ownership of the array is not transferred.
 * @param handle_n_ints The corresponding length.
 * @param quantum The MQDescriptor's quantum.
 * @param flags The MQDescriptor's flags.
 *
 * @return A heap-allocated AidlMQDesc instance owned by the caller,
 * which must be freed with freeDesc.
 */

AidlMQDesc* convertDesc(const GrantorDescriptor* grantors, size_t n_grantors, const int* handle_fds,
                        size_t handle_n_fds, const int32_t* handle_ints, size_t handle_n_ints,
                        int32_t quantum, int32_t flags);

/**
 * Free a heap-allocated AidlMQDesc. Simply calls delete.
 *
 * @param desc The AidlMQDesc to free.
 */

void freeDesc(AidlMQDesc* desc);

const GrantorDescriptor* descGrantors(const AidlMQDesc& desc);
size_t descNumGrantors(const AidlMQDesc& desc);
const ndk::ScopedFileDescriptor* descHandleFDs(const AidlMQDesc& desc);
size_t descHandleNumFDs(const AidlMQDesc& desc);
const int* descHandleInts(const AidlMQDesc& desc);
size_t descHandleNumInts(const AidlMQDesc& desc);
int32_t descQuantum(const AidlMQDesc& desc);
int32_t descFlags(const AidlMQDesc& desc);

/**
 * FmqWrapper is a wrapper around AidlMessageQueue that exposes the Unsynchronized flavor
 * of fmq to be used from rust.
 * TODO: b/393598835: This wrapper will be deprecated once libfmq_rust has support for
 * Unsynchronized write.
 */

class FmqWrapper {
    std::unique_ptr<AidlMQ> inner;

private:
    FmqWrapper(const AidlMQDesc& desc, bool resetPointers = true);
    FmqWrapper(size_t numElementsInQueue);

public:
    size_t availableToRead() const;
    bool isValid() const;
    bool writeToBuffer(const int8_t* data, size_t count);
    bool readFromBuffer(int8_t* data, size_t count);
    bool skipMessages(size_t count);
    AidlMQDesc* dupeDesc() const;

    friend FmqWrapper* createFmqWrapper(const AidlMQDesc& desc, bool resetPointers);
    friend FmqWrapper* createFmqWrapper(size_t numElementsInQueue);
    friend void freeFmqWrapper(FmqWrapper* wrapper);
};

FmqWrapper* createFmqWrapper(const AidlMQDesc& desc, bool resetPointers = true);
FmqWrapper* createFmqWrapper(size_t numElementsInQueue);
void freeFmqWrapper(FmqWrapper* wrapper);
