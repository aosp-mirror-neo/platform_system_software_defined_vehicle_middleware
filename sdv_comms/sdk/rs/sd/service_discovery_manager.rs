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

use bytemuck::TransparentWrapper;
use futures::stream::{Stream, StreamExt};
use sdv_comms_bindgen::ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL;
use sdv_comms_bindgen::{
    ASDV_CallbackVTable_ServiceDiscoveryManager_GetUnit,
    ASDV_CallbackVTable_ServiceDiscoveryManager_ListUnitsByName,
    ASDV_CallbackVTable_ServiceDiscoveryManager_ListUnitsByType,
    ASDV_CallbackVTable_ServiceDiscoveryManager_NotifyListenerOfServiceUnitChange,
    ASDV_CallbackVTable_ServiceDiscoveryManager_RegisterServiceUnitChangeListener,
    ASDV_ServiceDiscoveryManager_registerServiceUnitChangeListenerByName,
    ASDV_ServiceDiscoveryManager_registerServiceUnitChangeListenerByType,
    ASDV_ServiceUnitChangeEvent, ASDV_ServiceUnitChangeListenerHandle, ASDV_ServiceUnitDefinition,
};
use sdv_comms_common::callback::sdk::{
    CallbackWrapper, OnceCallbackReceivable, OnceCallbackWrapper,
};
use sdv_comms_common::ffi_str;
use sdv_comms_common::owned_opaque_ptr::OwnedOpaquePtr;
use sdv_status_rs::{derive, SdvResult, SdvStatusCode};
use std::ffi::c_void;

use crate::id::ServiceFqin;
use crate::sd::{ServiceUnitDefinition, UnitNameDiscoveryArgs, UnitType};
use crate::ContextRef;

use crate::sd::UnitChangeEventStream;

/// Entry point for all SDV Service Discovery APIs.
///
/// Instances of [ServiceDiscoveryManager] are cheap to create so may be created as often as
/// needed. Additionally instances are expected to be both [Send] (passable to different threads)
/// and Sync (callable from multiple threads).
pub struct ServiceDiscoveryManager {
    ctx: ContextRef,
}

impl ServiceDiscoveryManager {
    /// Creates a new instance of [ServiceDiscoveryManager].
    pub fn new(ctx: ContextRef) -> Self {
        ServiceDiscoveryManager { ctx }
    }

