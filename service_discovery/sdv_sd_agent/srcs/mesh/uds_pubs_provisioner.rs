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

use super::vvm_config::VvmConfig;
use crate::sha256::sha256;
use anyhow::bail;
use vvmtruststore::UdsPublicKeys;

use binder::IBinder;
use coset::{CborSerializable, CoseError, CoseKey};
use log::warn;
use std::collections::hash_map::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use google_sdv_mesh_provisioning::aidl::google::sdv::mesh::provisioning::IWriteCallback::{
    ErrorCode::ErrorCode,
    IWriteCallback, //
};

/// Compiles the UdsPubs corresponding to the current mesh state.
///
/// It does so by collecting the UDS_Public keys of the DICE chains of all SDV VMs.
/// Its job is only done after it has processed the DICE chains of all SDV VMs that are declared
/// to be part of the SDV Mesh, as per VvmConfig.
pub(crate) struct UdsPubsProvisioner {
    /// Maps a VM name to the UDS_Public keys of the hardware it is running on.
    ///
    /// A piece of hardware will usually have a single UDS_Public key. But in some deployments it
    /// may have two: one for the android boot path and another for the secure world boot path.
    keys_map: HashMap<String, Vec<CoseKey>>,

    remote_vms: Vec<String>,

    tokio_runtime: Arc<tokio::runtime::Runtime>,

    write_callback: Option<binder::Strong<dyn IWriteCallback>>,
}

impl UdsPubsProvisioner {
    pub fn try_new(
        vvmconfig: &VvmConfig,
        local_vm_uds_pubs: Vec<CoseKey>,
        tokio_runtime: Arc<tokio::runtime::Runtime>,
    ) -> anyhow::Result<Self> {
        let remote_vms =
            vvmconfig.remote_cfgs.iter().map(|vm_config| vm_config.name.to_string()).collect();

        let mut provisioner = UdsPubsProvisioner {
            keys_map: HashMap::new(),
            remote_vms,
            tokio_runtime,
            write_callback: None,
        };

        provisioner.add_uds_pubs_as_keys(vvmconfig.own_cfg.name.as_ref(), local_vm_uds_pubs)?;
        Ok(provisioner)
    }

    pub fn forget_vm(&mut self, vm_name: &str) {
        self.keys_map.remove(vm_name);
    }

    /// Writes the uds_pubs file when ready, in a separate task, notifying the callback when done.
    pub fn write_uds_pubs_when_ready(
        &mut self,
        callback: binder::Strong<dyn IWriteCallback>,
    ) -> anyhow::Result<(), ErrorCode> {
        if self.is_ready() {
            self.spawn_write_uds_pubs_task(callback);
        } else {
            self.clean_up_dead_write_callback();

            if self.write_callback.is_some() {
                report_failure(callback, ErrorCode::BUSY);
            } else {
                self.write_callback = Some(callback);
            }
        }
        Ok(())
    }

    fn maybe_write_uds_pubs(&mut self) -> anyhow::Result<()> {
        if !self.is_ready() {
            return Ok(());
        }

        self.clean_up_dead_write_callback();

        if let Some(callback) = self.write_callback.clone() {
            self.write_callback = None;
            self.spawn_write_uds_pubs_task(callback);
        }

        Ok(())
    }

    fn spawn_write_uds_pubs_task(&self, callback: binder::Strong<dyn IWriteCallback>) {
        assert!(self.is_ready());
        let uds_public_keys = self.generate_uds_public_keys();
        self.tokio_runtime.spawn(async move {
            write_uds_pubs_task(callback, uds_public_keys).await;
        });
    }

    fn generate_uds_public_keys(&self) -> UdsPublicKeys {
        let mut all_keys = Vec::new();

        for keys in self.keys_map.values() {
            for key in keys {
                // Note: Cannot have all_keys be a BTreeSet as CoseKey doesn't implement Ord.
                if !all_keys.contains(key) {
                    all_keys.push(key.clone());
                }
            }
        }

        UdsPublicKeys { keys: all_keys }
    }

