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

#[cfg(not(comms_stack_stub))]
use bytemuck::TransparentWrapper;
use sdv_comms_common::{define_callback_ffi_struct, define_once_callback_ffi_struct};
use sdv_comms_common::{ffi_str, try_status};
use sdv_comms_ctx::ffi::ASDV_Context;
use sdv_comms_id::ffi::{ASDV_ServiceFqin, ASDV_ServiceIdentity, ASDV_UnitIdentifier};
use sdv_comms_id::ffi::{
    ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
    ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
    ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL, ASDV_VM_NAME_MAX_SIZE_BYTES_INCLUDING_NUL,
};
#[cfg(not(comms_stack_stub))]
use sdv_comms_id::ServiceFqin;
use sdv_comms_stack_ffi_macro::{comms_stack_ffi_fn, comms_stack_ffi_struct};
use sdv_status_rs::{derive, sdv_status_t, SdvStatus, SdvStatusCode};
use std::os::raw::c_uchar;

use crate::ffi::unit_type::ASDV_UnitType;
use crate::ffi::{
    ASDV_InferredMetadata, ASDV_PublisherUnitMetadata, ASDV_ServerUnitMetadata,
    ASDV_UnitMetadataType,
};
#[cfg(not(comms_stack_stub))]
use crate::manager::ServiceUnitChangeSubscriptionHandle;
#[cfg(not(comms_stack_stub))]
use crate::{discovery::ServiceUnitDefinition, manager};

extern crate libc;

