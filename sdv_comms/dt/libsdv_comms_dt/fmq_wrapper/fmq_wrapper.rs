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
//!Exposing rust friendly wrapper for libfmq.

pub use android_hardware_common_fmq::aidl::android::hardware::common::fmq::{
    GrantorDescriptor::GrantorDescriptor, MQDescriptor::MQDescriptor,
    UnsynchronizedWrite::UnsynchronizedWrite,
};
use log::error;
use sdv_fmq_bindgen::{
    convertDesc, convertGrantor, createFmqWrapper, createFmqWrapper1, descFlags, descGrantors,
    descHandleFDs, descHandleInts, descHandleNumFDs, descHandleNumInts, descNumGrantors,
    descQuantum, freeDesc, freeFmqWrapper, ndk_ScopedFileDescriptor, AidlMQDesc, FmqWrapper,
    NativeHandle, ParcelFileDescriptor,
};
use std::ptr::NonNull;

/// Struct that wraps around cpp FmqWrapper
pub struct FmqWrapperRust {
    inner: NonNull<FmqWrapper>,
}

// SAFETY: FmqWrapperRust object is essentially a wrapper around a thread safe cpp class
// called FmqWrapper. [FmqWrapperRust] is safe to be sent to different threads.
unsafe impl Send for FmqWrapperRust {}

/// Implementing functions for FMQ Wrapper that are exposed to rust
impl FmqWrapperRust {
    /// Create a new MessageQueue with capacity for `elems` elements.
    ///
    /// The constructed queue will only be writeable via the returned FmqWrapperRust.
    /// Any handles to the queue created via dupe_desc() and from_desc() (or constructed
    /// from a descriptor passed other processes) will be read-only.
    ///
    /// The resulting instance could be invalid in the event of an error; callers need to
    /// check with is_valid() before attempting to use it.
    pub fn new(elems: usize) -> Self {
        Self {
            // SAFETY: Calling bindgen'd constructor. The only argument that
            // can't be validated by the implementation is the quantum, which
            // must equal the element size.
            inner: NonNull::new(unsafe { createFmqWrapper1(elems) })
                .expect("Unable to create FmqWrapper"),
        }
    }

    /// Converts rust MQDescriptor to cpp MQDescriptor, retrieves the corresponding fmq and
    /// wraps it a new FmqWrapperRust object
    pub fn from_desc(desc: &MQDescriptor<i8, UnsynchronizedWrite>, reset_pointers: bool) -> Self {
        // These two types (Rust and C++ MQDescriptors) are semantically equivalent but
        // come from separate AIDL codegen backends, so we must convert between them.
        let mut grantors = desc
            .grantors
            .iter()
            // # Safety: this just forwards the integers to the GrantorDescriptor
            // constructor; GrantorDescriptor is POD.
            .map(|g| unsafe { convertGrantor(g.fdIndex, g.offset, g.extent) })
            .collect::<Vec<_>>();

        // # Safety: These pointer/length pairs come from Vecs that will outlive
        // this function call, and the call itself copies all data it needs out
        // of them.
        let cpp_desc = unsafe {
            convertDesc(
                grantors.as_mut_ptr(),
                grantors.len(),
                desc.handle.fds.as_ptr().cast(),
                desc.handle.fds.len(),
                desc.handle.ints.as_ptr(),
                desc.handle.ints.len(),
                desc.quantum,
                desc.flags,
            )
        };

        // SAFETY: Calling bindgen'd constructor which does not store cpp_desc,
        // but just passes it to the initializer of AidlMQDesc, which
        // deep-copies it.
        let inner = NonNull::new(unsafe { createFmqWrapper(cpp_desc, reset_pointers) })
            .expect("Unable to create FmqWrapper");
        // SAFETY: we must free the desc returned by convertDesc; the pointer
        // was just returned above so we know it is valid.
        unsafe { freeDesc(cpp_desc) };
        Self { inner }
    }

