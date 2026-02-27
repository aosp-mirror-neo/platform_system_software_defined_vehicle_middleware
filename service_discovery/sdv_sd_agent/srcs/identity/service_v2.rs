// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use google_sdv_identity::aidl::google::sdv::identity::IIdentityAgent::IIdentityAgentAsyncServer as IIdentityAgentV2AsyncServer;
use google_sdv_identity::binder::{ExceptionCode, Interface, Result as BinderResult};
use google_sdv_privileged_identity::aidl::google::sdv::privileged::identity::IPrivilegedIdentityAgent::IPrivilegedIdentityAgentAsyncServer as IPrivilegedIdentityAgentAsyncServer;
use google_sdv_privileged_identity::binder::Result as GatewayBinderResult;

use google_sdv_identity::aidl::google::sdv::identity::{
    ProcessIdentifier::ProcessIdentifier as ProcessIdentifierAidl,
    ServiceFqin::ServiceFqin as ServiceFqinV2Aidl,
    ServiceIdentity::EcPublicKey::EcPublicKey as EcPublicKeyAidl,
    ServiceIdentity::ServiceIdentity as ServiceIdentityV2Aidl,
};

use crate::identity::id_gen::SidGenerator;
use crate::identity::lazy_identity_verifier::{IdentityVerifierProvider, LazyIdentityVerifier};
use crate::kv::death_watcher::{self, DeathWatcher};
use crate::kv::process_identifier::get_calling_process_identifier;
use crate::kv::ServiceRegistry;
use crate::mesh::VvmConfig;
use native_services_names::NativeServicesNames;

use sdv_comms_id::{names::VmName, ServiceFqin};

use anyhow::anyhow;
use anyhow::ensure;
use async_trait::async_trait;
use binder::{DeathRecipient, IBinder, SpIBinder, Status};

use std::fmt;
use std::sync::Arc;

use log::error;
use tracing::instrument;

pub(crate) struct IdentityAgentV2 {
    service_registry: Arc<ServiceRegistry>,
    identity_verifier: Box<dyn IdentityVerifierProvider>,
    native_services_names: Arc<NativeServicesNames>,
    sid_gen: SidGenerator,
    self_vm_name: VmName,
}

/// Struct that combines callback and client Binder
/// It should be alive while we are waiting for a callback.
struct DeathWatcherImpl {
    client_binder: SpIBinder,
    callback: Option<DeathRecipient>,
}

impl DeathWatcherImpl {
    fn new(client_binder: SpIBinder) -> Self {
        Self { client_binder, callback: None }
    }
}

impl DeathWatcher for DeathWatcherImpl {
    fn link_to_death(&mut self, callback: death_watcher::Callback) -> anyhow::Result<()> {
        let mut recipient = DeathRecipient::new(callback);

        // Only adds a weak pointer, so we store recipient to keep it alive
        self.client_binder
            .link_to_death(&mut recipient)
            .map_err(|_| anyhow!("Failed to link to death"))?;

        self.callback = Some(recipient);
        Ok(())
    }
}

// We want short string representation in logs without extra details
impl fmt::Debug for DeathWatcherImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeathWatcher({})", if self.callback.is_some() { "cb" } else { "empty" })
    }
}

impl IdentityAgentV2 {
    pub(crate) fn new(
        service_registry: Arc<ServiceRegistry>,
        vvm_config: &VvmConfig,
    ) -> anyhow::Result<Self> {
        let self_vm_name = vvm_config.own_cfg.name.clone();
        Ok(IdentityAgentV2 {
            service_registry,
            identity_verifier: Box::new(LazyIdentityVerifier::default()),
            native_services_names: Arc::new(NativeServicesNames::new()?),
            sid_gen: SidGenerator::try_new(vvm_config)?,
            self_vm_name,
        })
    }

    pub(crate) fn new_shallow_copy(source: &IdentityAgentV2) -> Self {
        IdentityAgentV2 {
            service_registry: source.service_registry.clone(),
            identity_verifier: Box::new(LazyIdentityVerifier::default()),
            native_services_names: source.native_services_names.clone(),
            sid_gen: source.sid_gen.clone(),
            self_vm_name: source.self_vm_name.clone(),
        }
    }