    fn clean_up_dead_write_callback(&mut self) {
        if let Some(write_callback) = self.write_callback.as_ref() {
            if write_callback.as_binder().ping_binder().is_ok() {
                // The binder interface is still alive.
                return;
            }
        } else {
            // No binder interface to be checked.
            return;
        }

        self.write_callback = None;
    }

    /// Adds the UDS public keys of the VM named `vm_name`.
    ///
    /// The UDS public key is encoded as a COSE_Key.
    pub fn add_uds_pubs(&mut self, vm_name: &str, uds_pubs: Vec<Vec<u8>>) -> anyhow::Result<()> {
        let uds_pubs = uds_pubs
            .into_iter()
            .map(|v| CoseKey::from_slice(&v))
            .collect::<Result<Vec<CoseKey>, CoseError>>()?;

        self.add_uds_pubs_as_keys(vm_name, uds_pubs)
    }

    fn add_uds_pubs_as_keys(
        &mut self,
        vm_name: &str,
        uds_pubs: Vec<CoseKey>,
    ) -> anyhow::Result<()> {
        if uds_pubs.len() != 1 && uds_pubs.len() != 2 {
            bail!("VM {vm_name} presented {} UDS public keys.", uds_pubs.len());
        }

        self.keys_map.insert(vm_name.to_owned(), uds_pubs);

        self.maybe_write_uds_pubs()?;

        Ok(())
    }

    fn is_ready(&self) -> bool {
        for vm_name in &self.remote_vms {
            if !self.keys_map.contains_key(vm_name) {
                return false;
            }
        }
        true
    }

    #[cfg(test)]
    pub(crate) fn keys_map(&self) -> &HashMap<String, Vec<CoseKey>> {
        &self.keys_map
    }
}

fn report_failure(callback: binder::Strong<dyn IWriteCallback>, error_code: ErrorCode) {
    if let Err(e) = callback.onFailure(error_code) {
        log::error!("Failed to call google.sdv.mesh.provisioning.IWriteCallback::onFailed: {e}")
    }
}

fn report_success(callback: binder::Strong<dyn IWriteCallback>, factory_trust: &[u8]) {
    if let Err(e) = callback.onSuccess(factory_trust) {
        log::error!("Failed to call google.sdv.mesh.provisioning.IWriteCallback::onWritten: {e}")
    }
}

async fn write_uds_pubs_task(
    callback: binder::Strong<dyn IWriteCallback>,
    uds_public_keys: UdsPublicKeys,
) {
    match write_async(uds_public_keys).await {
        Ok(factory_trust) => report_success(callback, &factory_trust),
        Err(error_code) => report_failure(callback, error_code),
    };
}

#[cfg(not(test))]
fn get_uds_pubs_path() -> PathBuf {
    PathBuf::from(vvmtruststore::UDS_PUBS_PATH)
}

#[cfg(test)]
fn get_uds_pubs_path() -> PathBuf {
    tests::get_temp_uds_pubs_path()
}

