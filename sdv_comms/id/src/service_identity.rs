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

//! This file declares the Service Identity object.

use google_sdv_identity::aidl::google::sdv::identity::{
    IIdentityAgent::BpIdentityAgent, IIdentityAgent::IIdentityAgent,
    ProcessIdentifier::ProcessIdentifier,
};

use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::EcPublicKey::EcPublicKey as EcPublicKeyAidl;
use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::ServiceIdentity as ServiceIdentityAidl;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};

use crate::ffi::ASDV_ServiceIdentity;
use crate::{ServiceFqin, ServiceFqinValidationError};

use binder::{binder_impl::Binder, Interface, ThreadState};

/// Size in bytes of the X25519 Service Public Key.
pub const SERVICE_PUBLIC_KEY_SIZE_BYTES: usize = 32;

/// Defines the PublicKey of a Service Bundle.
#[derive(Clone, Debug, PartialEq)]
pub struct ServicePublicKey([u8; SERVICE_PUBLIC_KEY_SIZE_BYTES]);

impl ServicePublicKey {
    /// Creates a new Keypair.
    pub fn forge() -> ([u8; SERVICE_PUBLIC_KEY_SIZE_BYTES], ServicePublicKey) {
        // TODO: Replace with BoringSSL functions
        ([0_u8; 32], ServicePublicKey([0_u8; 32]))
    }
}

impl TryFrom<EcPublicKeyAidl> for ServicePublicKey {
    type Error = SdvStatus;

    fn try_from(aidl: EcPublicKeyAidl) -> Result<Self, Self::Error> {
        Ok(ServicePublicKey(aidl.keyValue))
    }
}

impl From<ServicePublicKey> for EcPublicKeyAidl {
    fn from(value: ServicePublicKey) -> Self {
        let mut ec_key_aidl = EcPublicKeyAidl::default();
        ec_key_aidl.keyValue.copy_from_slice(&value.0);
        ec_key_aidl
    }
}

/// Defines the Service Identity of a Service Bundle.
#[derive(Clone, Debug, PartialEq)]
pub struct ServiceIdentity {
    sid: u64,
    fqin: ServiceFqin,
    pub_key: ServicePublicKey,
}

impl ServiceIdentity {
    /// Returns the globally unique 64 bit identifier of this ServiceIdentity.
    pub fn sid(&self) -> u64 {
        self.sid
    }

    /// Returns the Fully Qualified Instance Name of the Service Bundle identified by this ServiceIdentity.
    pub fn fqin(&self) -> &ServiceFqin {
        &self.fqin
    }

    /// Returns the Public Key for the Service Bundle identified by this ServiceIdentity.
    pub fn pub_key(&self) -> &ServicePublicKey {
        &self.pub_key
    }

    /// Clones this ServiceIdentity instance into an ASDV_ServiceIdentity.
    pub fn clone_into(&self, asdv_identity: &mut ASDV_ServiceIdentity) -> SdvResult<()> {
        asdv_identity.sid = self.sid;
        self.fqin.clone_into(&mut asdv_identity.fqin)?;
        asdv_identity.public_key.clone_from_slice(&self.pub_key.0);
        Ok(())
    }
}

impl TryFrom<ServiceIdentityAidl> for ServiceIdentity {
    type Error = SdvStatus;

    fn try_from(aidl: ServiceIdentityAidl) -> Result<Self, Self::Error> {
        fn map_validation_err(err: ServiceFqinValidationError) -> SdvStatus {
            SdvStatus::with_message(SdvStatusCode::Internal, err.to_string())
        }

        Ok(ServiceIdentity {
            sid: aidl.sid as u64,
            fqin: aidl.name.try_into().map_err(map_validation_err)?,
            pub_key: aidl.publicKey.try_into()?,
        })
    }
}

/// Converts ASDV_ServiceIdentity to ServiceIdentity struct.
impl TryFrom<&ASDV_ServiceIdentity> for ServiceIdentity {
    type Error = SdvStatus;

    fn try_from(asdv_service_identity: &ASDV_ServiceIdentity) -> SdvResult<ServiceIdentity> {
        Ok(ServiceIdentity {
            sid: asdv_service_identity.sid,
            fqin: ServiceFqin::try_from(&asdv_service_identity.fqin)?,
            pub_key: ServicePublicKey(asdv_service_identity.public_key),
        })
    }
}

