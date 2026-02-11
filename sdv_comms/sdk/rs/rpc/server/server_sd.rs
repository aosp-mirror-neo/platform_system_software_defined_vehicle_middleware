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

use crate::sd::UnitRegistrationArgs;

use async_trait::async_trait;
use binder_tokio::Tokio;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::os::fd::OwnedFd;
use google_sdv_rpc::aidl::google::sdv::rpc::IRpcAgent::{
    BpRpcAgent, IRpcAgent, IRpcAgentAsync
};
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceRegistrationAgent::{
    BpServiceRegistrationAgent, IServiceRegistrationAgent, IServiceRegistrationAgentAsync
};
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::UnitType::UnitType;
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::AccessControl::AccessControl;
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::RegistrationToken::RegistrationToken;
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::ApplicationMetadata::ApplicationMetadata;
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::ApplicationMetadata::TypedMetadata::TypedMetadata::ServerUnit;
use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::ServerUnitMetadata::ServerUnitMetadata;

async fn get_rpc_agent() -> SdvResult<binder::Strong<dyn IRpcAgentAsync<Tokio>>> {
    let descriptor = <BpRpcAgent as IRpcAgent>::get_descriptor().to_owned() + "/default";
    let binder_service = binder_tokio::wait_for_interface::<dyn IRpcAgentAsync<Tokio>>(&descriptor);
    let rpc_agent = binder_service.await?;
    Ok(rpc_agent)
}

pub(crate) struct SdvServiceRegistrar {
    unit_reg_args: UnitRegistrationArgs,
    is_registered: bool,
}

#[async_trait]
impl crate::rpc::server::ServiceRegistrar for SdvServiceRegistrar {
    async fn register(&mut self) -> SdvResult<tokio::net::TcpListener> {
        if self.is_registered {
            return Err(SdvStatus::with_message(
                SdvStatusCode::Internal,
                "Trying register the unit that is already registered",
            ));
        }

        let owned_fd = match register_service(&self.unit_reg_args).await {
            Ok(fd) => fd,
            Err(err) => return Err(err),
        };

        self.is_registered = true;

        let std_listener: std::net::TcpListener = std::net::TcpListener::from(owned_fd);

        let listener: tokio::net::TcpListener =
            match tokio::net::TcpListener::from_std(std_listener) {
                Ok(listener) => listener,
                Err(_e) => {
                    log::warn!("Could not create a TCP listener");
                    return Err(SdvStatus::with_message(
                        SdvStatusCode::Unavailable,
                        "Could not create a TCP listener",
                    ));
                }
            };

        Ok(listener)
    }
}

impl Drop for SdvServiceRegistrar {
    fn drop(&mut self) {
        if self.is_registered {
            unregister_blocking(&self.unit_reg_args.get_unit_name());
            self.is_registered = false;
        }
    }
}

impl SdvServiceRegistrar {
    pub(crate) fn new(unit_reg_args: UnitRegistrationArgs) -> SdvServiceRegistrar {
        SdvServiceRegistrar { unit_reg_args, is_registered: false }
    }
}

async fn get_registration_agent(
) -> SdvResult<binder::Strong<dyn IServiceRegistrationAgentAsync<Tokio>>> {
    let descriptor = <BpServiceRegistrationAgent as IServiceRegistrationAgent>::get_descriptor()
        .to_owned()
        + "/default";
    let binder_service =
        binder_tokio::wait_for_interface::<dyn IServiceRegistrationAgentAsync<Tokio>>(&descriptor);
    let registration_agent = binder_service.await?;
    Ok(registration_agent)
}

fn get_registration_agent_blocking() -> SdvResult<binder::Strong<dyn IServiceRegistrationAgent>> {
    let descriptor = <BpServiceRegistrationAgent as IServiceRegistrationAgent>::get_descriptor()
        .to_owned()
        + "/default";
    let registration_agent = binder::get_interface::<dyn IServiceRegistrationAgent>(&descriptor);

    Ok(registration_agent.unwrap())
}

// Register service unit and obtain registration token as registration confirmation.
async fn request_registration_token(
    unit_registration_args: &UnitRegistrationArgs,
) -> SdvResult<RegistrationToken> {
    let registration_agent = get_registration_agent().await?;
    let registered_unit_type = unit_registration_args.get_unit_type();
    let unit_type = UnitType {
        sdvPackageName: registered_unit_type.get_sdv_package_name().to_string(),
        serviceBundleName: "".to_string(), // service bundle name as part of unit types is deprecated
        typeName: registered_unit_type.get_unit_type_name().to_string(),
    };
    let acl: AccessControl = AccessControl { connectAcls: Vec::new() };
    let application_metadata = ApplicationMetadata {
        typedMetadata: ServerUnit(ServerUnitMetadata { version: 1 }),
        valueHolder: unit_registration_args.get_application_metadata()?,
    };
    let token = registration_agent
        .registerServiceUnit(
            &unit_registration_args.get_unit_name(),
            &unit_type,
            &acl,
            &application_metadata,
        )
        .await?;
    Ok(token)
}

/// Registers the service in service discovery. At this moment it works with IP and Port.
pub async fn register_service(unit_registration_args: &UnitRegistrationArgs) -> SdvResult<OwnedFd> {
    let token = request_registration_token(unit_registration_args).await?;

    let rpc_agent = get_rpc_agent().await?;
    let fd = rpc_agent.registerServer(&token).await?;
    Ok(fd.into())
}

