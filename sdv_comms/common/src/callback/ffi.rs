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

//! Helper library on the Rust FFI side of the Comms Stack for Callback management.
//! Users of the API may use it in one of the following ways. See sdk.rs for the SDK side of
//! Callback Management.
//!
//! Example usage (using callback that can be invoked multiple times):
//! ```
//! use callback::ffi::Callback;
//!
//! // Create a VTable for receiving the Callback through the FFI.
//! // This creates the struct ASDV_CallbackVTable_MyExampleCallback.
//! define_callback_ffi_struct!(MyExampleCallback, Box<ExampleValue>);
//!
//! pub extern "C" fn example(vtable: ASDV_CallbackVTable_MyExampleCallback) {
//!     let cb: Callback<Box<ExampleValue>> = vtable.into();
//!
//!     // Call invoke on the Callback struct multiple times.
//!     for (...) {
//!         cb.invoke(Box::new( ... ));
//!     }
//!
//!     // When the callback object is dropped; the SDK side receiver will stop being invoked.
//!     drop(cb);
//! }
//! ```
//!
//! Example usage (using callback that can be invoked only once):
//! ```
//! use callback::ffi::OnceCallback;
//!
//! // Create a VTable for receiving the Callback through the FFI.
//! // This creates the struct ASDV_CallbackVTable_MyExampleCallback.
//! define_once_callback_ffi_struct!(MyExampleCallback, Box<ExampleValue>);
//!
//! pub extern "C" fn example(vtable: ASDV_CallbackVTable_MyExampleCallback) {
//!     let cb: OnceCallback<Box<ExampleValue>> = vtable.into();
//!
//!     // Call invoke on the Callback struct exactly once.
//!     cb.invoke(Box::new( ... ));
//! }
//! ```
//!
//! TODO(b/357912905): Handle panics across the callback if `cfg!(not(panic = "abort"))` to
//! prevent undefined behaviour across the C ABI boundary.

use sdv_status_rs::{sdv_status_t, SdvResult, SdvStatus, SdvStatusCode};
use std::ffi::c_void;

/// An FFI side wrapper for a Callback.
///
// LINT.IfChange(callback_vtable_docs)
/// This includes a callback function `func`, a `clear` function and some `cb_data` pointer for
/// invoking the two functions. All the pointers here are owned by the Callback struct and are
/// released automatically when the Callback object is dropped.
///
/// # SAFETY
/// - `func` will be legal to call on the provided `cb_data` and any `T` an arbitrary number of
///   times so long as `clear` has not been called on `cb_data`.
/// - `clear` may be called on `cb_data` at most once. After `clear` has been called using
///   `cb_data`, calling `func` on the same `cb_data` is undefined behaviour.
/// - `func` and `clear` must be callable on any thread, though the user may assume that they
///   are not in use on multiple threads at the same time.
// LINT.ThenChange(/sdv_comms/common/include_c_abi/current/sdv/callback.h:callback_vtable_docs)
#[repr(C)]
pub struct Callback<T> {
    func: unsafe extern "C" fn(user_data: *mut std::ffi::c_void, val: T) -> sdv_status_t,
    clear: Option<unsafe extern "C" fn(cb_data: *mut std::ffi::c_void)>,
    cb_data: *mut std::ffi::c_void,
}

// SAFETY: It is safe to move the Callback object across threads because it contains external pointers.
unsafe impl<T> Send for Callback<T> {}

impl<T> Callback<T> {
    /// Create a new Callback object based on the functions.
    /// Ownership of the pointers is transferred into the returned [Callback] object.
    ///
    /// See Safety section on [Callback] for details about safety expectations of the provided
    /// function pointers.
    pub fn new(
        func: unsafe extern "C" fn(user_data: *mut std::ffi::c_void, val: T) -> sdv_status_t,
        clear: Option<unsafe extern "C" fn(cb_data: *mut std::ffi::c_void)>,
        cb_data: *mut std::ffi::c_void,
    ) -> Callback<T> {
        Callback { func, clear, cb_data }
    }

    /// Invoke the Callback with the provided args.
    pub fn invoke(&self, val: T) -> SdvResult<()> {
        // SAFETY: Calling `func` using `cb_data` is safe.
        let status_code = unsafe { (self.func)(self.cb_data, val) };
        SdvStatus::from_raw_code(status_code).into()
    }
}

impl<T> Drop for Callback<T> {
    fn drop(&mut self) {
        if let Some(clear) = self.clear {
            // SAFETY: Calling `clear` using `cb_data` is safe. Since the Callback object is
            // destroyed after this function completes, `func` can never be called again.
            unsafe { (clear)(self.cb_data) };
        }
    }
}

