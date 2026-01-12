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

//! Helper library on the Rust SDK side of the Comms Stack for Callback management.
//! Users of the API may use it in one of the following ways. See ffi.rs for the SDK side of
//! Callback Management.
//!
//! Example usage (using Rust closures):
//! ```
//! use callback::CallbackWrapper;
//!
//! type MyCallback = CallbackWrapper<...>;
//!
//! pub fn exec_(f: impl FnMut(T) + 'static) {
//!     let cb = MyCallback::new(f);
//!     some_ffi::set_callback(
//!         MyCallback::trampoline_func, MyCallback::trampoline_clear, cb.into_void_ptr());
//! }
//! ```
//!
//! Example usage (using Rust Futures):
//! ```
//! use callback::OnceCallbackWrapper;
//! use mycodebase_bindgen::CallbackVTable_MyCallback;
//!
//! type MyCallback = OnceCallbackWrapper<...>;
//!
//! pub async fn exec_() {
//!     let (cb, fut) = MyCallback::new_async();
//!     let vtable = CallbackVTable_MyCallback {
//!         func: MyCallback::trampoline_func,
//!         clear: MyCallback::trampoline_clear,
//!         cb_data: cb.into_void_ptr(),
//!     };
//!
//!     some_ffi::set_callback(vtable);
//!     fut.await
//! }
//! ```
//!
//! Example usage (using Rust Streams):
//! ```
//! use callback::CallbackWrapper;
//!
//! type MyCallback = CallbackWrapper<...>;
//!
//! pub async fn exec_() {
//!     let (cb, stream) = MyCallback::new_async();
//!     some_ffi::set_callback(
//!         MyCallback::trampoline_func, MyCallback::trampoline_clear, cb.into_void_ptr());
//!     stream.next().await
//! }
//! ```
//!
//! TODO(b/357912905): Handle panics across the callback if `cfg!(not(panic = "abort"))` to
//! prevent undefined behaviour across the C ABI boundary.

use cfg_if::cfg_if;
use futures::channel::{mpsc, oneshot};
use sdv_status_rs::{sdv_status_t, SdvResult, SdvStatus, SdvStatusCode};
use std::ffi::c_void;
use std::ptr::NonNull;

use crate::callback::MaybeUsed;

/// A Wrapper type for a Callback passed across the FFI that is can be invoked multiple times.
///
/// Note that it is required to introduce additional Box indirection to pass just the `dyn FnMut`
/// pointer to the trampoline. `dyn FnMut` is a trait object. A pointer to it is a "fat pointer"
/// (vtable pointer + data pointer), which does not fit in a single `c_void` pointer.
/// See https://stackoverflow.com/a/57754902
pub struct CallbackWrapper<T> {
    handler: Box<dyn FnMut(T) -> SdvResult<()> + 'static>,

    // Observer for ensuring the [CallbackWrapper] is dropped appropriately.
    // Note that this value is available for tests only, because we want to test that the object
    // is properly dropped, and never leaked.
    #[cfg(test)]
    drop_cb: Option<Box<dyn FnOnce() + Send + 'static>>,
}

impl<T> CallbackWrapper<T> {
    /// Creates a new [CallbackWrapper] for the provided closure.
    #[inline]
    fn new_internal(f: impl FnMut(T) -> SdvResult<()> + 'static) -> Box<CallbackWrapper<T>> {
        let handler = Box::new(f);
        cfg_if! {if #[cfg(test)] {
            Box::new(CallbackWrapper { handler, drop_cb: None })
        } else {
            Box::new(CallbackWrapper { handler })
        }}
    }

    /// Creates a new [CallbackWrapper] for the provided closure.
    #[inline]
    pub fn new(mut f: impl FnMut(T) + Send + 'static) -> Box<CallbackWrapper<T>> {
        Self::new_internal(move |t| {
            f(t);
            Ok(())
        })
    }

    /// Set a callback to be triggered when this [CallbackWrapper] is destroyed.
    #[cfg(test)]
    pub fn set_drop_callback(&mut self, cb: impl FnOnce() + Send + 'static) {
        self.drop_cb = Some(Box::new(cb));
    }

    /// Converts the calling [CallbackWrapper] into a [c_void] pointer by transferring ownership.
    #[inline]
    pub fn into_void_ptr(self: Box<CallbackWrapper<T>>) -> *mut c_void {
        Box::into_raw(self) as *mut c_void
    }

    /// The C FFI function to use as a function pointer for registering a callback.
    /// The other side of the C FFI can call this function with a data pointer provided
    /// by the `CallbackWrapper::into_void_ptr`.
    ///
    /// # Safety
    /// - `cb_data` must be created by `CallbackWrapper::into_void_ptr`.
    /// - this function must not be called after `trampoline_clear` has been called.
    pub unsafe extern "C" fn trampoline_func(cb_data: *mut c_void, val: T) -> sdv_status_t {
        // SAFETY: It is expected that the `cb_data` pointer will always be the same
        // pointer as the one obtained from `CallbackWrapper::into_raw_ptr`.
        let opt_wrapper = unsafe { cb_data.cast::<CallbackWrapper<T>>().as_mut() };
        if let Some(wrapper) = opt_wrapper {
            match (wrapper.handler)(val) {
                Ok(()) => SdvStatusCode::Ok.into_raw(),
                Err(status) => {
                    if status.status_code() != SdvStatusCode::ResourceExhausted {
                        log::warn!(
                            "CallbackWrapper::handle returned error. \
                            trampoline_func() will return status: {status}"
                        );
                    }
                    status.status_code().into_raw()
                }
            }
        } else {
            log::warn!(
                "trampoline_func() called with null for `cb_data`. This should never happen."
            );
            SdvStatusCode::Internal.into_raw()
        }
    }

    /// The C FFI function to use as a function pointer for registering a callback.
    /// The other side of the C FFI can call this function with a data pointer provided
    ///
    /// # Safety
    /// - `cb_data` must be created by `CallbackWrapper::into_void_ptr`.
    /// - this function must be called exactly once for any specific `cb_data` to clear
    ///   the allocations of `cb_data`.
    pub unsafe extern "C" fn trampoline_clear(cb_data: *mut c_void) {
        // SAFETY: It is expected that the `cb_data` pointer will always be the same
        // pointer as the one obtained from `CallbackWrapper::into_raw_ptr`.
        let wrapper = unsafe { Box::<CallbackWrapper<T>>::from_raw(cb_data.cast()) };
        drop(wrapper);
    }
}