/// Describes the arguments for Service Unit discovery.
///
/// These arguments comprises the following components:
/// 1) [REQUIRED] SDV Package Name: The Package Name within which the Service Bundle was shipped.
/// 2) [REQUIRED] Service Bundle Name: The Service Bundle Name as declared in the Package Manifest.
/// 3) [OPTIONAL] Service Instance Name: The Service Instance Name as assigned by the Orchestrator
///    when the Service Bundle was loaded. This value may be empty (indicated by a NUL character
///    (\0) as the first byte).
/// 4) [OPTIONAL] SDV VM Name: The SDV VM Instance on which this Service is registered. This value
///    may be empty (indicated by a NUL character (\0) as the first byte).
///
/// Each name field in this struct MUST be a valid ASCII encoded sequence of bytes ending with the
/// NUL (\0) character. All fields (if present) MUST match the naming requirements according to
/// [Service Naming Conventions]
/// (https://docs.partner.android.com/automotive/sdv/core-areas/naming-conventions)xd
#[comms_stack_ffi_struct(shared)]
#[repr(C)]
pub struct ASDV_UnitNameDiscoveryArgs {
    pub(crate) sdv_package_name: [c_uchar; ASDV_PACKAGE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
    pub(crate) service_bundle_name:
        [c_uchar; ASDV_SERVICE_BUNDLE_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
    pub(crate) unit_name: [c_uchar; ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
    pub(crate) sdv_vm_name: [c_uchar; ASDV_VM_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
}

/// Individual result of a Service Discovery call, representing a single ServiceUnit discovered.
#[comms_stack_ffi_struct(opaque)]
#[derive(Clone)]
pub struct ASDV_ServiceUnitDefinition(
    // cbindgen:no-export
    pub(crate) ServiceUnitDefinition,
);

/// Discriminant for ServiceUnitChangeEventType types.
#[repr(u8)]
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ASDV_ServiceUnitChangeEventType {
    /// Type for the service unit registration event.
    ASDV_SERVICE_UNIT_REGISTERED = 1,
    /// Type for the service unit unregistration event.
    ASDV_SERVICE_UNIT_UNREGISTERED = 2,
    /// Type for the service unit change event.
    ASDV_SERVICE_UNIT_CHANGED = 3,
}

/// Parameter type for the callback which is triggered when a service unit
/// registers/unregisters/changes at Service Discovery.
#[comms_stack_ffi_struct(opaque)]
pub struct ASDV_ServiceUnitChangeEvent(
    pub(crate) (ASDV_ServiceUnitChangeEventType, Box<ASDV_ServiceUnitDefinition>),
);

/// Clones an ASDV_ServiceUnitDefinition.
///
/// This function always returns a valid heap-allocated ASDV_ServiceUnitDefinition. It aborts
/// if it fails to allocate the object on the heap. The caller is responsible for freeing the
/// created object, which is returned as a pointer by this function.
///
/// SAFETY: The caller takes ownership of the returned object and is responsible for freeing the
/// memory pointed to by the returned pointer using ASDV_ServiceUnitDefinition_destroy().
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceUnitDefinition_clone(
    sud: Option<&ASDV_ServiceUnitDefinition>,
) -> Option<Box<ASDV_ServiceUnitDefinition>> {
    sud.map(|sud| Box::new(sud.clone()))
}

/// Deletes an ASDV_ServiceUnitDefinition returned by ASDV_ServiceUnitDefinition_create() function.
///
/// Parameters:
///    [IN] ptr: The ASDV_ServiceUnitDefinition value to delete.
///
/// # SAFETY: The caller guarantees that `ptr` is a valid pointer created with
/// ASDV_ServiceUnitDefinition_create().
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceUnitDefinition_destroy(
    ptr: Option<Box<ASDV_ServiceUnitDefinition>>,
) {
    if let Some(sud) = ptr {
        drop(sud)
    }
}

/// Returns the set value for UnitIdentifier of the ServiceUnitDefinition.
///
/// Parameters:
///    [IN] service_unit_definition: A valid pointer to the ServiceUnitDefinition object for which
///         to get the UnitIdentifier
///    [OUT] unit_identifier: Pointer to UnitIdentifier
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT if the size of the allocated UnitIdentifier is not large enough.
///
/// SAFETY: The caller guarantees that `service_unit_definition` and `unit_identifier` are all valid pointers that
///        live for at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceUnitDefinition_getUnitIdentifier(
    service_unit_definition: &ASDV_ServiceUnitDefinition,
    unit_identifier: &mut ASDV_UnitIdentifier,
) -> sdv_status_t {
    try_status! {
        service_unit_definition.0.get_unit_identifier().clone_into(unit_identifier)
    }
}

/// Returns the UnitType of the ServiceUnitDefinition.
///
/// Parameters:
///    [IN] service_unit_definition: A valid pointer to the ServiceUnitDefinition object for which
///         to get the UnitType
///    [OUT] unit_type: Pointer to UnitType
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT if the size of the allocated UnitType is not large enough.
///
/// SAFETY: The caller guarantees that `service_unit_definition` and `unit_type` are all valid pointers that
///        live for at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceUnitDefinition_getUnitType(
    service_unit_definition: &ASDV_ServiceUnitDefinition,
    unit_type: &mut ASDV_UnitType,
) -> sdv_status_t {
    try_status! {
        service_unit_definition.0.get_unit_type().clone_into(unit_type)
    }
}
/// Returns the type of metadata set of the ServiceUnitDefinition.
///
/// Parameters:
///    [IN] service_unit_definition: A valid pointer to the ServiceUnitDefinition object for which
/// to get the UnitMetadata type.
///    [OUT] metadata_type: The [UnitTypeMetadata] for the provided ServiceUnitDefinition.
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT in case if `service_unit_definition` is not a valid pointer.
///
/// SAFETY: The caller guarantees that `service_unit_definition` and `metadata_type` are valid
/// pointers that live for at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceUnitDefinition_getUnitMetadataType(
    service_unit_definition: &ASDV_ServiceUnitDefinition,
    metadata_type: &mut ASDV_UnitMetadataType,
) -> sdv_status_t {
    try_status! {
        *metadata_type = service_unit_definition.0.get_unit_metadata().get_type();
        Ok(())
    }
}

/// Returns the size of application metadata set for the ServiceUnit
///
/// Parameters:
///    [IN] service_unit_definition: A valid pointer to the ServiceUnitDefinition object for which
///         to get the Application Metadata size
///    [OUT] buffer_size: Pointer to an usize that will store the size of Application Metadata
///
/// SAFETY: The caller guarantees that `service_unit_definition` and `buffer_size` are valid
///         pointers that live for at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceUnitDefinition_getApplicationMetadataSize(
    service_unit_definition: &ASDV_ServiceUnitDefinition,
    buffer_size: &mut libc::size_t,
) -> sdv_status_t {
    try_status! {
        *buffer_size = service_unit_definition.0.get_unit_metadata().get_application_metadata_size();
        Ok(())
    }
}