    /// Runs a Service Discovery using the provided [UnitType]. Returns a list of
    /// [ServiceUnitDefinition] of service units that were registered with the provided [UnitType].
    pub fn list_units_by_type(
        &self,
        unit_type: &UnitType,
    ) -> impl Stream<Item = ServiceUnitDefinition> + Send {
        type ListUnitsByTypeCb = CallbackWrapper<OwnedOpaquePtr<ASDV_ServiceUnitDefinition>>;

        unsafe extern "C" fn trampoline_func(
            cb_data: *mut c_void,
            ptr: *mut ASDV_ServiceUnitDefinition,
        ) -> i32 {
            // SAFETY: `cb_data` and `ptr` are valid pointers that are passed to
            // ASDV_ServiceDiscoveryManager_listUnitsByType.
            unsafe {
                ListUnitsByTypeCb::trampoline_func(
                    cb_data,
                    OwnedOpaquePtr::from_ffi_value(ptr)
                        .expect("Pointer returned by listUnitsByType should be valid"),
                )
            }
        }

        let (cb, stream) = ListUnitsByTypeCb::new_async();
        let vtable = ASDV_CallbackVTable_ServiceDiscoveryManager_ListUnitsByType {
            func: Some(trampoline_func),
            clear: Some(ListUnitsByTypeCb::trampoline_clear),
            cb_data: cb.into_void_ptr(),
        };

        // SAFETY: `ctx`, `fqin` and `unit_name` are valid pointers to corresponding types.
        // `vtable` represents the correct Callback Vtable: ASDV_CallbackVTable_ServiceDiscoveryManager_GetUnit.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServiceDiscoveryManager_listUnitsByType(
                self.ctx.as_raw(),
                UnitType::peel_ref(unit_type),
                vtable,
            )
        };

        // TODO(b/361742242): Handle errors returned from the function call more gracefully.
        SdvStatusCode::from(status)
            .expect("Call to ASDV_ServiceDiscoveryManager_listUnitsByType failed.");

        stream.map(ServiceUnitDefinition)
    }

    /// Runs a Service Discovery using the provided [UnitNameDiscoveryArgs]. Returns a list of
    /// [ServiceUnitDefinition] of service whose registration match given [UnitNameDiscoveryArgs].
    pub fn list_units_by_name(
        &self,
        name_args: &UnitNameDiscoveryArgs,
    ) -> impl Stream<Item = ServiceUnitDefinition> + Send {
        type ListUnitsByNameCb = CallbackWrapper<OwnedOpaquePtr<ASDV_ServiceUnitDefinition>>;

        unsafe extern "C" fn trampoline_func(
            cb_data: *mut c_void,
            ptr: *mut ASDV_ServiceUnitDefinition,
        ) -> i32 {
            // SAFETY: `cb_data` and `ptr` are valid pointers that are passed to
            // ASDV_ServiceDiscoveryManager_listUnitsByName.
            unsafe {
                ListUnitsByNameCb::trampoline_func(
                    cb_data,
                    OwnedOpaquePtr::from_ffi_value(ptr)
                        .expect("Pointer returned by list_units_by_name should be valid"),
                )
            }
        }

        let (cb, stream) = ListUnitsByNameCb::new_async();
        let vtable = ASDV_CallbackVTable_ServiceDiscoveryManager_ListUnitsByName {
            func: Some(trampoline_func),
            clear: Some(ListUnitsByNameCb::trampoline_clear),
            cb_data: cb.into_void_ptr(),
        };

        // TODO(b/361742242): Handle status returned from the function call.
        // SAFETY: `ctx`, `fqin` and `unit_name` are valid pointers to corresponding types.
        // `vtable` represents the correct Callback Vtable: ASDV_CallbackVTable_ServiceDiscoveryManager_GetUnit.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServiceDiscoveryManager_listUnitsByName(
                self.ctx.as_raw(),
                UnitNameDiscoveryArgs::peel_ref(name_args),
                vtable,
            )
        };

        // TODO(b/361742242): Handle errors returned from the function call more gracefully.
        SdvStatusCode::from(status)
            .expect("Call to ASDV_ServiceDiscoveryManager_listUnitsByName failed.");

        stream.map(ServiceUnitDefinition)
    }

    /// Runs a Service Discovery for a specific Service Unit identified by the provided
    /// [ServiceFqin] and Unit Name. Returns a single [ServiceUnitDefinition] if found or
    /// None otherwise.
    pub async fn get_unit(
        &self,
        fqin: &ServiceFqin,
        unit_name: String,
    ) -> SdvResult<ServiceUnitDefinition> {
        let mut unit_name_buf = vec![0; ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL as usize];
        ffi_str::str_to_bytes(&unit_name, &mut unit_name_buf).expect("Invalid unit name");

        type GetUnitCb = OnceCallbackWrapper<OwnedOpaquePtr<ASDV_ServiceUnitDefinition>>;
        let (cb, fut) = GetUnitCb::new_async();
        let vtable = ASDV_CallbackVTable_ServiceDiscoveryManager_GetUnit {
            func: Some(GetUnitCb::trampoline_func),
            clear: Some(GetUnitCb::trampoline_clear),
            cb_data: cb.into_void_ptr(),
        };

        // TODO(b/361742242): Handle status returned from the function call.
        // SAFETY: `ctx`, `fqin` and `unit_name` are valid pointers to corresponding types.
        // `vtable` represents the correct Callback Vtable: ASDV_CallbackVTable_ServiceDiscoveryManager_GetUnit.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServiceDiscoveryManager_getUnit(
                self.ctx.as_raw(),
                ServiceFqin::peel_ref(fqin),
                unit_name_buf.as_ptr(),
                vtable,
            )
        };

        // TODO(b/361742242): Handle errors returned from the function call more gracefully.
        SdvStatusCode::from(status)
            .expect("Call to ASDV_ServiceDiscoveryManager_listUnitsByType failed.");

        let sud = fut.await.map_err(derive!(SdvStatusCode::Internal))??;
        Ok(ServiceUnitDefinition(sud))
    }

    /// Subscribes to the service unit events by unit type.
    /// Returns a [futures::Stream] of [ServiceUnitChangeEvent].
    pub async fn subscribe_service_unit_change_by_type(
        &self,
        unit_type: &UnitType,
    ) -> SdvResult<UnitChangeEventStream> {
        let (listener_cb_vtable, event_receiver) = {
            type NotifyListenerCb = CallbackWrapper<OwnedOpaquePtr<ASDV_ServiceUnitChangeEvent>>;

            // TODO: simplify  when tooling is ready
            unsafe extern "C" fn trampoline_func(
                cb_data: *mut c_void,
                ptr: *mut ASDV_ServiceUnitChangeEvent,
            ) -> i32 {
                // SAFETY: `cb_data` and `ptr` are valid pointers that are passed to
                // ASDV_ServiceDiscoveryManager_listUnitsByType.
                unsafe {
                    NotifyListenerCb::trampoline_func(
                        cb_data,
                        OwnedOpaquePtr::from_ffi_value(ptr)
                            .expect("Pointer returned by listUnitsByType should be valid"),
                    )
                }
            }

            let (cb, receiver) = NotifyListenerCb::new_async();
            let vtable =
                ASDV_CallbackVTable_ServiceDiscoveryManager_NotifyListenerOfServiceUnitChange {
                    func: Some(trampoline_func),
                    clear: Some(NotifyListenerCb::trampoline_clear),
                    cb_data: cb.into_void_ptr(),
                };
            (vtable, receiver)
        };
        let (callback_handle_future, completion_cb_vtable) = {
            type SubscriptionHandleCb =
                OnceCallbackWrapper<OwnedOpaquePtr<ASDV_ServiceUnitChangeListenerHandle>>;
            let (cb, fut) = SubscriptionHandleCb::new_async();
            let vtable =
                ASDV_CallbackVTable_ServiceDiscoveryManager_RegisterServiceUnitChangeListener {
                    func: Some(SubscriptionHandleCb::trampoline_func),
                    clear: Some(SubscriptionHandleCb::trampoline_clear),
                    cb_data: cb.into_void_ptr(),
                };

            (fut, vtable)
        };

        // SAFETY: `ctx` and `unit_type` are valid pointers to corresponding types.
        // `listener_cb_vtable` and `completion_cb_vtable` represents the correct Callback Vtable: ASDV_CallbackVTable_ServiceDiscoveryManager_GetUnit.
        let status = unsafe {
            ASDV_ServiceDiscoveryManager_registerServiceUnitChangeListenerByType(
                self.ctx.as_raw(),
                UnitType::peel_ref(unit_type),
                listener_cb_vtable,
                completion_cb_vtable,
            )
        };

        // TODO(b/361742242): Handle errors returned from the function call more gracefully.
        SdvStatusCode::from(status).expect(
            "Call to ASDV_ServiceDiscoveryManager_registerServiceUnitChangeListenerByType failed.",
        );

        let callback_handle =
            callback_handle_future.await.map_err(derive!(SdvStatusCode::Internal))??;
        Ok(UnitChangeEventStream::new(self.ctx, callback_handle, event_receiver))
    }

    /// Subscribes to the service unit events using the provided [UnitNameDiscoveryArgs].
    /// Returns a [futures::Stream] of [ServiceUnitChangeEvent].
    pub async fn subscribe_service_unit_change_by_name(
        &self,
        name_args: &UnitNameDiscoveryArgs,
    ) -> SdvResult<UnitChangeEventStream> {
        let (listener_cb_vtable, event_receiver) = {
            type NotifyListenerCb = CallbackWrapper<OwnedOpaquePtr<ASDV_ServiceUnitChangeEvent>>;

            // TODO: simplify  when tooling is ready
            unsafe extern "C" fn trampoline_func(
                cb_data: *mut c_void,
                ptr: *mut ASDV_ServiceUnitChangeEvent,
            ) -> i32 {
                // SAFETY: `cb_data` and `ptr` are valid pointers that are passed to
                // ASDV_ServiceDiscoveryManager_listUnitsByType.
                unsafe {
                    NotifyListenerCb::trampoline_func(
                        cb_data,
                        OwnedOpaquePtr::from_ffi_value(ptr)
                            .expect("Pointer returned by listUnitsByType should be valid"),
                    )
                }
            }

            let (cb, receiver) = NotifyListenerCb::new_async();
            let vtable =
                ASDV_CallbackVTable_ServiceDiscoveryManager_NotifyListenerOfServiceUnitChange {
                    func: Some(trampoline_func),
                    clear: Some(NotifyListenerCb::trampoline_clear),
                    cb_data: cb.into_void_ptr(),
                };
            (vtable, receiver)
        };
        let (callback_handle_future, completion_cb_vtable) = {
            type SubscriptionHandleCb =
                OnceCallbackWrapper<OwnedOpaquePtr<ASDV_ServiceUnitChangeListenerHandle>>;
            let (cb, fut) = SubscriptionHandleCb::new_async();
            let vtable =
                ASDV_CallbackVTable_ServiceDiscoveryManager_RegisterServiceUnitChangeListener {
                    func: Some(SubscriptionHandleCb::trampoline_func),
                    clear: Some(SubscriptionHandleCb::trampoline_clear),
                    cb_data: cb.into_void_ptr(),
                };

            (fut, vtable)
        };

        // SAFETY: `ctx` and `unit_type` are valid pointers to corresponding types.
        // `listener_cb_vtable` and `completion_cb_vtable` represents the correct Callback Vtables
        let status = unsafe {
            ASDV_ServiceDiscoveryManager_registerServiceUnitChangeListenerByName(
                self.ctx.as_raw(),
                UnitNameDiscoveryArgs::peel_ref(name_args),
                listener_cb_vtable,
                completion_cb_vtable,
            )
        };

        // TODO(b/361742242): Handle errors returned from the function call more gracefully.
        SdvStatusCode::from(status).expect(
            "Call to ASDV_ServiceDiscoveryManager_registerServiceUnitChangeListenerByName failed.",
        );

        let callback_handle =
            callback_handle_future.await.map_err(derive!(SdvStatusCode::Internal))??;
        Ok(UnitChangeEventStream::new(self.ctx, callback_handle, event_receiver))
    }
}
