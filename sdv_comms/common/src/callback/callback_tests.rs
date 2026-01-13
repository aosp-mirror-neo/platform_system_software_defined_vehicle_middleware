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

use futures::StreamExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::callback::sdk::CallbackWrapper;
use crate::define_callback_ffi_struct;

const TEST_VALUE: u32 = 42;

define_callback_ffi_struct!(SimpleU32, u32);

mod test_ffi {
    use std::sync::{Arc, Mutex};

    use super::{ASDV_CallbackVTable_SimpleU32, TEST_VALUE};
    use crate::callback::ffi::Callback;

    /// Introduce a hypothetical container that holds the Callback object.
    /// In actual implementation this would be held somewhere in the FFI implementation side.
    /// We cannot use a static value here because the static callback will be accessed
    /// concurrently by multiple test-cases - this would lead to flaky tests.
    #[derive(Clone)]
    pub struct FFiContainer {
        callback: Arc<Mutex<Option<Callback<u32>>>>,
    }

    impl FFiContainer {
        pub fn new() -> FFiContainer {
            Self { callback: Arc::new(Mutex::new(None)) }
        }

        pub extern "C" fn set_callback(&self, vtable: ASDV_CallbackVTable_SimpleU32) {
            let mut opt_cb = self.callback.lock().unwrap();
            *opt_cb = Some(vtable.into());
        }

        pub extern "C" fn invoke_callback(&self) {
            let mut opt_cb = self.callback.lock().unwrap();
            let result = if let Some(ref cb) = *opt_cb {
                cb.invoke(TEST_VALUE)
            } else {
                panic!("ERR: No callback set");
            };
            if result.is_err() {
                // We should destroy the callback here.
                if let Some(cb) = opt_cb.take() {
                    drop(cb);
                }
            }
        }
    }
}

#[test]
fn test_callback() {
    type TestCallbackWrapper = CallbackWrapper<u32>;
    let ffi = test_ffi::FFiContainer::new();
    let cb_invoked = Arc::new(AtomicBool::new(false));

    let cb = TestCallbackWrapper::new({
        let cb_invoked_ref = Arc::clone(&cb_invoked);
        move |val| {
            assert_eq!(val, TEST_VALUE);
            cb_invoked_ref.store(true, Ordering::Release);
        }
    });

    ffi.set_callback(ASDV_CallbackVTable_SimpleU32 {
        func: TestCallbackWrapper::trampoline_func,
        clear: Some(TestCallbackWrapper::trampoline_clear),
        cb_data: cb.into_void_ptr(),
    });
    ffi.invoke_callback();

    assert!(cb_invoked.load(Ordering::Acquire));
}

#[tokio::test]
async fn test_callback_async_stream() {
    type TestCallbackWrapper = CallbackWrapper<u32>;
    let ffi = test_ffi::FFiContainer::new();

    let (cb, receiver) = TestCallbackWrapper::new_async();
    ffi.set_callback(ASDV_CallbackVTable_SimpleU32 {
        func: TestCallbackWrapper::trampoline_func,
        clear: Some(TestCallbackWrapper::trampoline_clear),
        cb_data: cb.into_void_ptr(),
    });

    ffi.invoke_callback();
    ffi.invoke_callback();
    ffi.invoke_callback();
    ffi.invoke_callback();
    ffi.invoke_callback();

    let val: Vec<u32> = receiver.take(5).collect().await;
    assert_eq!(val, Vec::from([TEST_VALUE; 5]));
}

#[tokio::test]
async fn test_callback_async_stream_dropped() {
    type TestCallbackWrapper = CallbackWrapper<u32>;
    let ffi = test_ffi::FFiContainer::new();
    let cb_dropped = Arc::new(AtomicBool::new(false));

    let (mut cb, receiver) = TestCallbackWrapper::new_async();
    cb.set_drop_callback({
        let cb_dropped_ref = Arc::clone(&cb_dropped);
        move || {
            cb_dropped_ref.store(true, Ordering::Release);
        }
    });
    ffi.set_callback(ASDV_CallbackVTable_SimpleU32 {
        func: TestCallbackWrapper::trampoline_func,
        clear: Some(TestCallbackWrapper::trampoline_clear),
        cb_data: cb.into_void_ptr(),
    });

    drop(receiver);
    // After we drop the receiver; if the FFI side tries to call the Callback, it will be
    // cleared instead.
    ffi.invoke_callback();

    assert!(cb_dropped.load(Ordering::Acquire));
}

#[test]
fn test_callback_ffi_without_clear_fn() {
    type TestCallbackWrapper = CallbackWrapper<u32>;
    let ffi = test_ffi::FFiContainer::new();
    let cb_invoked = Arc::new(AtomicBool::new(false));

    let cb = TestCallbackWrapper::new({
        let cb_invoked_ref = Arc::clone(&cb_invoked);
        move |val| {
            assert_eq!(val, TEST_VALUE);
            cb_invoked_ref.store(true, Ordering::Release);
        }
    });
    let cb_ptr = cb.into_void_ptr();

    ffi.set_callback(ASDV_CallbackVTable_SimpleU32 {
        func: TestCallbackWrapper::trampoline_func,
        clear: None,
        cb_data: cb_ptr, // copy the pointer; this will not be cleared by the ffi.
    });
    ffi.invoke_callback();

    assert!(cb_invoked.load(Ordering::Acquire));

    // Clear the callback stored on the ffi side.
    drop(ffi);

    // SAFETY: The cb_ptr is no longer held by the underlying Callback, so it is safe to regain
    // ownership and drop the object here.
    unsafe { TestCallbackWrapper::trampoline_clear(cb_ptr) };
}