/// Returns the set value for Application Metadata of the ServiceUnitDefinition.
///
/// Parameters:
///    [IN] service_unit_definition: A valid pointer to the ServiceUnitDefinition object for which
///         to get the UnitMetadata
///    [OUT] application_metadata: Pointer to a mutable byte array of size buffer_size
///    [In] buffer_size: Size of the application metadata being read
///
/// SAFETY: The caller guarantees that `service_unit_definition` and `application_metadata` are
///         valid pointers that live for at least the duration of the function call.
///         `application_metadata` is at least buffer_size long.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceUnitDefinition_getApplicationMetadata(
    service_unit_definition: &ASDV_ServiceUnitDefinition,
    #[allow_ptr] application_metadata: *mut u8,
    #[allow_value] buffer_size: libc::size_t,
) -> sdv_status_t {
    try_status! {
        let metadata = service_unit_definition.0.get_unit_metadata().get_application_metadata();
        if metadata.len() < buffer_size {
            return Err(SdvStatus::with_message(
                SdvStatusCode::DataLoss, "Application Metadata received is not of
                expected size"))
        }
        // SAFETY: The caller guarantees that `application_metadata` is a valid mutable pointer of
        // the expected size. The expected size is determined using
        // ASDV_ServiceUnitDefinition_getApplicationMetadataSize API call.
        let application_metadata_slice = unsafe {
             std::slice::from_raw_parts_mut(application_metadata, buffer_size)
        };
        application_metadata_slice.copy_from_slice(metadata);
        Ok(())
    }
}

/// Returns the set value for UnitMetadata of the ServiceUnitDefinition.
///
/// Parameters:
///    [IN] service_unit_definition: A valid pointer to the ServiceUnitDefinition object for which
///         to get the UnitMetadata
///    [OUT] unit_metadata: Pointer to UnitMetadata
///
/// SAFETY: The caller guarantees that `service_unit_definition` and `unit_identifier` are valid
///         pointers that live for at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceUnitDefinition_getPublisherUnitMetadata(
    service_unit_definition: &ASDV_ServiceUnitDefinition,
    metadata: &mut ASDV_PublisherUnitMetadata,
) -> sdv_status_t {
    try_status! {

        if let Some(ref metadata_internal) = service_unit_definition.0.get_unit_metadata().get_publisher_metadata() {
            let publisher_metadata = ASDV_PublisherUnitMetadata::peel_mut(metadata);
            publisher_metadata.clone_from(metadata_internal);
            Ok(())
        } else {
            Err(SdvStatus::with_message(
                SdvStatusCode::FailedPrecondition, "UnitMetadata is not of type PublisherUnitMetadata"))
        }
    }
}

/// Returns the set value for UnitMetadata of the ServiceUnitDefinition.
///
/// Parameters:
///    [IN] service_unit_definition: A valid pointer to the ServiceUnitDefinition object for which
///         to get the UnitMetadata
///    [OUT] unit_metadata: Pointer to UnitMetadata
///
/// SAFETY: The caller guarantees that `service_unit_definition` and `unit_identifier` are valid
///         pointers that live for at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceUnitDefinition_getServerUnitMetadata(
    service_unit_definition: &ASDV_ServiceUnitDefinition,
    metadata: &mut ASDV_ServerUnitMetadata,
) -> sdv_status_t {
    try_status! {

        if let Some(ref metadata_internal) = service_unit_definition.0.get_unit_metadata().get_server_metadata() {
            let server_metadata = ASDV_ServerUnitMetadata::peel_mut(metadata);
            server_metadata.clone_from(metadata_internal);
            Ok(())
        } else {
            Err(SdvStatus::with_message(
                SdvStatusCode::FailedPrecondition, "UnitMetadata is not of type ServerUnitMetadata"))
        }
    }
}

/// Returns the set value for InferredMetadata of the ServiceUnitDefinition.
///
/// Parameters:
///    [IN] service_unit_definition: A valid pointer to the ServiceUnitDefinition object for
///         which to get the UnitMetadata
///    [OUT] metadata: Pointer to ASDV_InferredMetadata
///
/// SAFETY: The caller guarantees that `service_unit_definition` and `metadata` are valid pointers
///         that live for at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceUnitDefinition_getInferredMetadata(
    service_unit_definition: &ASDV_ServiceUnitDefinition,
    metadata: &mut ASDV_InferredMetadata,
) -> sdv_status_t {
    try_status! {
        let metadata_internal = service_unit_definition.0.get_inferred_metadata();

        let inferred_metadata = ASDV_InferredMetadata::peel_mut(metadata);
        inferred_metadata.clone_from(metadata_internal);
        Ok(())
    }
}