impl<T: 'static> CallbackWrapper<T> {
    /// Creates a new [CallbackWrapper] which can be used to emulate async functions that may be
    /// called multiple times. Returns a [mpsc::Receiver] which can be used as a [futures::Stream].
    ///
    /// # Safety
    /// - `unset_cb` MUST trigger the FFI call to unset the callback, which MUST in turn call the
    ///   `clear` function registered with the callback.
    /// - `unset_cb` may be called multiple times if the FFI side attempts to call the callback
    ///   anytime after the returned receiver is dropped.
    #[inline]
    pub fn new_async() -> (Box<CallbackWrapper<T>>, mpsc::Receiver<T>) {
        // TODO(b/434266286): To implement the callback mechanism for DT, it is not necessary to
        // create a channel with a buffer of 1024 messages. A single message buffer should be
        // sufficient.
        let (mut sender, receiver) = mpsc::channel(1024);
        let cb_wrapper = Self::new_internal(move |val| {
            sender.try_send(val).map_err(|e| {
                if e.is_full() {
                    // TODO: This error has different meanings for DT and SD and requires different
                    // reactions. In the future it's recommended to use separate CallbackWrapper
                    // implementations.
                    SdvStatus::new(SdvStatusCode::ResourceExhausted)
                } else {
                    log::warn!("Async Stream send error: {e}");
                    SdvStatus::new(SdvStatusCode::Cancelled)
                }
            })
        });
        (cb_wrapper, receiver)
    }
}

#[cfg(test)]
/// This implementation can be used to detect when the CallbackWrapper is dropped.
impl<T> Drop for CallbackWrapper<T> {
    fn drop(&mut self) {
        // Execute the drop callback if one exists
        if let Some(drop_cb) = self.drop_cb.take() {
            drop_cb();
        }
    }
}

/// Trait that mark types as eligible to be used in a [OnceCallback].
///
/// # Safety
/// Implementing types need to declare the FFI safe type that represents Self in FFI
/// functions (extern "C" fn). [OnceCallbackWrapper] takes ownership of the received value,
/// hence OnceCallback::from_ffi_value must be implemented to represent transfer of ownership.
pub unsafe trait OnceCallbackReceivable: Sized {
    /// The type that will be used as the FFI equivalent of the current type.
    type FfiType;

    /// Converts a value of [Self::FfiType] type to the [Self] type.
    fn from_ffi_value(val: Self::FfiType) -> SdvResult<Self>;
}

/// Implementation of [OnceCallbackReceivable] for an owned pointer. The owned pointer is
/// represented as a std::ptr::NonNull<T>.
///
/// SAFETY: It is safe to transfer ownership of a `*mut T` to a `NonNull<T>`.
unsafe impl<T> OnceCallbackReceivable for NonNull<T> {
    type FfiType = *mut T;

    fn from_ffi_value(val: Self::FfiType) -> SdvResult<Self> {
        NonNull::new(val).ok_or(SdvStatus::new(SdvStatusCode::DataLoss))
    }
}

macro_rules! impl_once_callback_receivable_for_primitive_types {
    ($($t:ty),*) => {
        $(
            /// SAFETY: It is safe to transfer ownership of primitive types i.e. pass primitive
            /// types by value.
            unsafe impl OnceCallbackReceivable for $t {
                type FfiType = $t;

                fn from_ffi_value(val: Self::FfiType) -> SdvResult<Self> {
                    Ok(val)
                }
            }
        )*
    }
}

impl_once_callback_receivable_for_primitive_types!(
    u8, u16, u32, u64, usize, i8, i16, i32, i64, bool
);

