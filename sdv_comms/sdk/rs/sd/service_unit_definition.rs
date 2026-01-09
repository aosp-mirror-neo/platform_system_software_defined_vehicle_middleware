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
use std::fmt::Debug;

use bytemuck::TransparentWrapper;

use sdv_comms_bindgen::ASDV_ServiceIdentity;
use sdv_comms_bindgen::ASDV_ServiceUnitDefinition;
use sdv_comms_bindgen::ASDV_UnitIdentifier;
use sdv_comms_bindgen::ASDV_UnitMetadataType;
use sdv_comms_bindgen::ASDV_UnitType;
use sdv_comms_common::callback::sdk::OnceCallbackReceivable;
use sdv_comms_common::owned_opaque_ptr::OwnedOpaquePtr;
use sdv_status_rs::SdvStatusCode;

use crate::id::ServiceIdentity;
use crate::id::UnitIdentifier;
use crate::sd::{
    InferredMetadata, PublisherUnitMetadata, ServerUnitMetadata,
    UnitMetadata::{self, *},
    UnitType,
};

/// Defines a single result of Service Unit discovery.
/// Owns a pointer of type sdv_comms_bindgen::ASDV_ServiceUnitDefinition.
pub struct ServiceUnitDefinition(pub(crate) OwnedOpaquePtr<ASDV_ServiceUnitDefinition>);

static_assertions::assert_impl_all!(ServiceUnitDefinition: Send, Sync);

// TODO(b/364816238): Review manual Debug implementation
impl Debug for ServiceUnitDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ServiceUnitDefinition").field(&self.0.as_ptr()).finish()
    }
}

/// Safety: A `ServiceUnitDefinition` is a wrapper around `ASDV_ServiceUnitDefinition` which should
/// be threadsafe.
unsafe impl Sync for ServiceUnitDefinition {}

impl Drop for ServiceUnitDefinition {
    fn drop(&mut self) {
        // SAFETY: It is safe to call the FFI function using a pointer
        // created by ASDV_ServiceUnitDefinition_create().
        unsafe { sdv_comms_bindgen::ASDV_ServiceUnitDefinition_destroy(self.0.as_mut_ptr()) }
    }
}

impl Clone for ServiceUnitDefinition {
    fn clone(&self) -> Self {
        // SAFETY: It is safe to call FFI function that creates a new ASDV_ServiceUnitDefinition
        // pointer and transfers ownership of the new pointer.
        let ptr = unsafe { sdv_comms_bindgen::ASDV_ServiceUnitDefinition_clone(self.0.as_ptr()) };
        ServiceUnitDefinition(
            OwnedOpaquePtr::from_ffi_value(ptr)
                .expect("Pointer returned by ServiceUnitDefinition_clone must be valid"),
        )
    }
}

impl ServiceUnitDefinition {
    /// Returns the [UnitIdentifier] for the Service Unit described by this [ServiceUnitDefinition].
    pub fn get_unit_identifier(&self) -> UnitIdentifier {
        let mut unit_identifier = ASDV_UnitIdentifier::default();

        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServiceUnitDefinition_getUnitIdentifier(
                self.0.as_ptr(),
                &mut unit_identifier,
            )
        };