/// Returns the ServiceIdentity of the Service Bundle that registered the provided
/// ServiceUnitDefinition.
///
/// Parameters:
///    [IN] service_unit_definition: A valid reference to the ServiceUnitDefinition object for which
///         to get the ServiceIdentity
///    [OUT] service_identity: reference to ServiceIdentity
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT if the size of the allocated ServiceIdentity is not large enough.
///
/// SAFETY: The caller guarantees that `service_unit_definition` and `service_identity` are all
///    valid references that live for at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceUnitDefinition_getServiceIdentity(
    service_unit_definition: &ASDV_ServiceUnitDefinition,
    service_identity: &mut ASDV_ServiceIdentity,
) -> sdv_status_t {
    try_status! {
        service_unit_definition.0.get_identity().clone_into(service_identity)
    }
}

// Define the struct ASDV_CallbackVTable_ServiceDiscoveryManager_ListUnitsByType.
define_callback_ffi_struct!(
    ServiceDiscoveryManager_ListUnitsByType,
    Box<ASDV_ServiceUnitDefinition>
);

/// Lists discoverable Service Units that match the provided UnitType.
///
/// Parameters:
///    [IN] ctx: Pointer to the Context object for this ServiceBundle.
///    [IN] unit_type: The UnitType used for discovering Service Units.
///    [IN] cb_vtable: The CallbackVTable which will be used to trigger the callback when the
///         backing operation has results.
///
/// This function returns immediately and invokes the provided callback (multiple times) as soon
/// as results from the IPC call to the local (same VM) ServiceDiscoveryAgent are available.
///
/// SAFETY: The caller guarantees all the following:
///   1. `ctx` is a valid Context object pointer,
///   2. `unit_type` is a pointer to a valid ASDV_UnitType declaration,
///   3. cb_vtable transfers ownership of any provided ASDV_ServiceUnitDefinition pointers. These
///      must be cleared appropriately by calling ASDV_ServiceUnitDefinition_destroy after use.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceDiscoveryManager_listUnitsByType(
    ctx: &'static ASDV_Context,
    unit_type: &ASDV_UnitType,
    #[allow_value] cb_vtable: ASDV_CallbackVTable_ServiceDiscoveryManager_ListUnitsByType,
) -> sdv_status_t {
    try_status! {
        let ctx = ASDV_Context::peel_ref(ctx);
        let unit_type = unit_type.try_into()?;
        let callback = cb_vtable.into();

        ctx.get_executor().spawn(Box::pin(async move {
            match manager::list_service_units_by_type(ctx, &unit_type).await {
                Ok(service_units) => {
                    for sud in service_units.into_iter() {
                        let sud_ptr = Box::new(ASDV_ServiceUnitDefinition(sud));
                        if let Err(status) = callback.invoke(sud_ptr) {
                            log::error!("ServiceDiscoveryManager_listUnitsByType callback \
                                invocation failed with error: {status}");
                            break;
                        }
                    }
                }
                Err(e) => {
                    // TODO: b/361742242 - return error to the API user instead of logging.
                    log::error!("Failed to list service units by type: {e}");
                }
            }
            drop(callback);
        }));

        Ok(())
    }
}

// Define the struct ASDV_CallbackVTable_ServiceDiscoveryManager_ListUnitsByName.
define_callback_ffi_struct!(
    ServiceDiscoveryManager_ListUnitsByName,
    Box<ASDV_ServiceUnitDefinition>
);

