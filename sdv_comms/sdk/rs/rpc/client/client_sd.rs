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

use std::net::TcpStream;
use std::os::fd::OwnedFd;

use tokio::net::TcpStream as TokioTcpStream;

use async_trait::async_trait;
use binder_tokio::Tokio;
use google_sdv_identity::aidl::google::sdv::identity::IIdentityAgent::{
    BpIdentityAgent, IIdentityAgent, IIdentityAgentAsync,
};
use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinAidl;
use google_sdv_rpc::aidl::google::sdv::rpc::IRpcAgent::{
    BpRpcAgent, IRpcAgent, IRpcAgentAsync,
};
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::IServiceDiscoveryAgent::{
    BpServiceDiscoveryAgent, IServiceDiscoveryAgent, IServiceDiscoveryAgentAsync
};
use sdv_status_rs::SdvResult;
use sdv_status_rs::SdvStatus;
use sdv_status_rs::SdvStatusCode;

use crate::id::ServiceFqin;
use crate::rpc::client::internal::TargetServiceInfo;
use crate::sd::UnitType;

async fn get_rpc_agent() -> SdvResult<binder::Strong<dyn IRpcAgentAsync<Tokio>>> {
    let descriptor = <BpRpcAgent as IRpcAgent>::get_descriptor().to_owned() + "/default";
    let binder_service = binder_tokio::wait_for_interface::<dyn IRpcAgentAsync<Tokio>>(&descriptor);
    let rpc_agent = binder_service.await?;
    Ok(rpc_agent)
}

async fn get_service_discovery_agent() -> SdvResult<binder::Strong<dyn IServiceDiscoveryAgent>> {
    let descriptor = <BpServiceDiscoveryAgent as IServiceDiscoveryAgent>::get_descriptor()
        .to_owned()
        + "/default";
    let binder_service =
        binder_tokio::wait_for_interface::<dyn IServiceDiscoveryAgent>(&descriptor);
    let sd_agent = binder_service.await?;
    Ok(sd_agent)
}

async fn get_identity_agent() -> SdvResult<binder::Strong<dyn IIdentityAgent>> {
    let descriptor: String =
        <BpIdentityAgent as IIdentityAgent>::get_descriptor().to_owned() + "/default";
    let binder_service = binder_tokio::wait_for_interface::<dyn IIdentityAgent>(&descriptor);
    let identity_agent = binder_service.await?;
    Ok(identity_agent)
}

#[derive(Clone)]
pub struct SdvServiceDiscoverer;

#[async_trait]
impl crate::rpc::client::Connector for SdvServiceDiscoverer {
    async fn connect(&self, fqin: &ServiceFqin, unit_name: &str) -> SdvResult<TokioTcpStream> {
        let rpc_agent = get_rpc_agent().await?;
        let identity_agent: binder::Strong<dyn IIdentityAgentAsync<Tokio>> =
            get_identity_agent().await?.clone().into_async();

        let fqin = ServiceFqinAidl {
            sdvVmName: fqin.get_sdv_vm_name().to_string(),
            sdvPackageName: fqin.get_sdv_package_name().to_string(),
            serviceBundleName: fqin.get_service_bundle_name().to_string(),
            serviceInstanceName: fqin.get_service_instance_name().to_string(),
        };
        let identity = identity_agent.findIdentityByName(&fqin).await?.ok_or_else(|| {
            SdvStatus::with_message(SdvStatusCode::Internal, "Failed to find server identity")
        })?;
        let sid = identity.sid;
        let result_fd = rpc_agent.getFdConnection(sid, unit_name).await?;

        let fd: OwnedFd = result_fd.into();
        let std_stream = TcpStream::from(fd);
        let tcp_stream = match TokioTcpStream::from_std(std_stream) {
            Ok(tcp_stream) => tcp_stream,
            Err(err) => {
                let msg = format!("Could not convert FD to TCP Stream: {err}");
                return Err(SdvStatus::with_message(SdvStatusCode::Internal, msg));
            }
        };
        Ok(tcp_stream)
    }
}

// Performs request to the Service Discovery agent to fetch UnitType
// of an RPC server by a given FQIN and service unit name.
//
// Returns TargetServiceInfo which also contains input FQIN and service unit name.
pub async fn get_service_unit_type(
    fqin: &ServiceFqin,
    unit_name: &str,
) -> SdvResult<TargetServiceInfo> {
    let sd_agent = get_service_discovery_agent().await?;
    do_get_service_unit_type(fqin, unit_name, &sd_agent).await
}

