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

#include "FmqWrapper.hpp"

/*
 * Convert a Rust NativeHandle (passed as its individual fields) to a C++ one.
 * Duplicates the file descriptors, which are passed as integers.
 */
NativeHandle convertHandle(const int* fds, size_t n_fds, const int32_t* ints, size_t n_ints) {
    std::vector<ndk::ScopedFileDescriptor> fdv;
    for (size_t i = 0; i < n_fds; i++) {
        fdv.push_back(ndk::ScopedFileDescriptor(dup(fds[i])));
    }
    std::vector<int32_t> intv(ints, ints + n_ints);

    return NativeHandle(std::move(fdv), intv);
}

GrantorDescriptor convertGrantor(int32_t fdIndex, int32_t offset, int64_t extent) {
    return GrantorDescriptor(fdIndex, offset, extent);
}

AidlMQDesc* convertDesc(const GrantorDescriptor* grantors, size_t n_grantors, const int* handle_fds,
                        size_t handle_n_fds, const int32_t* handle_ints, size_t handle_n_ints,
                        int32_t quantum, int32_t flags) {
    std::vector<GrantorDescriptor> grantorsv(grantors, grantors + n_grantors);
    auto&& handle = convertHandle(handle_fds, handle_n_fds, handle_ints, handle_n_ints);

    return new AidlMQDesc{
            grantorsv,
            std::move(handle),
            quantum,
            flags,
    };
}

void freeDesc(AidlMQDesc* desc) {
    delete desc;
}

const GrantorDescriptor* descGrantors(const AidlMQDesc& desc) {
    return desc.grantors.data();
}
size_t descNumGrantors(const AidlMQDesc& desc) {
    return desc.grantors.size();
}
const ndk::ScopedFileDescriptor* descHandleFDs(const AidlMQDesc& desc) {
    return desc.handle.fds.data();
}
size_t descHandleNumFDs(const AidlMQDesc& desc) {
    return desc.handle.fds.size();
}
const int* descHandleInts(const AidlMQDesc& desc) {
    return desc.handle.ints.data();
}
size_t descHandleNumInts(const AidlMQDesc& desc) {
    return desc.handle.ints.size();
}
int32_t descQuantum(const AidlMQDesc& desc) {
    return desc.quantum;
}
int32_t descFlags(const AidlMQDesc& desc) {
    return desc.flags;
}

FmqWrapper::FmqWrapper(size_t numElementsInQueue) : inner(new AidlMQ(numElementsInQueue)) {}

FmqWrapper::FmqWrapper(const AidlMQDesc& desc, bool resetPointers) :
      inner(new AidlMQ(desc, resetPointers)) {}

bool FmqWrapper::writeToBuffer(const int8_t* data, size_t count) {
    return inner->write(data, count);
}

bool FmqWrapper::isValid() const {
    return inner->isValid();
}

size_t FmqWrapper::availableToRead() const {
    return inner->availableToRead();
}

bool FmqWrapper::readFromBuffer(int8_t* data, size_t count) {
    return inner->read(data, count);
}

bool FmqWrapper::skipMessages(size_t nMessages) {
    AidlMessageQueue<int8_t, UnsynchronizedWrite>::MemTransaction _tx;
    auto result = inner->beginRead(nMessages, &_tx);
    if (result) {
        return inner->commitRead(nMessages);
    }
    return result;
}

AidlMQDesc* FmqWrapper::dupeDesc() const {
    return new AidlMQDesc(inner->dupeDesc());
}

FmqWrapper* createFmqWrapper(const AidlMQDesc& desc, bool resetPointers) {
    return new FmqWrapper(desc, resetPointers);
}

FmqWrapper* createFmqWrapper(size_t numElementsInQueue) {
    auto fmq = std::unique_ptr<FmqWrapper>(new FmqWrapper(numElementsInQueue));

    if (!fmq->isValid()) {
        ALOGE("Unable to create valid FMQ for %zu elements.", numElementsInQueue);
        // Return invalid instance; callers expected to check.
        return fmq.release();
    }

    // When creating an FMQ for the first time with a count of elements, the
    // current process will be writing to it via the FmqWrapper we constructed
    // above, and other processes should have only read-only access.
    //
    // So let's go ahead and protect the region from writes; this only affects
    // future mappings constructed via descriptors returned from the FmqWrapper,
    // not to the mapping already created inside the FmqWrapper above.
    auto desc = fmq->inner->dupeDesc();
    if (ashmem_set_prot_region(desc.handle.fds[0].get(), PROT_READ) != 0) {
        ALOGE("Error protecting FMQ ashmem: %s", strerror(errno));
        // Return an invalid instance instead of the one we constructed above
        // since we weren't able to write-protect the memory.
        return new FmqWrapper(/* numElementsInQueue=*/0);
    }

    return fmq.release();
}

void freeFmqWrapper(FmqWrapper* wrapper) {
    delete wrapper;
}