/// Lists discoverable Service Units that match the provided UnitNameDiscoveryArgs.
///
/// Parameters:
///    [IN] ctx: Pointer to the Context object for this ServiceBundle.
///    [IN] name_args: The UnitNameDiscoveryArgs used for discovering Service Units.
///    [IN] cb_vtable: The CallbackVTable which will be used to trigger the callback when the
///         backing operation has results.
///
/// This function returns immediately and invokes the provided callback (multiple times) as soon
/// as results from the IPC call to the local (same VM) ServiceDiscoveryAgent are available.
///
/// SAFETY: The caller guarantees all the following:
///   1. `ctx` is a valid Context object pointer,
///   2. `name_args` is a pointer to a valid ASDV_UnitNameDiscoveryArgs declaration,
///   3. cb_vtable transfers ownership of any provided ASDV_ServiceUnitDefinition pointers. These
///      must be cleared appropriately by calling ASDV_ServiceUnitDefinition_destroy after use.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceDiscoveryManager_listUnitsByName(
    ctx: &'static ASDV_Context,
    name_args: &ASDV_UnitNameDiscoveryArgs,
    #[allow_value] cb_vtable: ASDV_CallbackVTable_ServiceDiscoveryManager_ListUnitsByName,
) -> sdv_status_t {
    try_status! {
        let ctx = ASDV_Context::peel_ref(ctx);
        let name_args = name_args.try_into()?;
        let callback = cb_vtable.into();

        ctx.get_executor().spawn(Box::pin(async move {
            match manager::list_service_units_by_name(ctx, &name_args).await {
                Ok(service_units) => {
                    for sud in service_units.into_iter() {
                        let sud_ptr = Box::new(ASDV_ServiceUnitDefinition(sud));
                        if let Err(status) = callback.invoke(sud_ptr) {
                            log::error!("ServiceDiscoveryManager_listUnitsByType callback failed \
                                with error: {status}");
                            break;
                        }
                    };
                }
                Err(e) => {
                    // TODO: b/361742242 - return error to the API user instead of logging.
                    log::error!("Failed to list service units by name: {e}");
                }
            }
            drop(callback);
        }));

        Ok(())
    }
}

// Define the struct ASDV_CallbackVTable_ServiceDiscoveryManager_GetUnit.
define_once_callback_ffi_struct!(ServiceDiscoveryManager_GetUnit, Box<ASDV_ServiceUnitDefinition>);

/// Finds the specific ServiceUnit that matches the provided ServiceFqin and UnitName.
///
/// Parameters:
///    [IN] ctx: Pointer to the Context object for this ServiceBundle.
///    [IN] fqin: The ServiceFqin for the hosting ServiceBundle of the requested ServiceUnit.
///    [IN] unit_name: The unit_name registered by the ServiceBundle during ServiceUnit creation.
///    [IN] cb_vtable: The CallbackVTable which will be used to trigger the callback when the
///         backing operation completes.
///
/// This function returns immediately and invokes the provided callback when the IPC call to the
/// local (same VM) ServiceDiscoveryAgent completes.
///
/// SAFETY: The caller guarantees all the following:
///   1. `ctx` is a valid Context object pointer,
///   2. `fqin` is a pointer to a validated ASDV_ServiceFqin declaration,
///   3. `unitName` is a valid pointer to a string of valid ASCII encoded bytes ending with the
///      NUL(\0) character.
///   4. cb_vtable transfers ownership of the provided ASDV_ServiceUnitDefinition pointer. This
///      must be cleared appropriately by calling ASDV_ServiceUnitDefinition_destroy after use.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceDiscoveryManager_getUnit(
    ctx: &'static ASDV_Context,
    fqin: &ASDV_ServiceFqin,
    #[allow_ptr] unit_name: *const c_uchar,
    #[allow_value] cb_vtable: ASDV_CallbackVTable_ServiceDiscoveryManager_GetUnit,
) -> sdv_status_t {
    try_status! {
        let ctx = ASDV_Context::peel_ref(ctx);
        let fqin = ServiceFqin::try_from(fqin)?;
        // SAFETY: The caller promises that the pointer is a Nul terminated C String.
        let unit_name = unsafe { ffi_str::cstr_pointer_to_str(unit_name as _)? };
        let callback = cb_vtable.into();

        ctx.get_executor().spawn(Box::pin(async move {
            let service_unit_res = manager::get_service_unit(ctx, &fqin, &unit_name).await
                .and_then(|opt_sud| opt_sud.ok_or(SdvStatus::new(SdvStatusCode::NotFound)))
                .map(|sud| Box::new(ASDV_ServiceUnitDefinition(sud)));
            callback.invoke(service_unit_res);
        }));

        Ok(())
    }
}