fn unregister_blocking(service_unit_name: &str) {
    let reg_agent = get_registration_agent_blocking().unwrap();
    reg_agent.unregisterServiceUnit(service_unit_name).unwrap();
}

#[cfg(test)]
mod tests {
    use lazy_static::lazy_static;
    use sdv_status_rs::SdvStatusCode;
    use std::str::FromStr;

    use crate::id::ServiceFqin;
    use crate::rpc::server::SdvServiceRegistrar;
    use crate::rpc::server::ServiceRegistrar;
    use crate::sd::{ServerUnitMetadata, UnitMetadata, UnitRegistrationArgs, UnitType};
    use crate::ContextRef;
    use binder::binder_impl::Binder;
    use binder::Interface;

    use crate::rpc::server::server_sd::get_rpc_agent;
    use crate::rpc::server::server_sd::request_registration_token;

    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::RegistrationToken::RegistrationToken;

    use google_sdv_identity::aidl::google::sdv::identity::{
        ServiceFqin::ServiceFqin as ServiceFqinAidl, ServiceIdentity::EcPublicKey::EcPublicKey,
        ServiceIdentity::ServiceIdentity,
    };

    // Check that the server can't obtain the FD if it present a token that is not
    // known to service discovery. At the moment the token is checked in
    // system/software_defined_vehicle/middleware/service_discovery/sdv_sd_agent/srcs/kv.rs
    #[tokio::test]
    async fn test_wrong_registration_token() {
        // Build the token is a way similar to
        // system/software_defined_vehicle/middleware/service_discovery/sdv_sd_agent/srcs/kv.rs
        let service_identity = ServiceIdentity {
            name: ServiceFqinAidl {
                sdvVmName: "vmname".to_string(),
                sdvPackageName: "com.example.def".to_string(),
                serviceBundleName: "S1".to_string(),
                serviceInstanceName: "p1".to_string(),
            },
            sid: 123123123123,
            publicKey: EcPublicKey { keyValue: *b"01234567890123456789012345678912" },
        };

        let binder_token = Binder::new(()).as_binder();
        let token = RegistrationToken { identity: service_identity, token: Some(binder_token) };

        let rpc_agent =
            get_rpc_agent().await.expect("It is expected for RPC agent to be reachable");
        let fd = rpc_agent.registerServer(&token).await;

        assert!(fd.is_err());
    }

    static UNIT_NAME: &str = "unit-name";
    static UNIT_TYPE_STR: &str = "com.google.sdv.test.UnitType#Type";

    /// The first part of a service's FQIN. Each test should compose their own service FQIN to avoid
    /// interfering with each other, given that they all access the same sdv_sd_agent.
    static FQIN_PREFIX: &str = "vm-name:com.android.sdv.RpcTest";

    lazy_static! {
        static ref UNIT_TYPE: UnitType = UnitType::from_str(UNIT_TYPE_STR).expect("Valid UnitType");
    }

    fn make_unit_registration_args() -> UnitRegistrationArgs {
        let mut server_unit_metadata = ServerUnitMetadata::new();
        server_unit_metadata.set_version(1);
        let mut unit_registration_args = UnitRegistrationArgs::new();
        unit_registration_args.set_unit_metadata(&UnitMetadata::Server(server_unit_metadata));
        unit_registration_args.set_unit_name(UNIT_NAME);
        unit_registration_args.set_unit_type(&UNIT_TYPE);
        unit_registration_args
    }

    #[tokio::test]
    async fn test_modified_registration_token() {
        let fqin =
            ServiceFqin::from_str(&format!("{FQIN_PREFIX}ModifiedRegToken/instance")).unwrap();
        let _ctx = ContextRef::create(fqin);

        let ura = make_unit_registration_args();

        let token =
            request_registration_token(&ura).await.expect("A registration token is expected");

        let rpc_agent =
            get_rpc_agent().await.expect("It is expected for RPC agent to be reachable");

        // Modification of the registration token should not lead to open FD,
        // but if we modify the identity part of the token, then it is accepted
        // by SD.
        // TODO (b/396395509): Uncomment or delete the code blow as soon as the
        // behavior is fixed.
        // let mut id = token.identity.clone();
        // id.name.sdvVmName = "wrongvmname".to_string();
        // let mut t = token.clone();
        // t.identity = id;
        // let fd = rpc_agent.registerServer(&t).await;
        // assert!(fd.is_err());

        // Check that modification of the token field leads to the
        // FD issuing error.
        let mut t = token.clone();
        t.token = Some(Binder::new(()).as_binder());
        let fd = rpc_agent.registerServer(&t).await;
        assert!(fd.is_err());

        // Check that original token still works.
        let fd = rpc_agent.registerServer(&token).await;
        assert!(fd.is_ok());
    }

    #[tokio::test]
    async fn test_register_already_registered_error() {
        let fqin =
            ServiceFqin::from_str(&format!("{FQIN_PREFIX}AlreadyRegistered/instance")).unwrap();
        // _ctx needs to be held to keep the Binder context alive for the test.
        let _ctx = ContextRef::create(fqin);

        let ura = make_unit_registration_args();
        let mut registrar = SdvServiceRegistrar::new(ura);

        // First call: should succeed and set is_registered to true
        let result1 = registrar.register().await;
        assert!(result1.is_ok(), "First registration should succeed: {:?}", result1.err());

        // Second call: should fail because it's already registered
        let result2 = registrar.register().await;
        assert!(result2.is_err(), "Second registration should fail");
        assert_eq!(result2.unwrap_err().status_code(), SdvStatusCode::Internal);
    }
}