    /// Obtain a copy of the MessageQueue's descriptor, which may be used to
    /// access it remotely.
    pub fn dupe_desc(&self) -> MQDescriptor<i8, UnsynchronizedWrite> {
        // SAFETY: dupeDesc may be called on any valid FmqWrapper; it
        // simply forwards to dupeDesc on the inner AidlMessageQueue and wraps
        // in a heap allocation.
        let cpp_desc: *mut AidlMQDesc = unsafe { (*self.inner.as_ptr()).dupeDesc() };
        let grantor_to_rust =
            |g: &sdv_fmq_bindgen::aidl_android_hardware_common_fmq_GrantorDescriptor| {
                GrantorDescriptor { fdIndex: g.fdIndex, offset: g.offset, extent: g.extent }
            };

        let scoped_to_parcel_fd = |fd: &ndk_ScopedFileDescriptor| {
            use std::os::fd::{BorrowedFd, FromRawFd, OwnedFd};
            // SAFETY: the fd is already open as an invariant of ndk::ScopedFileDescriptor, so
            // it will not be -1, as required by BorrowedFd.
            let borrowed = unsafe { BorrowedFd::borrow_raw(fd._base as i32) };
            ParcelFileDescriptor::new(match borrowed.try_clone_to_owned() {
                Ok(fd) => fd,
                Err(e) => {
                    error!("could not dup NativeHandle fd {}: {}", fd._base, e);
                    // SAFETY: OwnedFd requires the fd is not -1. If we failed to dup the fd,
                    // other code downstream will fail, but we can do no better than pass it on.
                    unsafe { OwnedFd::from_raw_fd(fd._base as i32) }
                }
            })
        };

        let mut desc = MQDescriptor::<i8, UnsynchronizedWrite>::default();
        // SAFETY: These slices are created out of the pointer and length pairs exposed by the
        // individual descFoo accessors, so we know they are valid pointer/lengths and point to
        // data that will continue to exist as long as the desc does.
        //
        // Calls to the descFoo accessors on cpp_desc are sound because we know inner.dupeDesc
        // returns a valid pointer to a new heap-allocated AidlMQDesc.
        let (grantors, fds, ints, quantum, flags) = unsafe {
            let grantors =
                slice_from_raw_parts_or_empty(descGrantors(cpp_desc), descNumGrantors(cpp_desc));
            let fds =
                slice_from_raw_parts_or_empty(descHandleFDs(cpp_desc), descHandleNumFDs(cpp_desc));
            let ints = slice_from_raw_parts_or_empty(
                descHandleInts(cpp_desc),
                descHandleNumInts(cpp_desc),
            );
            let quantum = descQuantum(cpp_desc);
            let flags = descFlags(cpp_desc);
            (grantors, fds, ints, quantum, flags)
        };
        let fds = fds.iter().map(scoped_to_parcel_fd).collect();
        let ints = ints.to_vec();
        desc.grantors = grantors.iter().map(grantor_to_rust).collect();
        desc.handle = NativeHandle { fds, ints };
        desc.quantum = quantum;
        desc.flags = flags;
        // SAFETY: we must free the desc returned by dupeDesc; the pointer was
        // just returned above so we know it is valid.
        unsafe { freeDesc(cpp_desc) };
        desc
    }

    /// Returns the number of messages available to read
    pub fn available_to_read(&self) -> usize {
        // SAFETY: inner should point to a valid FmqWrapper
        unsafe { (*self.inner.as_ptr()).availableToRead() }
    }

    /// Checks if the fmq created by the `new` function is valid
    pub fn is_valid(&self) -> bool {
        // SAFETY: inner should point to a valid FmqWrapper
        unsafe { (*self.inner.as_ptr()).isValid() }
    }

    /// Writes `count` number of messages from the array to the fmq
    pub fn write_to_buffer(&mut self, data: &[i8], count: usize) -> bool {
        // SAFETY: inner should point to a valid FmqWrapper
        // `data` should be a non-null pointer to an array of min length `count`
        unsafe { (*self.inner.as_ptr()).writeToBuffer(data.as_ptr(), count) }
    }

    /// Reads `count` number of messages from the fmq to the provided array
    pub fn read_from_buffer(&mut self, data: &mut [i8], count: usize) -> bool {
        // SAFETY: inner should point to a valid FmqWrapper
        //`data` should be a non-null pointer to an array of min length `count`
        unsafe { (*self.inner.as_ptr()).readFromBuffer(data.as_mut_ptr(), count) }
    }