#[comms_stack_ffi_struct(opaque)]
pub struct ASDV_ServiceUnitChangeListenerHandle(
    // cbindgen:no-export
    ServiceUnitChangeSubscriptionHandle,
);

// Define the struct ASDV_CallbackVTable_ServiceDiscoveryManager_RegisterServiceUnitChangeListener.
define_once_callback_ffi_struct!(
    ServiceDiscoveryManager_RegisterServiceUnitChangeListener,
    Box<ASDV_ServiceUnitChangeListenerHandle>
);

// Define the struct ASDV_CallbackVTable_ServiceDiscoveryManager_NotifyListenerOfServiceUnitChange.
define_callback_ffi_struct!(
    ServiceDiscoveryManager_NotifyListenerOfServiceUnitChange,
    Box<ASDV_ServiceUnitChangeEvent>
);

/// Registers service unit change listener by UnitType.
///
/// Parameters:
///    [IN] ctx: Pointer to the Context object for this ServiceBundle.
///    [IN] unit_type: The UnitType used for discovering Service Units.
///    [IN] cb_vtable: The CallbackVTable which will be used to trigger the callback when the
///         backing operation completes.
///
/// This function returns immediately and invokes the provided callback when the IPC call to the
/// local (same VM) ServiceDiscoveryAgent completes.
///
/// SAFETY: The caller guarantees all the following:
///   1. `ctx` is a valid Context object pointer,
///   2. `unit_type` is a pointer to a valid ASDV_UnitType declaration,
///   3. listener_cb_vtable transfers ownership of the provided ASDV_ServiceUnitChangeEvent pointer.
///      The caller must use ASDV_ServiceUnitChangeEvent_extractDefinition to extract definition and destroy it.
///   4. completion_cb_vtable transfers ownership of the provided ASDV_ServiceUnitChangeListenerHandle
///      pointer. This must be cleared appropriately by calling ASDV_ServiceDiscoveryManager_unregisterServiceUnitChangeListener
///      after use.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceDiscoveryManager_registerServiceUnitChangeListenerByType(
    ctx: &'static ASDV_Context,
    unit_type: &ASDV_UnitType,
    #[allow_value] listener_cb_vtable: ASDV_CallbackVTable_ServiceDiscoveryManager_NotifyListenerOfServiceUnitChange,
    #[allow_value] completion_cb_vtable: ASDV_CallbackVTable_ServiceDiscoveryManager_RegisterServiceUnitChangeListener,
) -> sdv_status_t {
    try_status! {
        let ctx = ASDV_Context::peel_ref(ctx);
        let unit_type = unit_type.try_into()?;
        let listener_cb_vtable = listener_cb_vtable.into();
        let completion_callback = completion_cb_vtable.into();

        ctx.get_executor().spawn(Box::pin(async move {
            let subscription_handle = manager::register_service_unit_change_listener_by_type(ctx, &unit_type, listener_cb_vtable)
            .await
                .map(|handle| Box::new(ASDV_ServiceUnitChangeListenerHandle(handle)));
            completion_callback.invoke(subscription_handle);
        }));

        Ok(())
    }
}