impl From<ServiceIdentity> for ServiceIdentityAidl {
    fn from(value: ServiceIdentity) -> Self {
        ServiceIdentityAidl {
            sid: value.sid as i64,
            name: value.fqin.into(),
            publicKey: value.pub_key.into(),
        }
    }
}

/// Registers a new [ServiceIdentity] for the provided [ServiceFqin].
pub fn register_new_identity(
    fqin: ServiceFqin,
    pub_key: ServicePublicKey,
) -> SdvResult<ServiceIdentity> {
    let descriptor = <BpIdentityAgent as IIdentityAgent>::get_descriptor().to_owned() + "/default";
    let identity_agent: binder::Strong<dyn IIdentityAgent> = binder::check_interface(&descriptor)?;

    // The death notification happens not when local pointer goes out of scope, but when process dies.
    let link_to_death = Binder::new(()).as_binder();
    let identity_aidl =
        identity_agent.createIdentity(&pub_key.into(), &fqin.into(), &link_to_death)?;
    identity_aidl.try_into()
}

/// Returns an identity for a caller of a currently executed binder procedure.
pub fn get_binder_caller_process_identity() -> SdvResult<ServiceIdentity> {
    let descriptor = <BpIdentityAgent as IIdentityAgent>::get_descriptor().to_owned() + "/default";
    let identity_agent: binder::Strong<dyn IIdentityAgent> = binder::check_interface(&descriptor)?;

    let process_identifier = ProcessIdentifier {
        pid: ThreadState::get_calling_pid() as i64,
        uid: ThreadState::get_calling_uid() as i64,
    };

    let Some(identity_aidl) = identity_agent.findIdentityByProcess(&process_identifier)? else {
        return Err(SdvStatus::with_message(
            SdvStatusCode::NotFound,
            "Identity for process not found",
        ));
    };

    identity_aidl.try_into()
}

#[cfg(test)]
mod tests {
    use super::*;

    use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinAidl;

    #[test]
    fn test_service_identity_try_from_aidl_success() {
        let aidl = ServiceIdentityAidl {
            sid: 12345_i64,
            name: ServiceFqinAidl {
                sdvVmName: "vm-test".to_string(),
                sdvPackageName: "com.google.sdv".to_string(),
                serviceBundleName: "TestBundleName".to_string(),
                serviceInstanceName: "default".to_string(),
            },
            publicKey: EcPublicKeyAidl { keyValue: [45_u8; SERVICE_PUBLIC_KEY_SIZE_BYTES] },
        };

        let id = ServiceIdentity::try_from(aidl).unwrap();

        assert_eq!(id.sid(), 12345_u64);
        let fqin: &ServiceFqin = id.fqin();
        assert_eq!(fqin.to_string(), "vm-test:com.google.sdv.TestBundleName/default");
        assert_eq!(id.pub_key(), &ServicePublicKey([45_u8; SERVICE_PUBLIC_KEY_SIZE_BYTES]));
    }

    #[test]
    fn test_service_identity_try_from_aidl_failed_with_fqin_parse_error() {
        let aidl = ServiceIdentityAidl {
            sid: 12345_i64,
            name: ServiceFqinAidl {
                sdvVmName: "".to_string(),
                sdvPackageName: "".to_string(),
                serviceBundleName: "".to_string(),
                serviceInstanceName: "".to_string(),
            },
            publicKey: EcPublicKeyAidl { keyValue: [45_u8; SERVICE_PUBLIC_KEY_SIZE_BYTES] },
        };

        let id = ServiceIdentity::try_from(aidl);

        assert!(id.is_err());
    }

    #[test]
    fn test_service_identity_into_aidl_success() {
        let aidl = ServiceIdentityAidl {
            sid: 12345_i64,
            name: ServiceFqinAidl {
                sdvVmName: "vm-test".to_string(),
                sdvPackageName: "com.google.sdv".to_string(),
                serviceBundleName: "TestBundleName".to_string(),
                serviceInstanceName: "default".to_string(),
            },
            publicKey: EcPublicKeyAidl { keyValue: [45_u8; SERVICE_PUBLIC_KEY_SIZE_BYTES] },
        };
        let id = ServiceIdentity::try_from(aidl.clone()).unwrap();

        let result: ServiceIdentityAidl = id.into();

        assert_eq!(result, aidl);
    }
}
