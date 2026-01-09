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

use sdv_comms_common::try_status;
use sdv_comms_stack_ffi_macro::{comms_stack_ffi_fn, comms_stack_ffi_struct};
use std::os::raw::c_uchar;

use super::service_fqin::{ASDV_ServiceFqin, ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL};

#[allow(non_camel_case_types)]
type sdv_status_t = i32;

#[cfg(comms_stack_ffi)]
use crate::ServiceIdentity;

/// Size in bytes of the X25519 Service Public Key.
pub const ASDV_SERVICE_PUBLIC_KEY_SIZE_BYTES: usize = 32;

/// A X25519 Service Public Key.
#[allow(non_camel_case_types)]
pub type ASDV_ServicePublicKey = [c_uchar; ASDV_SERVICE_PUBLIC_KEY_SIZE_BYTES];

/// Describes the Service Identity of a Service Bundle.
#[comms_stack_ffi_struct(shared)]
#[repr(C)]
#[derive(Default)]
pub struct ASDV_ServiceIdentity {
    /// The SID of the service bundle instance
    pub(crate) sid: u64,
    /// The fully qualified instance name of the service bundle instance.
    pub(crate) fqin: ASDV_ServiceFqin,
    // TODO: b/400635525 - Remove this once API breaks are possible.
    /// Unused
    pub(crate) public_key: ASDV_ServicePublicKey,
}

/// Identifies a singular Service Unit representing a SDV RPC Server or SDV DT Publisher.
#[comms_stack_ffi_struct(shared)]
#[repr(C)]
pub struct ASDV_UnitIdentifier {
    // ServiceIdentity for the service unit.
    pub(crate) service_identity: ASDV_ServiceIdentity,
    /// A valid ASCII encoded sequence of bytes ending with the NUL (\0) character.
    /// For more details check out the [Service Naming Conventions]
    ///(https://docs.partner.android.com/automotive/sdv/core-areas/naming-conventions)
    pub(crate) unit_name: [c_uchar; ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL],
}

impl Default for ASDV_UnitIdentifier {
    fn default() -> Self {
        ASDV_UnitIdentifier {
            service_identity: ASDV_ServiceIdentity::default(),
            unit_name: [0; ASDV_UNIT_NAME_MAX_SIZE_BYTES_INCLUDING_NUL], // Initialize with zeros
        }
    }
}

/// Check whether two `ASDV_ServicePublicKey`s are equal.
///
/// # Safety
///
/// The caller must ensure:
///
/// * `public_key_lhs` and `public_key_rhs` are valid, non-null pointers.
/// * Both `ServicePublicKey` structures remain valid for the duration of this function call.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServicePublicKey_equal(
    _public_key_lhs: &ASDV_ServicePublicKey,
    _public_key_rhs: &ASDV_ServicePublicKey,
    _result: &mut bool,
) -> sdv_status_t {
    unimplemented!()
}

/// Check whether two `ASDV_ServiceIdentity`s are equal.
///
/// # Error
///
/// This function returns `ASDV_STATUS_OK` if `service_identity_lhs` and `service_identity_rhs` are
/// valid. Otherwise it can return the following errors:
///
/// * `ASDV_STATUS_INVALID_ARGUMENT` in case `service_identity_lhs` or `service_identity_rhs` is not a valid ServiceIdentity value.
/// * `ASDV_STATUS_INTERNAL` if an unexpected error occurs during the function call.
///
/// # Safety
///
/// The caller must guarantee that:
///
/// * `service_identity_lhs` is a valid, non-null pointer to a `ASDV_ServiceIdentity` structure, and the pointed-to
/// instance remains valid for the duration of this function call.
/// * `service_identity_rhs` is a valid, non-null pointer to a `ASDV_ServiceIdentity` structure, and the pointed-to
/// instance remains valid for the duration of this function call.
/// * `result` is a valid, non-null pointer to a boolean.
#[comms_stack_ffi_fn]
#[no_mangle]
pub unsafe extern "C" fn ASDV_ServiceIdentity_equals(
    service_identity_lhs: &ASDV_ServiceIdentity,
    service_identity_rhs: &ASDV_ServiceIdentity,
    result: &mut bool,
) -> sdv_status_t {
    try_status! {
        let lhs = ServiceIdentity::try_from(service_identity_lhs)?;
        let rhs = ServiceIdentity::try_from(service_identity_rhs)?;
        *result = lhs == rhs;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_public_key_size() {
        assert_eq!(
            ASDV_SERVICE_PUBLIC_KEY_SIZE_BYTES,
            crate::service_identity::SERVICE_PUBLIC_KEY_SIZE_BYTES
        );
    }
}