/// Registers service unit change listener by UnitNameDiscoveryArgs.
///
/// Parameters:
///    [IN] ctx: Pointer to the Context object for this ServiceBundle.
///    [IN] name_args: The UnitNameDiscoveryArgs used for discovering Service Units.
///    [IN] cb_vtable: The CallbackVTable which will be used to trigger the callback when the
///         backing operation completes.
///
/// This function returns immediately and invokes the provided callback when the IPC call to the
/// local (same VM) ServiceDiscoveryAgent completes.
///
/// SAFETY: The caller guarantees all the following:
///   1. `ctx` is a valid Context object pointer,
///   2. `name_args` is a pointer to a valid ASDV_UnitNameDiscoveryArgs declaration,
///   3. listener_cb_vtable transfers ownership of the provided ASDV_ServiceUnitChangeEvent pointer.
///      The caller must use ASDV_ServiceUnitChangeEvent_extractDefinition to extract definition and destroy it.
///   4. completion_cb_vtable transfers ownership of the provided ASDV_ServiceUnitChangeListenerHandle
///      pointer. This must be cleared appropriately by calling ASDV_ServiceDiscoveryManager_unregisterServiceUnitChangeListener
///      after use.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceDiscoveryManager_registerServiceUnitChangeListenerByName(
    ctx: &'static ASDV_Context,
    name_args: &ASDV_UnitNameDiscoveryArgs,
    #[allow_value] listener_cb_vtable: ASDV_CallbackVTable_ServiceDiscoveryManager_NotifyListenerOfServiceUnitChange,
    #[allow_value] completion_cb_vtable: ASDV_CallbackVTable_ServiceDiscoveryManager_RegisterServiceUnitChangeListener,
) -> sdv_status_t {
    try_status! {
        let ctx = ASDV_Context::peel_ref(ctx);
        let name_args = name_args.try_into()?;
        let listener_cb_vtable = listener_cb_vtable.into();
        let completion_callback = completion_cb_vtable.into();

        ctx.get_executor().spawn(Box::pin(async move {
            let subscription_handle = manager::register_service_unit_change_listener_by_name(ctx, &name_args, listener_cb_vtable)
            .await
                .map(|handle| Box::new(ASDV_ServiceUnitChangeListenerHandle(handle)));
            completion_callback.invoke(subscription_handle);
        }));

        Ok(())
    }
}

/// Unregisters service unit change listener.
///
/// Parameters:
///    [IN] ctx: Pointer to the Context object for this ServiceBundle.
///    [IN] subscription_handle: The handle returned by [ASDV_ServiceDiscoveryManager_registerServiceUnitChangeListenerByType]
///
/// This function is non-blocking
///
/// SAFETY: The caller guarantees all the following:
///   1. `ctx` is a valid Context object pointer,
///   2. `subscription_handle` is a pointer to a valid [ASDV_ServiceUnitChangeListenerHandle] created by the registration function.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceDiscoveryManager_unregisterServiceUnitChangeListener(
    ctx: &'static ASDV_Context,
    subscription_handle: Box<ASDV_ServiceUnitChangeListenerHandle>,
) -> sdv_status_t {
    try_status! {
        let ctx = ASDV_Context::peel_ref(ctx);

        ctx.get_executor().spawn(Box::pin(async move {
            let _res = manager::unregister_service_unit_change_listener_by_type(ctx, subscription_handle.0).await;
        }));

        Ok(())
    }
}

impl ASDV_ServiceUnitChangeEvent {
    /// Creates a new instance of service unit change event.
    #[cfg(not(comms_stack_stub))]
    pub fn new(
        event_type: ASDV_ServiceUnitChangeEventType,
        service_unit_definition: Box<ASDV_ServiceUnitDefinition>,
    ) -> ASDV_ServiceUnitChangeEvent {
        ASDV_ServiceUnitChangeEvent((event_type, service_unit_definition))
    }
}

/// Returns the type of event.
///
/// Parameters:
///    [IN] change_event: A valid pointer to the [ASDV_ServiceUnitChangeEvent] object for which
/// to get the [ASDV_ServiceUnitChangeEventType].
///    [OUT] event_type: The contained [ASDV_ServiceUnitChangeEventType] for the provided event.
///
/// Return Value: Returns SDV_STATUS_OK on success, otherwise one of the following error codes:
///    ASDV_STATUS_INVALID_ARGUMENT in case if `change_event` is not a valid pointer.
///
/// SAFETY: The caller guarantees that `change_event` and `event_type` are valid
/// pointers that live for at least the duration of the function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceUnitChangeEvent_getEventType(
    change_event: &ASDV_ServiceUnitChangeEvent,
    event_type: &mut ASDV_ServiceUnitChangeEventType,
) -> sdv_status_t {
    try_status! {
        *event_type = change_event.0.0;
        Ok(())
    }
}

/// Extracts the ASDV_ServerUnitDefinition from an ASDV_ServiceUnitChangeEvent, destroying it in the process.
///
/// Parameters:
///    [IN] ptr: The ASDV_ServiceUnitChangeEvent value to delete.
///
/// SAFETY: The caller guarantees that `ptr` is a valid pointer.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceUnitChangeEvent_extractDefinition(
    ptr: Option<Box<ASDV_ServiceUnitChangeEvent>>,
) -> Option<Box<ASDV_ServiceUnitDefinition>> {
    ptr.map(|event| event.0 .1)
}