// LINT.IfChange(callback_vtable)
/// Declare a struct that can be used to receive a callback from the FFI side.
///
/// This declaration MUST be kept in sync with the sdv/callback.h macro DEFINE_CALLBACK.
///
/// Safety:
/// Passing pointers of $value_type must be an FFI safe operation.
///
/// NOTE: We need to do this and cannot use Callback<T> directly because Rust does not
/// consider generic structs as FFI safe.
#[macro_export]
macro_rules! define_callback_ffi_struct {
    ( $name:ident, $value_type: ty ) => {
        $crate::callback::ffi::_macro_exports::paste! {
            #[repr(C)]
            pub struct [<ASDV_CallbackVTable_ $name>] {
                func: unsafe extern "C" fn(cb_data: *mut ::std::ffi::c_void, val: $value_type) -> $crate::callback::ffi::_macro_exports::sdv_status_t,
                clear: Option<unsafe extern "C" fn(cb_data: *mut std::ffi::c_void)>,
                cb_data: *mut std::ffi::c_void,
            }

            impl [<ASDV_CallbackVTable_ $name>] {
                #[inline]
                pub fn into(mut self) -> $crate::callback::ffi::Callback<$value_type> {
                    let clear = self.clear.take();
                    $crate::callback::ffi::Callback::new(self.func, clear, self.cb_data)
                }
            }

            impl Drop for [<ASDV_CallbackVTable_ $name>] {
                fn drop(&mut self) {
                    if let Some(clear) = self.clear {
                        // SAFETY: It is safe to call clear on the cb_data if it is not `null` to
                        // avoid a memory leak that might occur when the CallbackYTable object is
                        // dropped without converting into a Callback object.
                        unsafe { (clear)(self.cb_data) };
                    }
                }
            }
        }
    };
}
// LINT.ThenChange(/sdv_comms/common/include_c_abi/current/sdv/callback.h:callback_vtable)

/// Trait to mark types as eligible to be used in a [OnceCallback].
///
/// # Safety
/// Implementing types need to declare the FFI safe type that represents Self in FFI
/// functions (extern "C" fn). [OnceCallback] transfers ownership of the sent value, hence
/// `OnceCallback::into_ffi_value` must be implemented to represent transfer of ownership.
pub unsafe trait OnceCallbackSendable {
    /// The type that will be used as the FFI equivalent of the current type.
    type FfiType;

    /// Converts a value of [Self] type to the FfiType.
    fn into_ffi_value(self) -> Self::FfiType;

    /// Provides a null value for the FfiType used when the OnceCallback returns an error.
    fn ffi_null_value() -> Self::FfiType;
}

/// Implementation of OnceCallbackSendable for owned pointers. The pointer ownership is
/// transferred using `Box::into_raw`.
///
/// SAFETY: It is safe to transfer ownership of pointers using `Box::into_raw`.
unsafe impl<T> OnceCallbackSendable for Box<T> {
    type FfiType = *mut T;

    fn into_ffi_value(self) -> Self::FfiType {
        Box::into_raw(self)
    }

    fn ffi_null_value() -> Self::FfiType {
        std::ptr::null_mut()
    }
}

macro_rules! impl_once_callback_sendable_for_primitive_types {
    ($($t:ty),*) => {
        $(
            /// SAFETY: It is safe to transfer ownership of primitive types i.e. pass primitive
            /// types by value.
            unsafe impl OnceCallbackSendable for $t {
                type FfiType = $t;

                fn into_ffi_value(self) -> Self::FfiType {
                    self
                }

                fn ffi_null_value() -> Self::FfiType {
                    Default::default()
                }
            }
        )*
    }
}

impl_once_callback_sendable_for_primitive_types!(u8, u16, u32, u64, usize, i8, i16, i32, i64, bool);

/// An FFI side wrapper for a OnceCallback.
///
// LINT.IfChange(once_callback_vtable_docs)
/// This includes a callback function `func`, a `clear` function and some `cb_data` pointer for
/// invoking the two functions. All the pointers here are owned by the Callback struct and are
/// released automatically when the Callback object is dropped.
///
/// Note that the OnceCallbackWrapper requests ownership of the `cb_data` pointer when `func`
/// or `clear` is invoked. Thus only one of them may ever be invoked; but not both.
///
/// # SAFETY
/// - `func` may be called on `cb_data` and any `T` at most once. After `func` has been called
///   using `cb_data` calling `func` or `clear` on the same `cb_data` is undefined behaviour.
/// - `clear` may be called on `cb_data` at most once. After `clear` has been called using
///   `cb_data`, calling `func` or `clear` on the same `cb_data` is undefined behaviour.
/// - `func` and `clear` must be callable on any thread, though the user may assume that they
///   are not in use on multiple threads at the same time.
// LINT.ThenChange(/sdv_comms/common/include_c_abi/current/sdv/callback.h:once_callback_vtable_docs)
pub struct OnceCallback<T: OnceCallbackSendable> {
    func: unsafe extern "C" fn(cb_data: *mut c_void, val: T::FfiType, status: sdv_status_t),
    clear: Option<unsafe extern "C" fn(cb_data: *mut c_void)>,
    cb_data: *mut c_void,
}