        SdvStatusCode::from(status).expect("ServiceUnitIdentifier::get_unit_identifier() failed");
        UnitIdentifier::wrap(unit_identifier)
    }

    /// Returns the Unit Type for the Service Unit described by this [ServiceUnitDefinition].
    pub fn get_unit_type(&self) -> UnitType {
        let mut unit_type = ASDV_UnitType::default();

        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServiceUnitDefinition_getUnitType(
                self.0.as_ptr(),
                &mut unit_type,
            )
        };

        SdvStatusCode::from(status).expect("ServiceUnitIdentifier::get_unit_type() failed");
        UnitType::wrap(unit_type)
    }

    /// Returns the [ServiceIdentity] for the Service Bundle that registered the Service Unit
    /// described by this [ServiceUnitDefinition].
    pub fn get_service_bundle_identity(&self) -> ServiceIdentity {
        let mut service_identity = ASDV_ServiceIdentity::default();

        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServiceUnitDefinition_getServiceIdentity(
                self.0.as_ptr(),
                &mut service_identity,
            )
        };

        SdvStatusCode::from(status)
            .expect("ServiceUnitIdentifier::get_service_bundle_identity() failed");
        ServiceIdentity::wrap(service_identity)
    }

    /// Returns the [UnitMetadata] for the Service Unit described by this [ServiceUnitDefinition].
    pub fn get_inferred_metadata(&self) -> InferredMetadata {
        let mut inferred_metadata = InferredMetadata::new();

        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServiceUnitDefinition_getInferredMetadata(
                self.0.as_ptr(),
                inferred_metadata.as_raw_mut(),
            )
        };

        SdvStatusCode::from(status).expect("ServiceUnitIdentifier::get_inferred_metadata() failed");
        inferred_metadata
    }

    /// Returns the [UnitMetadata] for the Service Unit described by this [ServiceUnitDefinition].
    pub fn get_unit_metadata(&self) -> UnitMetadata {
        let mut metadata_type = ASDV_UnitMetadataType::default();
        // SAFETY: `self.0` is guaranteed to be a valid pointer to an [ASDV_ServiceUnitDefinition].
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServiceUnitDefinition_getUnitMetadataType(
                self.0.as_ptr(),
                &mut metadata_type,
            )
        };
        SdvStatusCode::from(status)
            .expect("ServiceUnitDefinition::get_unit_metadata_type() failed");

        match metadata_type {
            sdv_comms_bindgen::ASDV_UnitMetadataType_ASDV_UNIT_METADATA_TYPE_SERVER => {
                Server(self.get_server_unit_metadata())
            }
            sdv_comms_bindgen::ASDV_UnitMetadataType_ASDV_UNIT_METADATA_TYPE_PUBLISHER => {
                Publisher(self.get_publisher_unit_metadata())
            }
            _ => panic!("ServiceUnitDefinition::get_unit_metadata() failed: unknown metadata type"),
        }
    }

    /// Returns the Application Metadata for the Service Unit described by this [ServiceUnitDefinition].
    pub fn get_application_metadata(&self) -> Vec<u8> {
        let mut size: usize = 0;
        // SAFETY: `self.0` is guaranteed to be a valid pointer to an [ASDV_ServiceUnitDefinition].
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServiceUnitDefinition_getApplicationMetadataSize(
                self.0.as_ptr(),
                &mut size,
            )
        };

        SdvStatusCode::from(status).expect(
            "sdv_comms_bindgen::ASDV_ServiceUnitDefinition_getApplicationMetadataSize()
                    failed",
        );

        let mut application_metadata: Vec<u8> = vec![0; size];
        // SAFETY: `self.0` is guaranteed to be a valid pointer to an [ASDV_ServiceUnitDefinition]
        // and application_metadata is a pointer to an array of the correct size.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServiceUnitDefinition_getApplicationMetadata(
                self.0.as_ptr(),
                application_metadata.as_mut_ptr(),
                size,
            )
        };
        SdvStatusCode::from(status)
            .expect("sdv_comms_bindgen::ASDV_ServiceUnitDefinition_getApplicationMetadata failed");
        application_metadata
    }

    fn get_publisher_unit_metadata(&self) -> PublisherUnitMetadata {
        let mut metadata = PublisherUnitMetadata::new();

        // SAFETY: `self.0` and `metadata.as_raw_mut()` are valid pointers for the duration of the
        // function call.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServiceUnitDefinition_getPublisherUnitMetadata(
                self.0.as_ptr(),
                metadata.as_raw_mut(),
            )
        };

        SdvStatusCode::from(status)
            .expect("ServiceUnitDefinition::get_publisher_unit_metadata() failed");

        metadata
    }

    fn get_server_unit_metadata(&self) -> ServerUnitMetadata {
        let mut metadata = ServerUnitMetadata::new();

        // SAFETY: `self.0` and `metadata.as_raw_mut()` are valid pointers for the duration of the
        // function call.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServiceUnitDefinition_getServerUnitMetadata(
                self.0.as_ptr(),
                metadata.as_raw_mut(),
            )
        };

        SdvStatusCode::from(status)
            .expect("ServiceUnitDefinition::get_server_unit_metadata() failed");

        metadata
    }

    #[cfg(test)]
    fn create_for_test() -> Self {
        use sdv_comms_sd::ffi::ASDV_ServiceUnitDefinition as ASDV_ServiceUnitDefinitionInternal;
        use sdv_comms_sd::discovery::ServiceUnitDefinition as ServiceUnitDefinitionInternal;
        use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::{ServiceUnitDefinition as ServiceUnitDefinitionAidl};
        use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::{
            InferredMetadata::{InferredMetadata as InferredMetadataAidl, RelativeLocation::RelativeLocation as RelativeLocationAidl},
            ApplicationMetadata::{ApplicationMetadata as ApplicationMetadataAidl, TypedMetadata::TypedMetadata as TypedMetadataAidl},
            ServerUnitMetadata::ServerUnitMetadata as ServerUnitMetadataAidl,
        };
        use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::EcPublicKey::EcPublicKey as EcPublicKeyAidl;
        use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::ServiceIdentity as ServiceIdentityAidl;
        use sdv_comms_id::{ServiceFqin, SERVICE_PUBLIC_KEY_SIZE_BYTES};
        use sdv_comms_sd::discovery::UnitType;
        use std::str::FromStr;

        let fqin = ServiceFqin::from_str("vm-name:com.test.pkg.TestBundleName/default").unwrap();
        let unit_type = UnitType::from_str("com.google.sdv.WindowManager#Temperature").unwrap();
        let identity_aidl = ServiceIdentityAidl {
            sid: 1234,
            publicKey: EcPublicKeyAidl { keyValue: [45_u8; SERVICE_PUBLIC_KEY_SIZE_BYTES] },
            name: fqin.clone().into(),
        };

        let aidl = ServiceUnitDefinitionAidl {
            serviceUnitName: String::from("service_unit_name"),
            identity: identity_aidl.clone(),
            unitType: unit_type.clone().into(),
            appMetadata: ApplicationMetadataAidl {
                valueHolder: [1, 1, 1].to_vec(),
                typedMetadata: TypedMetadataAidl::ServerUnit(ServerUnitMetadataAidl {
                    version: 42,
                }),
            },
            inferredMetadata: InferredMetadataAidl {
                location: RelativeLocationAidl::LOCATION_LOCAL_HARDWARE,
            },
        };

        Self(
            OwnedOpaquePtr::from_ffi_value(Box::into_raw(Box::new(
                ASDV_ServiceUnitDefinitionInternal::wrap(
                    ServiceUnitDefinitionInternal::try_from(aidl).unwrap(),
                ),
            )) as *mut ()
                as *mut ASDV_ServiceUnitDefinition)
            .expect("Pointer created from ASDV_ServiceUnitDefinitionInternal must be valid"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{id::ServiceFqin, sd::RelativeLocation};
    use googletest::{assert_that, prelude::eq};
    use sdv_comms_id::SERVICE_PUBLIC_KEY_SIZE_BYTES;
    use std::str::FromStr;

    #[test]
    fn test_get_unit_metadata() {
        let sud = ServiceUnitDefinition::create_for_test();

        let mut expected_metadata_internal = ServerUnitMetadata::new();
        expected_metadata_internal.set_version(42);
        let expected_metadata = Server(expected_metadata_internal);

        assert_that!(sud.get_unit_metadata(), eq(&expected_metadata));
    }

    #[test]
    fn test_get_inferred_metadata() {
        let sud = ServiceUnitDefinition::create_for_test();

        assert_that!(
            sud.get_inferred_metadata().get_location(),
            eq(&RelativeLocation::LocalHardware)
        );
    }

    #[test]
    fn test_get_service_bundle_identity() {
        let sud = ServiceUnitDefinition::create_for_test();

        let fqin = ServiceFqin::from_str("vm-name:com.test.pkg.TestBundleName/default").unwrap();
        let service_identity = ASDV_ServiceIdentity {
            sid: 1234,
            fqin: ServiceFqin::peel(fqin),
            public_key: [45_u8; SERVICE_PUBLIC_KEY_SIZE_BYTES],
        };

        let service_identity = ServiceIdentity::wrap(service_identity);
        assert_that!(sud.get_service_bundle_identity(), eq(&service_identity));
    }

    #[test]
    fn test_get_unit_type() {
        let sud = ServiceUnitDefinition::create_for_test();

        // Same type used by create_for_test.
        let unit_type = UnitType::from_str("com.google.sdv.WindowManager#Temperature").unwrap();

        assert_that!(sud.get_unit_type(), eq(&unit_type));
    }
}