async fn do_get_service_unit_type(
    fqin: &ServiceFqin,
    unit_name: &str,
    sd_agent: &binder::Strong<dyn IServiceDiscoveryAgent>,
) -> SdvResult<TargetServiceInfo> {
    let sd_agent: binder::Strong<dyn IServiceDiscoveryAgentAsync<Tokio>> =
        sd_agent.clone().into_async();

    let fqin_aidl = ServiceFqinAidl {
        sdvVmName: fqin.get_sdv_vm_name().to_string(),
        sdvPackageName: fqin.get_sdv_package_name().to_string(),
        serviceBundleName: fqin.get_service_bundle_name().to_string(),
        serviceInstanceName: fqin.get_service_instance_name().to_string(),
    };
    let unit_definition =
        sd_agent.getServiceUnit(&fqin_aidl, unit_name).await?.ok_or_else(|| {
            SdvStatus::with_message(SdvStatusCode::Internal, "Failed to get service unit")
        })?;
    let unit_type = unit_definition.unitType;

    let unit_type = UnitType::builder()
        .set_sdv_package_name(unit_type.sdvPackageName)
        .set_service_bundle_name(unit_type.serviceBundleName)
        .set_unit_type_name(unit_type.typeName)
        .build()
        .map_err(|e| {
            SdvStatus::with_cause_and_message(
                SdvStatusCode::Internal,
                e,
                "SD returned an invalid unit type",
            )
        })?;

    Ok(TargetServiceInfo { service_unit_name: unit_name.to_owned(), unit_type, fqin: fqin.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use binder::BinderFeatures;
    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::UnitType::UnitType as AidlUnitType;
    use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::{
        IServiceDiscoveryAgent::{BnServiceDiscoveryAgent, IServiceDiscoveryAgent, MockIServiceDiscoveryAgent},
        ServiceUnitDefinition::ServiceUnitDefinition as AidlServiceUnitDefinition,
    };

    const TEST_VM_NAME: &str = "vm";
    const TEST_PKG_NAME: &str = "com.google.pkg";
    const TEST_BUNDLE_NAME: &str = "Bundle";
    const TEST_INSTANCE_NAME: &str = "instance";
    const TEST_UNIT_NAME: &str = "test_unit";
    const TEST_UNIT_TYPE_NAME: &str = "Name";

    fn get_default_fqin() -> ServiceFqin {
        ServiceFqin::builder()
            .sdv_vm_name(TEST_VM_NAME)
            .sdv_package_name(TEST_PKG_NAME)
            .service_bundle_name(TEST_BUNDLE_NAME)
            .service_instance_name(TEST_INSTANCE_NAME)
            .build()
            .expect("Failed to create fqin")
    }

    fn get_default_service_unit_definition() -> AidlServiceUnitDefinition {
        AidlServiceUnitDefinition {
            unitType: AidlUnitType {
                sdvPackageName: TEST_PKG_NAME.to_string(),
                serviceBundleName: TEST_BUNDLE_NAME.to_string(),
                typeName: TEST_UNIT_TYPE_NAME.to_string(),
            },
            ..Default::default()
        }
    }

    fn setup_mock_sd_agent(
        response: Result<Option<AidlServiceUnitDefinition>, binder::Status>,
    ) -> binder::Strong<dyn IServiceDiscoveryAgent> {
        let mut mock = MockIServiceDiscoveryAgent::new();
        mock.expect_getServiceUnit().return_once(move |_fqin, _unit_name| response);
        BnServiceDiscoveryAgent::new_binder(mock, BinderFeatures::default())
    }

    #[tokio::test]
    async fn test_do_get_service_unit_type_success() {
        let unit_def = get_default_service_unit_definition();
        let mock_sd_agent_strong = setup_mock_sd_agent(Ok(Some(unit_def)));

        let result =
            do_get_service_unit_type(&get_default_fqin(), TEST_UNIT_NAME, &mock_sd_agent_strong)
                .await
                .unwrap();

        assert_eq!(result.service_unit_name, TEST_UNIT_NAME);

        // Assert FQIN fields (derived from identity's FQIN)
        assert_eq!(result.fqin.get_sdv_vm_name(), TEST_VM_NAME);
        assert_eq!(result.fqin.get_sdv_package_name(), TEST_PKG_NAME);
        assert_eq!(result.fqin.get_service_bundle_name(), TEST_BUNDLE_NAME);
        assert_eq!(result.fqin.get_service_instance_name(), TEST_INSTANCE_NAME);

        // Assert UnitType fields (derived from unit definition's unitType)
        assert_eq!(result.unit_type.get_sdv_package_name(), TEST_PKG_NAME);
        assert_eq!(result.unit_type.get_unit_type_name(), TEST_UNIT_TYPE_NAME);
    }

    #[tokio::test]
    async fn test_do_get_service_unit_type_cant_build_unit_type() {
        let unit_def = AidlServiceUnitDefinition {
            // Due to wrond name unit type can't be built.
            unitType: AidlUnitType {
                sdvPackageName: TEST_PKG_NAME.to_string(),
                serviceBundleName: TEST_BUNDLE_NAME.to_string(),
                typeName: "wrong-name%%$$$".to_string(),
            },
            ..Default::default()
        };
        let mock_sd_agent_strong = setup_mock_sd_agent(Ok(Some(unit_def)));

        let result =
            do_get_service_unit_type(&get_default_fqin(), TEST_UNIT_NAME, &mock_sd_agent_strong)
                .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("SD returned an invalid unit type"));
    }

    #[tokio::test]
    async fn test_do_get_service_unit_type_no_unit_definition() {
        // SD Agent doesn't return any service unit definition.
        let mock_sd_agent_strong = setup_mock_sd_agent(Ok(None));

        let result =
            do_get_service_unit_type(&get_default_fqin(), TEST_UNIT_NAME, &mock_sd_agent_strong)
                .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Failed to get service unit"));
    }
}