pub async fn write_async(uds_public_keys: UdsPublicKeys) -> anyhow::Result<Vec<u8>, ErrorCode> {
    use futures_util::TryFutureExt;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    let uds_pubs_bytes: Vec<u8> = uds_public_keys.to_vec().map_err(|e| {
        warn!("Failed to serialize UdsPublicKeys: {e}");
        ErrorCode::INTERNAL
    })?;

    let path = get_uds_pubs_path();
    let mut cbor_file: File = File::create(&path)
        .map_err(|e| {
            warn!("Failed to create {} file: {e}", path.display());
            ErrorCode::IO
        })
        .await?;

    cbor_file
        .write_all(&uds_pubs_bytes)
        .map_err(|e| {
            warn!("Failed to write {} file: {e}", path.display());
            ErrorCode::IO
        })
        .await?;

    // Make sure the file was really written before telling the caller that this is done,
    // as a reboot into SDV Boot Mode LOCKED will likely ensue shortly aftwerwards.
    cbor_file
        .flush()
        .map_err(|e| {
            warn!("Failed to flush {} file: {e}", path.display());
            ErrorCode::IO
        })
        .await?;
    cbor_file
        .sync_all()
        .map_err(|e| {
            warn!("Failed to sync {} file: {e}", path.display());
            ErrorCode::IO
        })
        .await?;

    let factory_trust = sha256(&uds_pubs_bytes).map_err(|_| ErrorCode::INTERNAL)?;

    Ok(factory_trust)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::manager::test_support::{
        create_test_vvm_config, VM_INSTANCE_1_NAME, VM_INSTANCE_2_NAME,
    };
    use crate::mesh::uds_pubs::test_support::{
        create_example_key1, create_example_key2, create_example_key3,
    };
    use google_sdv_mesh_provisioning::aidl::google::sdv::mesh::provisioning::IWriteCallback::{
        BnWriteCallback, IWriteCallback,
    };
    use google_sdv_mesh_provisioning::binder::{self, BinderFeatures};
    use lazy_static::lazy_static;
    use std::path::PathBuf;
    use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
    use tempfile::TempDir;

    type ProvisioningResult = anyhow::Result<Vec<u8>, ErrorCode>;

    lazy_static! {
        static ref TEST_TEMP_DIR: TempDir =
            tempfile::tempdir().expect("Failed to create temp dir for tests");
    }

    pub(crate) fn get_temp_uds_pubs_path() -> PathBuf {
        TEST_TEMP_DIR.path().join("uds_pubs")
    }

    struct TestWriteCallback {
        sender: Sender<ProvisioningResult>,
    }

    impl binder::Interface for TestWriteCallback {}

    impl IWriteCallback for TestWriteCallback {
        fn onSuccess(&self, vvm_factory_trust: &[u8]) -> binder::Result<()> {
            self.sender.send(Ok(vvm_factory_trust.to_vec())).unwrap();
            Ok(())
        }

        fn onFailure(&self, error_code: ErrorCode) -> binder::Result<()> {
            self.sender.send(Err(error_code)).unwrap();
            Ok(())
        }
    }

    fn create_test_callback() -> (binder::Strong<dyn IWriteCallback>, Receiver<ProvisioningResult>)
    {
        let (sender, receiver) = channel();
        let callback =
            BnWriteCallback::new_binder(TestWriteCallback { sender }, BinderFeatures::default());
        (callback, receiver)
    }

    #[test]
    fn try_new_ok() -> anyhow::Result<()> {
        let vvm_config = create_test_vvm_config()?;
        let local_vm_uds_pubs = vec![create_example_key1()];
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new()?);

        let provisioner =
            UdsPubsProvisioner::try_new(&vvm_config, local_vm_uds_pubs.clone(), tokio_runtime)?;

        assert_eq!(provisioner.remote_vms, vec![VM_INSTANCE_2_NAME.to_string()]);
        assert_eq!(provisioner.keys_map.len(), 1);
        assert_eq!(provisioner.keys_map.get(VM_INSTANCE_1_NAME), Some(&local_vm_uds_pubs));
        assert!(!provisioner.is_ready());

        Ok(())
    }

    #[test]
    fn add_uds_pubs_and_is_ready() -> anyhow::Result<()> {
        let vvm_config = create_test_vvm_config()?;
        let local_vm_uds_pubs = vec![create_example_key1()];
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new()?);
        let mut provisioner =
            UdsPubsProvisioner::try_new(&vvm_config, local_vm_uds_pubs, tokio_runtime)?;
        assert!(!provisioner.is_ready());

        let remote_uds_pubs = vec![create_example_key2()];
        let remote_uds_pubs_cbor = remote_uds_pubs
            .iter()
            .map(|k| k.clone().to_vec())
            .collect::<Result<Vec<Vec<u8>>, _>>()?;

        provisioner.add_uds_pubs(VM_INSTANCE_2_NAME, remote_uds_pubs_cbor)?;

        assert!(provisioner.is_ready());
        assert_eq!(provisioner.keys_map.get(VM_INSTANCE_2_NAME), Some(&remote_uds_pubs));

        Ok(())
    }

    #[test]
    fn add_uds_pubs_wrong_key_count_err() -> anyhow::Result<()> {
        let vvm_config = create_test_vvm_config()?;
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new()?);
        let mut provisioner =
            UdsPubsProvisioner::try_new(&vvm_config, vec![create_example_key1()], tokio_runtime)?;

        // 0 keys
        let err = provisioner
            .add_uds_pubs(VM_INSTANCE_2_NAME, vec![])
            .expect_err("add_uds_pubs should fail with 0 keys");
        assert_eq!(err.to_string(), "VM instance2 presented 0 UDS public keys.");

        // 3 keys
        let remote_uds_pubs_cbor = vec![
            create_example_key1().to_vec()?,
            create_example_key2().to_vec()?,
            create_example_key3().to_vec()?,
        ];
        let err = provisioner
            .add_uds_pubs(VM_INSTANCE_2_NAME, remote_uds_pubs_cbor)
            .expect_err("add_uds_pubs should fail with 3 keys");
        assert_eq!(err.to_string(), "VM instance2 presented 3 UDS public keys.");

        Ok(())
    }

    #[test]
    fn forget_vm_becomes_not_ready() -> anyhow::Result<()> {
        let vvm_config = create_test_vvm_config()?;
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new()?);
        let mut provisioner =
            UdsPubsProvisioner::try_new(&vvm_config, vec![create_example_key1()], tokio_runtime)?;

        provisioner.add_uds_pubs(VM_INSTANCE_2_NAME, vec![create_example_key2().to_vec()?])?;
        assert!(provisioner.is_ready());
        assert!(provisioner.keys_map.contains_key(VM_INSTANCE_2_NAME));

        provisioner.forget_vm(VM_INSTANCE_2_NAME);

        assert!(!provisioner.is_ready());
        assert!(!provisioner.keys_map.contains_key(VM_INSTANCE_2_NAME));

        Ok(())
    }

    #[test]
    fn generate_uds_public_keys_no_duplicates() -> anyhow::Result<()> {
        let vvm_config = create_test_vvm_config()?;
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new()?);
        // Local VM has one key
        let mut provisioner =
            UdsPubsProvisioner::try_new(&vvm_config, vec![create_example_key1()], tokio_runtime)?;

        // Remote VM has one duplicate key and one new key
        provisioner.add_uds_pubs(
            VM_INSTANCE_2_NAME,
            vec![create_example_key1().to_vec()?, create_example_key2().to_vec()?],
        )?;

        let uds_public_keys = provisioner.generate_uds_public_keys();

        // The final list should have only 2 unique keys
        assert_eq!(uds_public_keys.keys.len(), 2);
        assert!(uds_public_keys.keys.contains(&create_example_key1()));
        assert!(uds_public_keys.keys.contains(&create_example_key2()));

        Ok(())
    }

    #[tokio::test]
    async fn test_write_async() -> anyhow::Result<()> {
        let keys = vec![create_example_key1(), create_example_key2()];
        let uds_public_keys = UdsPublicKeys { keys };

        let factory_trust = write_async(uds_public_keys.clone()).await.unwrap();

        let path = get_temp_uds_pubs_path();
        let written_bytes = tokio::fs::read(&path).await?;

        let written_uds_public_keys = UdsPublicKeys::from_slice(&written_bytes)?;
        assert_eq!(written_uds_public_keys, uds_public_keys);

        let expected_factory_trust = sha256(&written_bytes)?;
        assert_eq!(factory_trust, expected_factory_trust);

        Ok(())
    }

    #[test]
    fn write_uds_pubs_when_ready_not_ready_stores_callback() -> anyhow::Result<()> {
        let vvm_config = create_test_vvm_config()?;
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new()?);
        let mut provisioner =
            UdsPubsProvisioner::try_new(&vvm_config, vec![create_example_key1()], tokio_runtime)?;

        let (callback, receiver) = create_test_callback();
        provisioner.write_uds_pubs_when_ready(callback.clone()).unwrap();

        assert!(provisioner.write_callback.is_some());
        // Check that the stored callback is the one we provided.
        assert_eq!(provisioner.write_callback.as_ref().unwrap().as_binder(), callback.as_binder());

        // Callback should not have been called yet.
        assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));

        Ok(())
    }

    #[test]
    fn write_uds_pubs_when_ready_busy_fails() -> anyhow::Result<()> {
        let vvm_config = create_test_vvm_config()?;
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new()?);
        let mut provisioner =
            UdsPubsProvisioner::try_new(&vvm_config, vec![create_example_key1()], tokio_runtime)?;

        let (callback1, _receiver1) = create_test_callback();
        provisioner.write_uds_pubs_when_ready(callback1).unwrap();

        let (callback2, receiver2) = create_test_callback();
        provisioner.write_uds_pubs_when_ready(callback2).unwrap();

        // The second call should fail with BUSY.
        let result = receiver2.recv()?;
        assert_eq!(result, Err(ErrorCode::BUSY));

        Ok(())
    }

    #[test]
    fn write_uds_pubs_when_ready_already_ready_succeeds() -> anyhow::Result<()> {
        let vvm_config = create_test_vvm_config()?;
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new()?);
        let mut provisioner = UdsPubsProvisioner::try_new(
            &vvm_config,
            vec![create_example_key1()],
            tokio_runtime.clone(),
        )?;
        provisioner.add_uds_pubs(VM_INSTANCE_2_NAME, vec![create_example_key2().to_vec()?])?;
        assert!(provisioner.is_ready());

        let (callback, receiver) = create_test_callback();
        provisioner.write_uds_pubs_when_ready(callback).unwrap();

        // Wait for the async write task to complete.
        let result = receiver.recv()?;
        let factory_trust = result.unwrap();

        // Verify the created file.
        let written_bytes = tokio_runtime
            .block_on(async { tokio::fs::read(get_temp_uds_pubs_path()).await.unwrap() });
        let expected_factory_trust = sha256(&written_bytes)?;
        assert_eq!(factory_trust, expected_factory_trust);

        let written_keys = UdsPublicKeys::from_slice(&written_bytes)?;
        assert_eq!(written_keys.keys.len(), 2);
        assert!(written_keys.keys.contains(&create_example_key1()));
        assert!(written_keys.keys.contains(&create_example_key2()));

        Ok(())
    }

    #[test]
    fn write_uds_pubs_when_ready_becomes_ready_succeeds() -> anyhow::Result<()> {
        let vvm_config = create_test_vvm_config()?;
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new()?);
        let mut provisioner = UdsPubsProvisioner::try_new(
            &vvm_config,
            vec![create_example_key1()],
            tokio_runtime.clone(),
        )?;

        let (callback, receiver) = create_test_callback();
        provisioner.write_uds_pubs_when_ready(callback).unwrap();

        // Not ready yet, callback shouldn't have been called.
        assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));
        assert!(provisioner.write_callback.is_some());

        // Make it ready. This should trigger the write task.
        provisioner.add_uds_pubs(VM_INSTANCE_2_NAME, vec![create_example_key2().to_vec()?])?;

        // Wait for the async write task to complete.
        let result = receiver.recv()?;
        let factory_trust = result.unwrap();

        // Callback should be consumed.
        assert!(provisioner.write_callback.is_none());

        // Verify the created file.
        let written_bytes = tokio_runtime
            .block_on(async { tokio::fs::read(get_temp_uds_pubs_path()).await.unwrap() });
        let expected_factory_trust = sha256(&written_bytes)?;
        assert_eq!(factory_trust, expected_factory_trust);

        let written_keys = UdsPublicKeys::from_slice(&written_bytes)?;
        assert_eq!(written_keys.keys.len(), 2);
        assert!(written_keys.keys.contains(&create_example_key1()));
        assert!(written_keys.keys.contains(&create_example_key2()));

        Ok(())
    }
}