/// A Wrapper type for a Callback passed across the FFI that is promised to be invoked only once.
pub struct OnceCallbackWrapper<T: OnceCallbackReceivable> {
    // Sender for the callback.
    sender: MaybeUsed<oneshot::Sender<SdvResult<T>>>,

    // Observer for ensuring the [CallbackWrapper] is dropped appropriately.
    // Note that this value is available for tests only, because we want to test that the object
    // is properly dropped, and never leaked.
    #[cfg(test)]
    drop_cb: Option<Box<dyn FnOnce() + Send + 'static>>,
}

impl<T: OnceCallbackReceivable> OnceCallbackWrapper<T> {
    /// Creates a new [OnceCallbackWrapper] and a corresponding future that can be awaited on.
    #[inline]
    pub fn new_async() -> (Box<OnceCallbackWrapper<T>>, oneshot::Receiver<SdvResult<T>>) {
        let (sender, receiver) = oneshot::channel();
        cfg_if! {if #[cfg(test)] {
            (Box::new(Self { sender: MaybeUsed::new_unused(sender), drop_cb: None }), receiver)
        } else {
            (Box::new(Self { sender: MaybeUsed::new_unused(sender) }), receiver)
        }}
    }

    /// Converts the calling [CallbackWrapper] into a [c_void] pointer by transferring ownership.
    ///
    /// Note that we need to transfer ownership here because the trampoline_func consumes the
    /// `cb_data` ptr when it sends through the oneshot::Sender.
    #[inline]
    pub fn into_void_ptr(self: Box<OnceCallbackWrapper<T>>) -> *mut c_void {
        Box::into_raw(self) as *mut c_void
    }

    /// Set a callback to be triggered when this [CallbackWrapper] is destroyed.
    #[cfg(test)]
    pub fn set_drop_callback(&mut self, cb: impl FnOnce() + Send + 'static) {
        self.drop_cb = Some(Box::new(cb));
    }

    /// The C FFI function to use as a function pointer for registering the callback.
    /// The other side of the C FFI can call this function with a data pointer provided
    /// by the `OnceCallbackWrapper::into_void_ptr`.
    ///
    /// # Safety
    /// - `cb_data` must be created by `OnceCallbackWrapper::into_void_ptr`.
    /// - this function must only be called at most once; `cb_data` is destroyed after this
    ///   function completes.
    /// - this function must NOT be called if `trampoline_clear` has been called.
    /// - the `val` pointer must NOT be null if `status` is ASDV_STATUS_OK, and must be obtained
    ///   from converting a Boxed pointer i.e. `Box::into_raw`.
    pub unsafe extern "C" fn trampoline_func(
        cb_data: *mut c_void,
        val: T::FfiType,
        status: sdv_status_t,
    ) {
        // SAFETY: It is expected that the `cb_data` pointer will always be the same
        // pointer as the one obtained from `OnceCallbackWrapper::into_raw_ptr`.
        let mut wrapper = unsafe { Box::<OnceCallbackWrapper<T>>::from_raw(cb_data.cast()) };
        // It is panic-safe to call MaybeUsed::take() here, since this function takes ownership
        // of the `wrapper` so it will not be used again, and the caller guarantees that this
        // function will be called with a given `cb_data` only once.
        let sender = wrapper.sender.take();

        let result = match SdvStatusCode::from(status) {
            SdvStatusCode::Ok => T::from_ffi_value(val),
            sdv_code => Err(SdvStatus::new(sdv_code)),
        };

        if sender.send(result).is_err() {
            log::warn!(
                "OnceCallbackWrapper::sender returned error. This is likely because the \
                receiver has already been destroyed."
            );
        }

        // Explicitly call drop here to indicate that the value is destroyed.
        drop(wrapper);
    }

    /// The C FFI function to use as a function pointer for registering a callback.
    /// The other side of the C FFI can call this function with a data pointer provided
    /// by the `OnceCallbackWrapper::into_void_ptr`.
    ///
    /// # Safety
    /// - `cb_data` must be created by `CallbackWrapper::into_void_ptr`.
    /// - this function must be called exactly once for any specific `cb_data` to clear
    ///   the allocations of `cb_data`.
    /// - this function must not be called if `trampoline func` has been called.
    pub unsafe extern "C" fn trampoline_clear(cb_data: *mut c_void) {
        // SAFETY: It is expected that the `cb_data` pointer will always be the same
        // pointer as the one obtained from `CallbackWrapper::into_raw_ptr`.
        let wrapper = unsafe { Box::<OnceCallbackWrapper<T>>::from_raw(cb_data.cast()) };

        // Explicitly call drop here to indicate that the value is destroyed.
        drop(wrapper);
    }
}

#[cfg(test)]
/// This implementation can be used to detect when the CallbackWrapper is dropped.
impl<T: OnceCallbackReceivable> Drop for OnceCallbackWrapper<T> {
    fn drop(&mut self) {
        // Execute the drop callback if one exists
        if let Some(drop_cb) = self.drop_cb.take() {
            drop_cb();
        }
    }
}