    fn create_identity(
        &self,
        pub_key: &EcPublicKeyAidl,
        aidl_fqin: &ServiceFqinV2Aidl,
    ) -> BinderResult<ServiceIdentityV2Aidl> {
        let mut aidl_fqin = aidl_fqin.clone();
        aidl_fqin.sdvVmName = self.self_vm_name.clone().into();

        let fqin = ServiceFqin::try_from(aidl_fqin.clone()).map_err(|err| {
            error!("Rejecting registration for invalid FQIN {aidl_fqin:?} ({err})");
            Status::new_exception_str(ExceptionCode::ILLEGAL_ARGUMENT, Some(err.to_string()))
        })?;

        let is_sid_taken_closure = |sid_candidate: u64| {
            self.service_registry.find_identity_by_sid_v2(sid_candidate as i64).is_some()
        };

        let sid = self.sid_gen.generate_sid(&fqin, &is_sid_taken_closure).map_err(|err| {
            error!("Failed to generate SID for {fqin:?}: {err}");
            Status::new_service_specific_error_str(
                -1,
                Some(format!("Failed to generate SID: {err}")),
            )
        })?;

        Ok(ServiceIdentityV2Aidl { sid: sid as i64, publicKey: pub_key.clone(), name: aidl_fqin })
    }

    fn verify_fqin_for_process(
        &self,
        process_identifier: &ProcessIdentifierAidl,
        provided_fqin: &ServiceFqinV2Aidl,
    ) -> anyhow::Result<()> {
        log::info!("Checking registration of FQIN {provided_fqin:?} for {process_identifier:?}");

        let uid: u32 = process_identifier.uid.try_into()?;

        // root is used by tests, so it's blanket allowed for all service FQINs.
        // TODO: b/444007463 - do not allow root in user builds
        if uid == sdv_aid::AID_ROOT {
            return Ok(());
        }

        use google_sdv_identity::aidl::google::sdv::identity::LocalServiceFqin::LocalServiceFqin;

        let provided_local_fqin = LocalServiceFqin {
            sdvPackageName: provided_fqin.sdvPackageName.clone(),
            serviceBundleName: provided_fqin.serviceBundleName.clone(),
            serviceInstanceName: provided_fqin.serviceInstanceName.clone(),
        };

        if self
            .native_services_names
            .is_local_fqin_allowed_for_service_uid(uid, &provided_local_fqin)
        {
            return Ok(());
        }

        let identity_verifier = self.identity_verifier.get()?;

        let passed_verification: bool =
            identity_verifier.verifyFqinForProcess(&provided_local_fqin, process_identifier)?;

        ensure!(passed_verification);

        Ok(())
    }
}

impl Interface for IdentityAgentV2 {}

#[allow(non_snake_case)] // generated AIDL interface
#[async_trait]
impl IIdentityAgentV2AsyncServer for IdentityAgentV2 {
    #[instrument(name = "sd::IIdentityAgentV2::createIdentity", skip_all)]
    async fn createIdentity(
        &self,
        pub_key: &EcPublicKeyAidl,
        provided_fqin: &ServiceFqinV2Aidl,
        client_binder: &SpIBinder,
    ) -> BinderResult<ServiceIdentityV2Aidl> {
        let process_identifier = get_calling_process_identifier();

        self.verify_fqin_for_process(&process_identifier, provided_fqin).map_err(|err| {
            error!("Rejecting registration of FQIN {provided_fqin:?} for {process_identifier:?} ({err})");
            Status::new_exception_str(ExceptionCode::SECURITY, Some(err.to_string()))
        })?;

        let new_identity = self.create_identity(pub_key, provided_fqin)?;

        match self.service_registry.register_identity_v2(
            new_identity.clone(),
            Some(Box::new(DeathWatcherImpl::new(client_binder.clone()))),
            &process_identifier,
        ) {
            Ok(None) => Ok(new_identity),
            Ok(Some(existing_identity)) => Ok(existing_identity),
            Err(err) => {
                let msg = format!("Failed to create identity: {err:?}");
                error!("{msg}");
                Err(Status::new_service_specific_error_str(-1, Some(msg)))
            }
        }
    }

    #[instrument(name = "sd::IIdentityAgentV2::verifyIdentity", skip_all)]
    async fn verifyIdentity(&self, identity: &ServiceIdentityV2Aidl) -> BinderResult<bool> {
        if let Some(found_identity) = self.service_registry.find_identity_by_sid_v2(identity.sid) {
            return Ok(found_identity == *identity);
        }
        Ok(false)
    }

    #[instrument(name = "sd::IIdentityAgentV2::findIdentityByUid", skip_all)]
    async fn findIdentityByUid(&self, sid: i64) -> BinderResult<Option<ServiceIdentityV2Aidl>> {
        Ok(self.service_registry.find_identity_by_sid_v2(sid))
    }