// SAFETY: It is safe to move the [OnceCallback] object across threads because it contains
// external pointers, and owns them.
unsafe impl<T: OnceCallbackSendable> Send for OnceCallback<T> {}

impl<T: OnceCallbackSendable> OnceCallback<T> {
    /// Create a new OnceCallback object based on the functions.
    /// Ownership of the pointers is transferred into the returned [OnceCallback] object.
    ///
    /// See Safety section on [OnceCallback] for details about safety expectations of the
    /// provided function pointers.
    pub fn new(
        func: unsafe extern "C" fn(cb_data: *mut c_void, val: T::FfiType, status: sdv_status_t),
        clear: Option<unsafe extern "C" fn(cb_data: *mut c_void)>,
        cb_data: *mut c_void,
    ) -> OnceCallback<T> {
        OnceCallback { func, clear, cb_data }
    }

    /// Invoke the Callback with the provided args.
    pub fn invoke(mut self, result: SdvResult<T>) {
        let (status, val) = match result {
            Ok(val) => (SdvStatusCode::Ok, val.into_ffi_value()),
            Err(status) => {
                log::error!("Callback invocation failed with: {status}");
                (status.status_code(), T::ffi_null_value())
            }
        };
        // SAFETY: Calling `func` using `cb_data` is safe as guaranteed by the SAFETY comment
        // on the struct [OnceCallback].
        unsafe { (self.func)(self.cb_data, val, status.into_raw()) };

        // Prevent `clear` from being invoked on this callback.
        self.clear = None;
    }
}

impl<T: OnceCallbackSendable> Drop for OnceCallback<T> {
    fn drop(&mut self) {
        if let Some(clear) = self.clear {
            // SAFETY: Calling `clear` using `cb_data` is safe. Since the [OnceCallback] object
            // is destroyed after this function completes, `func` can never be called again.
            unsafe { (clear)(self.cb_data) };
        }
    }
}

// LINT.IfChange(once_callback_vtable)
/// Declare a struct that can be used to receive a once_callback from the FFI side.
///
/// This declaration MUST be kept in sync with the sdv/callback.h macro DEFINE_ONCE_CALLBACK.
///
/// NOTE: We need to do this and cannot use OnceCallback<T> directly because Rust does not
/// consider generic structs as FFI safe.
#[macro_export]
macro_rules! define_once_callback_ffi_struct {
    ( $name:ident, $value_type: ty ) => {
        $crate::callback::ffi::_macro_exports::assert_impl_all!($value_type: $crate::callback::ffi::OnceCallbackSendable);
        $crate::callback::ffi::_macro_exports::paste! {
            #[repr(C)]
            pub struct [<ASDV_CallbackVTable_ $name>] {
                func: unsafe extern "C" fn(
                    cb_data: *mut ::std::ffi::c_void,
                    val: <$value_type as $crate::callback::ffi::OnceCallbackSendable>::FfiType,
                    status: $crate::callback::ffi::_macro_exports::sdv_status_t),
                clear: Option<unsafe extern "C" fn(cb_data: *mut std::ffi::c_void)>,
                cb_data: *mut std::ffi::c_void,
            }

            impl [<ASDV_CallbackVTable_ $name>] {
                #[inline]
                pub fn into(mut self) -> $crate::callback::ffi::OnceCallback<$value_type> {
                    let clear = self.clear.take();
                    $crate::callback::ffi::OnceCallback::new(self.func, clear, self.cb_data)
                }
            }

            impl Drop for [<ASDV_CallbackVTable_ $name>] {
                fn drop(&mut self) {
                    if let Some(clear) = self.clear {
                        // SAFETY: It is safe to call clear on the cb_data if it is not `null` to
                        // avoid a memory leak that might occur when the CallbackYTable object is
                        // dropped without converting into a OnceCallback object.
                        unsafe { (clear)(self.cb_data) };
                    }
                }
            }
        }
    };
}
// LINT.ThenChange(/sdv_comms/common/include_c_abi/current/sdv/callback.h:once_callback_vtable)

#[doc(hidden)]
pub mod _macro_exports {
    pub use paste::paste;
    pub use sdv_status_rs::sdv_status_t;
    pub use static_assertions::assert_impl_all;
}
