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

use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Notify;

use crate::callback::sdk::OnceCallbackWrapper;
use crate::define_once_callback_ffi_struct;

const TEST_VALUE: u32 = 42;

// A sample Callback VTable struct which is used to accept a callback that returns an
// [SdvResult] containing [TestStruct].
define_once_callback_ffi_struct!(TestStructCreate, Box<TestStruct>);

mod test_ffi {
    use sdv_status_rs::{SdvStatus, SdvStatusCode};
    use tokio::task;
    use tokio::time::{sleep, Duration};

    use super::{ASDV_CallbackVTable_TestStructCreate, TEST_VALUE};
    use crate::callback::ffi::OnceCallback;

    /// A sample Struct which will be passed by value across the Callback.
    #[repr(C)]
    #[derive(Debug, PartialEq)]
    pub struct TestStruct(pub u32);

    pub extern "C" fn sample_invoke_cb_ok(once_cb: ASDV_CallbackVTable_TestStructCreate) {
        let cb: OnceCallback<Box<TestStruct>> = once_cb.into();
        // Invoke the Callback after some processing elsewhere.
        task::spawn(async move {
            sleep(Duration::from_millis(200)).await;
            cb.invoke(Ok(Box::new(TestStruct(TEST_VALUE))));
        });
    }

    pub extern "C" fn sample_invoke_cb_err(once_cb: ASDV_CallbackVTable_TestStructCreate) {
        let cb: OnceCallback<Box<TestStruct>> = once_cb.into();
        // Invoke the Callback after some processing elsewhere.
        task::spawn(async move {
            sleep(Duration::from_millis(100)).await;
            cb.invoke(Err(SdvStatus::new(SdvStatusCode::Internal)));
        });
    }

    pub extern "C" fn sample_ignore_cb(once_cb: ASDV_CallbackVTable_TestStructCreate) {
        let cb: OnceCallback<Box<TestStruct>> = once_cb.into();
        // The Callback is dropped after some processing elsewhere - it is ignored due to errors
        // and never invoked.
        task::spawn(async move {
            sleep(Duration::from_millis(100)).await;
            drop(cb);
        });
    }
}

use test_ffi::TestStruct;

#[tokio::test]
async fn test_callback_async_future_ok() {
    type TestCallbackWrapper = OnceCallbackWrapper<NonNull<TestStruct>>;
    let (cb, receiver) = TestCallbackWrapper::new_async();

    let vtable = ASDV_CallbackVTable_TestStructCreate {
        func: OnceCallbackWrapper::<NonNull<TestStruct>>::trampoline_func,
        clear: Some(OnceCallbackWrapper::<NonNull<TestStruct>>::trampoline_clear),
        cb_data: cb.into_void_ptr(),
    };
    test_ffi::sample_invoke_cb_ok(vtable);
    let val = receiver.await.unwrap();

    assert!(val.is_ok());
    // SAFETY: [OnceCallbackWrapper] guarantees that the pointer is valid.
    let val = unsafe { val.unwrap().as_ref() };
    assert_eq!(*val, test_ffi::TestStruct(TEST_VALUE));
}

#[tokio::test]
async fn test_callback_async_future_err() {
    type TestCallbackWrapper = OnceCallbackWrapper<NonNull<TestStruct>>;
    let (cb, receiver) = TestCallbackWrapper::new_async();

    let vtable = ASDV_CallbackVTable_TestStructCreate {
        func: OnceCallbackWrapper::<NonNull<TestStruct>>::trampoline_func,
        clear: Some(OnceCallbackWrapper::<NonNull<TestStruct>>::trampoline_clear),
        cb_data: cb.into_void_ptr(),
    };
    test_ffi::sample_invoke_cb_err(vtable);
    let val = receiver.await.unwrap();

    assert!(val.is_err());
}

#[tokio::test]
async fn test_callback_async_future_dropped() {
    type TestCallbackWrapper = OnceCallbackWrapper<NonNull<TestStruct>>;
    let cb_dropped = Arc::new(AtomicBool::new(false));
    let notify = Arc::new(Notify::new());
    let (mut cb, receiver) = TestCallbackWrapper::new_async();
    cb.set_drop_callback({
        let cb_dropped_ref = Arc::clone(&cb_dropped);
        let notify_ref = Arc::clone(&notify);
        move || {
            cb_dropped_ref.store(true, Ordering::Release);
            notify_ref.notify_one();
        }
    });

    let vtable = ASDV_CallbackVTable_TestStructCreate {
        func: OnceCallbackWrapper::<NonNull<TestStruct>>::trampoline_func,
        clear: Some(OnceCallbackWrapper::<NonNull<TestStruct>>::trampoline_clear),
        cb_data: cb.into_void_ptr(),
    };
    test_ffi::sample_invoke_cb_ok(vtable);
    drop(receiver);

    notify.notified().await;
    assert!(cb_dropped.load(Ordering::Acquire));
}

#[tokio::test]
async fn test_callback_async_future_unused_dropped() {
    type TestCallbackWrapper = OnceCallbackWrapper<NonNull<TestStruct>>;
    let cb_dropped = Arc::new(AtomicBool::new(false));
    let notify = Arc::new(Notify::new());
    let (mut cb, _receiver) = TestCallbackWrapper::new_async();
    cb.set_drop_callback({
        let cb_dropped_ref = Arc::clone(&cb_dropped);
        let notify_ref = Arc::clone(&notify);
        move || {
            cb_dropped_ref.store(true, Ordering::Release);
            notify_ref.notify_one();
        }
    });

    let vtable = ASDV_CallbackVTable_TestStructCreate {
        func: OnceCallbackWrapper::<NonNull<TestStruct>>::trampoline_func,
        clear: Some(OnceCallbackWrapper::<NonNull<TestStruct>>::trampoline_clear),
        cb_data: cb.into_void_ptr(),
    };
    test_ffi::sample_ignore_cb(vtable);

    notify.notified().await;
    assert!(cb_dropped.load(Ordering::Acquire));
}

#[tokio::test]
async fn test_callback_ffi_without_clear_fn() {
    type TestCallbackWrapper = OnceCallbackWrapper<NonNull<TestStruct>>;
    let (cb, _receiver) = TestCallbackWrapper::new_async();
    let cb_ptr = cb.into_void_ptr();

    let vtable = ASDV_CallbackVTable_TestStructCreate {
        func: OnceCallbackWrapper::<NonNull<TestStruct>>::trampoline_func,
        clear: None,
        cb_data: cb_ptr, // copy the pointer; this will not be cleared by the ffi.
    };

    test_ffi::sample_ignore_cb(vtable);

    // SAFETY: The cb_ptr is no longer held by the underlying Callback, so it is safe to regain
    // ownership and drop the object here.
    unsafe { TestCallbackWrapper::trampoline_clear(cb_ptr) };
}