    /// Skips `count` number of messages in the fmq and acoordingly sets the read pointer
    pub fn skip_messages(&mut self, count: usize) -> bool {
        // SAFETY: inner should point to a valid FmqWrapper
        unsafe { (*self.inner.as_ptr()).skipMessages(count) }
    }
}

impl Drop for FmqWrapperRust {
    fn drop(&mut self) {
        // SAFETY: inner should point to a valid FmqWrapper
        unsafe {
            freeFmqWrapper(self.inner.as_ptr());
        }
    }
}

/// Forms a slice from a pointer and a length.
///
/// Returns an empty slice when `data` is a null pointer and `len` is zero.
///
/// # Safety
///
/// This function has the same safety requirements as [`std::slice::from_raw_parts`],
/// but unlike that function, does not exhibit undefined behavior when `data` is a
/// null pointer and `len` is zero. In this case, it returns an empty slice.
unsafe fn slice_from_raw_parts_or_empty<'a, T>(data: *const T, len: usize) -> &'a [T] {
    if data.is_null() && len == 0 {
        &[]
    } else {
        // SAFETY: The caller must guarantee to satisfy the safety requirements
        // of the standard library function [`std::slice::from_raw_parts`].
        unsafe { std::slice::from_raw_parts(data, len) }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const BUFFER_SIZE: usize = 128;
    const SKIP_COUNT: usize = 64;

    #[test]
    fn end_to_end_test() {
        let pub_buff = [0x11_i8; BUFFER_SIZE];
        let mut sub_buff = [0x00_i8; BUFFER_SIZE];

        let mut fmq = FmqWrapperRust::new(BUFFER_SIZE);

        assert!(fmq.write_to_buffer(&pub_buff, BUFFER_SIZE));
        assert_eq!(fmq.available_to_read(), BUFFER_SIZE);
        assert!(fmq.read_from_buffer(&mut sub_buff, BUFFER_SIZE));
        assert_eq!(pub_buff, sub_buff);
    }

    #[test]
    fn dupe_desc_e2e_success() {
        let pub_buff = [0x11_i8; BUFFER_SIZE];
        let mut sub_buff = [0x00_i8; BUFFER_SIZE];

        let mut fmq_pub = FmqWrapperRust::new(BUFFER_SIZE);

        let desc = fmq_pub.dupe_desc();
        let mut fmq_sub = FmqWrapperRust::from_desc(&desc, false);

        assert!(fmq_pub.write_to_buffer(&pub_buff, BUFFER_SIZE));
        assert_eq!(fmq_sub.available_to_read(), BUFFER_SIZE);
        assert!(fmq_sub.read_from_buffer(&mut sub_buff, BUFFER_SIZE));
        assert_eq!(pub_buff, sub_buff);
    }

    #[test]
    fn skip_messages_success() {
        let pub_buff = [0x11_i8; BUFFER_SIZE];
        let mut sub_buff = [0x00_i8; BUFFER_SIZE];

        let mut fmq = FmqWrapperRust::new(BUFFER_SIZE);

        assert!(fmq.write_to_buffer(&pub_buff, BUFFER_SIZE));
        assert_eq!(fmq.available_to_read(), BUFFER_SIZE);

        assert!(fmq.skip_messages(SKIP_COUNT));
        assert_eq!(fmq.available_to_read(), SKIP_COUNT);
        assert!(fmq.read_from_buffer(&mut sub_buff, SKIP_COUNT));
        assert_eq!(&pub_buff[SKIP_COUNT..], &sub_buff[..SKIP_COUNT]);
    }

    #[test]
    fn new_returns_invalid_fmq_on_zero_size() {
        let fmq = FmqWrapperRust::new(0);
        assert!(!fmq.is_valid());
    }

    #[test]
    fn new_returns_invalid_fmq_on_excessive_size() {
        const HUGE_BUFFER_SIZE: usize = 1_usize << 31;
        let fmq = FmqWrapperRust::new(HUGE_BUFFER_SIZE);
        assert!(!fmq.is_valid());
    }
}