    #[instrument(name = "sd::IIdentityAgentV2::findIdentityByName", skip_all)]
    async fn findIdentityByName(
        &self,
        name: &ServiceFqinV2Aidl,
    ) -> BinderResult<Option<ServiceIdentityV2Aidl>> {
        Ok(self.service_registry.find_identity_by_fqin_v2(name))
    }

    #[instrument(name = "sd::IIdentityAgentV2::findIdentityByProcess", skip_all)]
    async fn findIdentityByProcess(
        &self,
        process_identifier: &ProcessIdentifierAidl,
    ) -> BinderResult<Option<ServiceIdentityV2Aidl>> {
        Ok(self.service_registry.find_identity_by_process_identifier(process_identifier))
    }
}

#[allow(non_snake_case)] // generated AIDL interface
#[async_trait]
impl IPrivilegedIdentityAgentAsyncServer for IdentityAgentV2 {
    #[instrument(
        name = "sd::IPrivilegedIdentityAgentAsyncServer::createIdentityForProcess",
        skip_all
    )]
    async fn createIdentityForProcess(
        &self,
        pub_key: &EcPublicKeyAidl,
        provided_fqin: &ServiceFqinV2Aidl,
        client_binder: &SpIBinder,
        process_identifier: &ProcessIdentifierAidl,
    ) -> GatewayBinderResult<ServiceIdentityV2Aidl> {
        // Not calling verify_fqin_for_process(), because, on the
        // IPrivilegedIdentityAgentAsyncServer, the caller is trusted to provide correct FQIN.

        let new_identity = self.create_identity(pub_key, provided_fqin)?;

        match self.service_registry.register_identity_v2(
            new_identity.clone(),
            Some(Box::new(DeathWatcherImpl::new(client_binder.clone()))),
            process_identifier,
        ) {
            Ok(None) => Ok(new_identity),
            Ok(Some(existing_identity)) => Ok(existing_identity),
            Err(_) => Err(Status::new_service_specific_error(-1, None)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::lazy_identity_verifier::MockIdentityVerifierProvider;
    use crate::kv::ServiceRegistry;
    use crate::mesh::vvm_config::{create_test_uds_ca_pub, VmConfig};

    use binder::{BinderFeatures, Strong};
    use std::default::Default;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use vvmtruststore::CertificateRevocationList;

    use google_sdv_identity::aidl::google::sdv::identity::ServiceFqin::ServiceFqin as ServiceFqinV2Aidl;
    use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::EcPublicKey::EcPublicKey as EcPublicKeyAidl;

    use google_sdv_identity_internal::aidl::google::sdv::identity::internal::IIdentityVerifier::{
        BnIdentityVerifier, IIdentityVerifier, MockIIdentityVerifier,
    };

    const VM_INSTANCE_1_NAME: &str = "instance1";

    /// Helper function to create a VvmConfig for testing
    fn create_fake_vvmconfig() -> VvmConfig {
        VvmConfig {
            uds_ca_pub: create_test_uds_ca_pub().unwrap(),
            uds_ca_rev_list: CertificateRevocationList::default(),
            policies: vec![],
            remote_cfgs: vec![], // No remote VMs needed for this test
            own_cfg: VmConfig::new_for_testing(
                VM_INSTANCE_1_NAME.to_owned().try_into().unwrap(),
                // Arbitrary address
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 98, 3)), 2001),
            ),
        }
    }

    fn create_native_services_names_for_test() -> NativeServicesNames {
        NativeServicesNames::from_entries(&[(
            sdv_aid::AID_SYSTEM,
            "com.android.sdv.update_manager.UpdateManagerAgent/default",
        )])
    }

    fn create_identity_manager_for_test() -> anyhow::Result<IdentityAgentV2> {
        let vvm_config = create_fake_vvmconfig();
        let self_vm_name = vvm_config.own_cfg.name.clone();
        Ok(IdentityAgentV2 {
            service_registry: Arc::new(ServiceRegistry::default()),
            identity_verifier: Box::new(LazyIdentityVerifier::default()),
            native_services_names: Arc::new(create_native_services_names_for_test()),
            sid_gen: SidGenerator::try_new(&vvm_config)?,
            self_vm_name,
        })
    }

    fn create_identity_manager_with_mock_verifier_provider(
        identity_verifier: MockIdentityVerifierProvider,
    ) -> anyhow::Result<IdentityAgentV2> {
        let config = create_fake_vvmconfig();
        let self_vm_name = config.own_cfg.name.clone();
        Ok(IdentityAgentV2 {
            service_registry: Arc::new(ServiceRegistry::default()),
            identity_verifier: Box::new(identity_verifier),
            sid_gen: SidGenerator::try_new(&config)?,
            self_vm_name,
            native_services_names: Arc::new(create_native_services_names_for_test()),
        })
    }

    fn create_identity_manager_with_verifier_expectations<F>(
        expectations: F,
    ) -> anyhow::Result<IdentityAgentV2>
    where
        F: Fn(&mut MockIIdentityVerifier) + Send + 'static,
    {
        let mut mock_verifier_provider = MockIdentityVerifierProvider::new();
        mock_verifier_provider.expect_get().returning(move || {
            let mut mock_verifier = MockIIdentityVerifier::new();
            expectations(&mut mock_verifier);
            let mock_service: Strong<dyn IIdentityVerifier> =
                BnIdentityVerifier::new_binder(mock_verifier, BinderFeatures::default());
            Ok(mock_service)
        });

        create_identity_manager_with_mock_verifier_provider(mock_verifier_provider)
    }

    fn create_test_fqin() -> ServiceFqinV2Aidl {
        ServiceFqinV2Aidl {
            sdvVmName: "vm".to_string(),
            sdvPackageName: "com.example".to_string(),
            serviceBundleName: "s".to_string(),
            serviceInstanceName: "i".to_string(),
        }
    }

    fn get_untrusted_uid() -> i64 {
        // An arbitrary UID that is out of the range that is trusted by default.
        (sdv_aid::AID_SYSTEM_EXT_RESERVED_END + 1).try_into().unwrap()
    }

    #[test]
    fn create_identity() -> anyhow::Result<()> {
        let identity_manager = create_identity_manager_for_test()?;

        let public_key = EcPublicKeyAidl { keyValue: *b"01234567890123456789012345678912" };
        let fqin = ServiceFqinV2Aidl {
            sdvVmName: VM_INSTANCE_1_NAME.to_string(),
            sdvPackageName: "com.example.def".to_string(),
            serviceBundleName: "S1".to_string(),
            serviceInstanceName: "p1".to_string(),
        };

        let result = identity_manager.create_identity(&public_key, &fqin).unwrap();
        assert_eq!(result.name.sdvVmName, VM_INSTANCE_1_NAME);
        assert_eq!(result.name.sdvPackageName, fqin.sdvPackageName);
        assert_eq!(result.name.serviceBundleName, fqin.serviceBundleName);
        assert_eq!(result.name.serviceInstanceName, fqin.serviceInstanceName);
        assert_eq!(result.publicKey, public_key);

        Ok(())
    }

    #[test]
    fn create_identity_overwrites_wrong_vm_name() -> anyhow::Result<()> {
        let identity_manager = create_identity_manager_for_test()?;
        let public_key = EcPublicKeyAidl { keyValue: *b"01234567890123456789012345678912" };
        let fqin = ServiceFqinV2Aidl {
            sdvVmName: "vm2".to_string(),
            sdvPackageName: "com.example.def_2".to_string(),
            serviceBundleName: "S2".to_string(),
            serviceInstanceName: "p2".to_string(),
        };

        let result = identity_manager.create_identity(&public_key, &fqin).unwrap();
        assert_eq!(result.name.sdvVmName, VM_INSTANCE_1_NAME);
        assert_eq!(result.name.sdvPackageName, fqin.sdvPackageName);
        assert_eq!(result.name.serviceBundleName, fqin.serviceBundleName);
        assert_eq!(result.name.serviceInstanceName, fqin.serviceInstanceName);
        assert_eq!(result.publicKey, public_key);

        Ok(())
    }

    #[test]
    fn create_identity_fails_on_invalid_fqin() -> anyhow::Result<()> {
        let identity_manager = create_identity_manager_for_test()?;
        let public_key = EcPublicKeyAidl { keyValue: *b"01234567890123456789012345678912" };
        let fqin = ServiceFqinV2Aidl {
            sdvVmName: "vm2".to_string(),
            sdvPackageName: "".to_string(),
            serviceBundleName: "s2".to_string(),
            serviceInstanceName: "p2".to_string(),
        };

        let create_err = identity_manager.create_identity(&public_key, &fqin).expect_err(
            "create_identity should have failed for invalid FQIN when authz enforcement enabled.",
        );
        assert_eq!(
            create_err.to_string(),
            "Status(-3, EX_ILLEGAL_ARGUMENT): 'invalid Package Name: should not be empty'"
        );
        Ok(())
    }

    #[test]
    fn verify_fqin_for_process_succeeds_for_native_service() -> anyhow::Result<()> {
        let identity_manager = create_identity_manager_for_test()?;
        let update_manager_uid = sdv_aid::AID_SYSTEM.try_into().unwrap();
        let update_manager_agent_fqin = ServiceFqinV2Aidl {
            sdvVmName: "vm".to_string(),
            sdvPackageName: "com.android.sdv.update_manager".to_string(),
            serviceBundleName: "UpdateManagerAgent".to_string(),
            serviceInstanceName: "default".to_string(),
        };
        let process_identifier = ProcessIdentifierAidl { pid: 1, uid: update_manager_uid };
        identity_manager
            .verify_fqin_for_process(&process_identifier, &update_manager_agent_fqin)?;
        Ok(())
    }

    #[test]
    fn verify_fqin_for_process_fails_for_unknown_native_service_uid() -> anyhow::Result<()> {
        let identity_manager = create_identity_manager_for_test()?;
        let process_identifier = ProcessIdentifierAidl { pid: 1, uid: get_untrusted_uid() };
        assert!(identity_manager
            .verify_fqin_for_process(&process_identifier, &create_test_fqin())
            .is_err());
        Ok(())
    }

    #[test]
    fn verify_fqin_for_process_fails_for_native_service_with_unknown_fqin() -> anyhow::Result<()> {
        let identity_manager = create_identity_manager_for_test()?;
        let update_manager_uid = sdv_aid::AID_SYSTEM.try_into().unwrap();
        let not_update_manager_agent_fqin = ServiceFqinV2Aidl {
            sdvVmName: "vm".to_string(),
            sdvPackageName: "com.android.sdv.update_manager".to_string(),
            serviceBundleName: "NOTUpdateManagerAgent".to_string(),
            serviceInstanceName: "default".to_string(),
        };
        let process_identifier = ProcessIdentifierAidl { pid: 1, uid: update_manager_uid };
        assert!(identity_manager
            .verify_fqin_for_process(&process_identifier, &not_update_manager_agent_fqin)
            .is_err());
        Ok(())
    }

    #[test]
    fn verify_fqin_for_process_succeeds_for_native_service_and_mock_is_not_called(
    ) -> anyhow::Result<()> {
        let mut mock_verifier_provider = MockIdentityVerifierProvider::new();
        mock_verifier_provider.expect_get().times(0);

        let identity_manager =
            create_identity_manager_with_mock_verifier_provider(mock_verifier_provider)?;
        let update_manager_uid = sdv_aid::AID_SYSTEM;
        let update_manager_agent_fqin = ServiceFqinV2Aidl {
            sdvVmName: "vm".to_string(),
            sdvPackageName: "com.android.sdv.update_manager".to_string(),
            serviceBundleName: "UpdateManagerAgent".to_string(),
            serviceInstanceName: "default".to_string(),
        };
        let process_identifier =
            ProcessIdentifierAidl { pid: 1, uid: update_manager_uid.try_into().unwrap() };

        identity_manager
            .verify_fqin_for_process(&process_identifier, &update_manager_agent_fqin)?;
        Ok(())
    }

    #[test]
    fn verify_fqin_for_process_succeeds_for_root_uid() -> anyhow::Result<()> {
        let identity_manager =
            create_identity_manager_with_verifier_expectations(|mock_verifier| {
                mock_verifier.expect_verifyFqinForProcess().times(0);
            })?;
        let process_identifier =
            ProcessIdentifierAidl { pid: 1, uid: sdv_aid::AID_ROOT.try_into().unwrap() };

        identity_manager.verify_fqin_for_process(&process_identifier, &create_test_fqin())?;
        Ok(())
    }

    #[test]
    fn verify_fqin_for_process_fails_if_verifier_fails() -> anyhow::Result<()> {
        let identity_manager =
            create_identity_manager_with_verifier_expectations(|mock_verifier| {
                mock_verifier.expect_verifyFqinForProcess().returning(|_, _| Ok(false));
            })?;
        let process_identifier = ProcessIdentifierAidl { pid: 1, uid: 123 };

        assert!(identity_manager
            .verify_fqin_for_process(&process_identifier, &create_test_fqin())
            .is_err());

        Ok(())
    }

    #[test]
    fn verify_fqin_for_process_succeeds_if_verifier_succeeds() -> anyhow::Result<()> {
        let identity_manager =
            create_identity_manager_with_verifier_expectations(|mock_verifier| {
                mock_verifier.expect_verifyFqinForProcess().returning(|_, _| Ok(true));
            })?;
        let process_identifier = ProcessIdentifierAidl { pid: 1, uid: 123 };

        identity_manager.verify_fqin_for_process(&process_identifier, &create_test_fqin())?;

        Ok(())
    }
}
