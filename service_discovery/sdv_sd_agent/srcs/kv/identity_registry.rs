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

use crate::kv::death_watcher::DeathWatcher;
use crate::kv::ServiceRegistryError;

use google_sdv_identity::aidl::google::sdv::identity::{
    ProcessIdentifier::ProcessIdentifier, ServiceFqin::ServiceFqin,
    ServiceIdentity::ServiceIdentity,
};

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use log::{error, warn};

/// The vehicle-wide unique identifier for a Service Identity.
/// TODO: Move this declaration to identity::id_gen module.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Sid(u64);

/// Converts from a raw i64 value.
/// Raw i64 values are produced from AIDL parcelables containing SID because AIDL does not support
/// unsigned integeres.
impl From<i64> for Sid {
    #[inline]
    fn from(val: i64) -> Sid {
        Sid(val as u64)
    }
}

/// Converts to raw i64 value.
/// Raw i64 values are used in AIDL parcelables containing SID because AIDL does not support
/// unsigned integeres.
impl From<Sid> for i64 {
    #[inline]
    fn from(val: Sid) -> i64 {
        val.0 as i64
    }
}

#[cfg(test)]
impl Sid {
    #[cfg(test)]
    pub(crate) fn new_for_testing(val: u64) -> Self {
        Self(val)
    }
}

/// Maintains a list of registered ServiceIdentity values and indexed by whether they are local or
/// remote.
#[derive(Default)]
pub(crate) struct IdentityRegistry {
    // A map containing all known ServiceIdentity values.
    // TODO: Replace ServiceIdentity AIDL with sdv_identity::ServiceIdentity.
    identities_map:
        HashMap<Sid, (ServiceIdentity, Option<ProcessIdentifier>, Option<Box<dyn DeathWatcher>>)>,

    // A Map containing ServiceIdentity values indexed by local ProcessIds.
    // This map redirects to [identities_map].
    local_identities: HashMap<ProcessIdentifier, Sid>,

    // A Map containing ServiceIdentity values indexed by Service FQIN.
    // This map redirects to [identities_map].
    by_fqin: HashMap<ServiceFqin, Sid>,
    // A Map containing ServiceIdentity values indexed by remote VM Name.
    // This map redirects to [identities_map].
    // TODO: Consider using a Set implementation for enabling faster removals.
    by_vm_name: HashMap<String, Vec<Sid>>,
}

impl IdentityRegistry {
    /// Internal method for registering identity.
    /// pub(crate) to use in tests.
    pub(crate) fn register_identity_internal(
        &mut self,
        identity: ServiceIdentity,
        opt_pid: Option<ProcessIdentifier>,
        death_watcher: Option<Box<dyn DeathWatcher>>,
    ) -> Result<(), ServiceRegistryError> {
        let sid = Sid::from(identity.sid);
        if let Some((ref existing_identity, ..)) = self.identities_map.get(&sid) {
            return Err(ServiceRegistryError::SidAlreadyExists {
                sid,
                existing_fqin: existing_identity.name.clone(),
            });
        }

        if let Some(existing_sid) = self.by_fqin.get(&identity.name) {
            let (existing_identity, ..) = self
                .identities_map
                .get(existing_sid)
                .expect("Identity registry corrupted. Unknown identity in `by_fqin` map.");
            return Err(ServiceRegistryError::FqinAlreadyExists {
                existing_sid: *existing_sid,
                fqin: existing_identity.name.clone(),
            });
        }

        if let Some(ref pid) = opt_pid {
            if let Some(existing_sid) = self.local_identities.get(pid).cloned() {
                let existing_identity = self.identities_map.get(&existing_sid).expect(
                    "Identity registry corrupted. Unknown identity in `local_identities` map.",
                );
                warn!("Trying to register {identity:?} for ProcessIdentifier {pid:?}; but {existing_identity:?} for the same ProcessIdentifier already exists");
                return Err(ServiceRegistryError::ProcessIdentityAlreadyExists {
                    existing_fqin: existing_identity.0.name.clone(),
                });
            }
            self.local_identities.insert(pid.clone(), sid);
        };
        self.by_fqin.insert(identity.name.clone(), sid);
        self.by_vm_name.entry(identity.name.sdvVmName.clone()).or_default().push(sid);
        self.identities_map.insert(sid, (identity, opt_pid, death_watcher));
        Ok(())
    }

    /// Registers the given Service Identity for a local (on the same VM) Service Bundle
    /// instance.
    /// TODO: Verify VM Name is the same as local_vm_name.
    pub(crate) fn register_local_identity(
        &mut self,
        identity: ServiceIdentity,
        death_watcher: Option<Box<dyn DeathWatcher>>,
        pid: &ProcessIdentifier,
    ) -> Result<(), ServiceRegistryError> {
        let fqin = identity.name.clone();
        self.register_identity_internal(identity, Some(pid.clone()), death_watcher).inspect_err(
            move |err| {
                error!("Failed to register local identity {fqin:?}: {err}");
            },
        )
    }

    /// Registers the given Service Identity for a remote (not on the same VM) Service Bundle
    /// instance.
    /// TODO: Verify VM Name is NOT the same as local_vm_name.
    pub(crate) fn register_remote_identity(
        &mut self,
        identity: ServiceIdentity,
    ) -> Result<(), ServiceRegistryError> {
        match self.register_identity_internal(identity, None, None) {
            // Already exists errors are expected for remote unit registration as we do it for every remote unit.
            // If multiple remote units are belongs to the same identity it will only be registered once.
            Err(ServiceRegistryError::FqinAlreadyExists { .. }) => Ok(()),
            Err(ServiceRegistryError::SidAlreadyExists { .. }) => Ok(()),
            res => res,
        }
    }

    fn get_identity_internal(&self, sid: &Sid) -> Option<&ServiceIdentity> {
        self.identities_map.get(sid).map(|(ref identity, _, _)| identity)
    }

    /// Fetches a reference to the ServiceIdentity registered with the provided ProcessIdentity.
    pub(crate) fn get_identity_for_process(
        &self,
        pid: &ProcessIdentifier,
    ) -> Option<&ServiceIdentity> {
        self.local_identities.get(pid).and_then(|sid| self.get_identity_internal(sid))
    }

    /// Fetches a reference to the ServiceIdentity registered with the provided SID.
    pub(crate) fn get_identity(&self, sid: u64) -> Option<&ServiceIdentity> {
        self.get_identity_internal(&Sid(sid))
    }

    /// Fetches a reference to the ServiceIdentity registered with the provided FQIN.
    pub(crate) fn get_identity_by_fqin(&self, fqin: &ServiceFqin) -> Option<&ServiceIdentity> {
        self.by_fqin.get(fqin).and_then(|sid| self.get_identity_internal(sid))
    }

    /// Removes the ServiceIdentity registered with provided `sid` value.
    /// This function returns success if the ServiceIdentity was removed, or an
    /// `IdentityNotFound` if the ServiceIdentity is not registered.
    fn remove_identity_internal(&mut self, sid: &Sid) -> Result<(), ServiceRegistryError> {
        let (identity, opt_pid) = match self.identities_map.get(sid) {
            None => return Err(ServiceRegistryError::IdentityNotFound),
            Some(value) => (&value.0, &value.1),
        };

        if let Some(pid) = opt_pid {
            self.local_identities.remove(pid);
        }
        self.by_fqin.remove(&identity.name);
        if let Entry::Occupied(mut entry) = self.by_vm_name.entry(identity.name.sdvVmName.clone()) {
            entry.get_mut().retain(|entry_sid| entry_sid != sid)
        }
        self.identities_map.remove(sid);

        Ok(())
    }

    /// Removes the provided ServiceIdentity.
    /// This function returns success if the ServiceIdentity was removed, or an
    /// `IdentityNotFound` if the ServiceIdentity is not registered.
    pub(crate) fn remove_identity(
        &mut self,
        identity: &ServiceIdentity,
    ) -> Result<(), ServiceRegistryError> {
        self.remove_identity_internal(&Sid::from(identity.sid))
    }

    /// Removes all identities registered with the given `vm_name`.
    /// This function returns success if all the ServiceIdentity values were removed, or an
    /// `IdentityNotFound` if the ServiceIdentity is not registered.
    pub(crate) fn remove_all_by_vm_name(
        &mut self,
        vm_name: &str,
    ) -> Result<(), ServiceRegistryError> {
        match self.by_vm_name.remove(vm_name) {
            None => Err(ServiceRegistryError::IdentityNotFound),
            Some(ref sids) => {
                for sid in sids.iter() {
                    let removed = self.remove_identity_internal(sid);
                    if removed.is_err() {
                        panic!("IdentityRegistry may be corrupted. Unknown sid {sid:?} in `remote_identities` map.")
                    }
                }
                Ok(())
            }
        }
    }

    /// Returns list of VMs that have registered identities
    pub(crate) fn get_known_vm_names(&self) -> Vec<String> {
        self.by_vm_name.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use google_sdv_identity::aidl::google::sdv::identity::ServiceIdentity::EcPublicKey::EcPublicKey;

    fn create_fqin(vm_name: &str, instance_name: &str) -> ServiceFqin {
        ServiceFqin {
            sdvVmName: vm_name.to_string(),
            sdvPackageName: "com.android.sdv.service_discovery.test".to_string(),
            serviceBundleName: "IdentityRegistryTests".to_string(),
            serviceInstanceName: instance_name.to_string(),
        }
    }

    fn create_identity(sid: i64, name: ServiceFqin) -> ServiceIdentity {
        ServiceIdentity {
            sid,
            publicKey: EcPublicKey { keyValue: *b"01234567890123456789012345678912" },
            name,
        }
    }

    fn create_identity_value_1() -> ServiceIdentity {
        create_identity(10001, create_fqin("vm-current", "instance-1"))
    }

    fn create_identity_value_2() -> ServiceIdentity {
        create_identity(10002, create_fqin("vm-current", "instance-2"))
    }

    fn create_identity_value_3() -> ServiceIdentity {
        create_identity(10003, create_fqin("vm-current", "instance-3"))
    }

    fn create_fqin_value_1() -> ServiceFqin {
        create_fqin("vm-current", "instance-1")
    }

    fn create_fqin_value_2() -> ServiceFqin {
        create_fqin("vm-current", "instance-2")
    }

    fn create_identity_registry() -> IdentityRegistry {
        Default::default()
    }

    #[test]
    fn test_register_local_identity_success() {
        let mut registry = create_identity_registry();
        let identity = create_identity_value_1();
        let pid = ProcessIdentifier { pid: 1234, uid: 1234 };

        let result = registry.register_local_identity(identity, None, &pid);

        assert!(result.is_ok());
    }

    #[test]
    fn test_register_remote_identity_success() {
        let mut registry = create_identity_registry();
        let identity = create_identity_value_1();

        let result = registry.register_remote_identity(identity);

        assert!(result.is_ok());
    }

    #[test]
    fn test_register_local_identity_twice_fails() {
        let mut registry = create_identity_registry();
        let pid = ProcessIdentifier { pid: 1234, uid: 1234 };

        let identity1 = create_identity_value_1();
        let result = registry.register_local_identity(identity1.clone(), None, &pid);
        assert!(result.is_ok());

        let identity2 = create_identity_value_2();
        let error = registry
            .register_local_identity(identity2, None, &pid)
            .expect_err("same PID adding a second identity should fail");
        assert_eq!(
            error,
            ServiceRegistryError::ProcessIdentityAlreadyExists {
                existing_fqin: create_fqin_value_1()
            }
        );

        let identity = registry.get_identity_for_process(&pid);
        assert_eq!(*identity.unwrap(), identity1);
    }

    #[test]
    fn test_register_local_identity_same_fqin_fails() {
        let mut registry = create_identity_registry();
        let fqin = create_fqin_value_1();

        let mut identity1 = create_identity_value_1();
        identity1.name = fqin.clone();
        let pid1 = ProcessIdentifier { pid: 1234, uid: 1234 };
        let result = registry.register_local_identity(identity1, None, &pid1);
        assert!(result.is_ok());

        let mut identity2 = create_identity_value_2();
        identity2.name = fqin.clone();
        let pid2 = ProcessIdentifier { pid: 5678, uid: 5678 };
        let result = registry.register_local_identity(identity2, None, &pid2);
        assert!(result.is_err());
    }

    #[test]
    fn test_register_local_identity_same_sid_fails() {
        let mut registry = create_identity_registry();
        let sid = 123456_i64;

        let mut identity1 = create_identity_value_1();
        identity1.sid = sid;
        let pid1 = ProcessIdentifier { pid: 1234, uid: 1234 };
        let result = registry.register_local_identity(identity1, None, &pid1);
        assert!(result.is_ok());

        let mut identity2 = create_identity_value_2();
        identity2.sid = sid;
        let pid2 = ProcessIdentifier { pid: 5678, uid: 5678 };
        let result = registry.register_local_identity(identity2, None, &pid2);
        assert!(result.is_err());
    }

    #[test]
    fn test_register_remote_identity_same_fqin_success() {
        let mut registry = create_identity_registry();
        let fqin = create_fqin_value_1();

        let mut identity1 = create_identity_value_1();
        identity1.name = fqin.clone();
        let result = registry.register_remote_identity(identity1);
        assert!(result.is_ok());

        let mut identity2 = create_identity_value_2();
        identity2.name = fqin.clone();
        let result = registry.register_remote_identity(identity2);
        assert!(result.is_ok());
    }

    #[test]
    fn test_register_remote_identity_same_sid_success() {
        let mut registry = create_identity_registry();
        let sid = 123456_i64;

        let mut identity1 = create_identity_value_1();
        identity1.sid = sid;
        let result = registry.register_remote_identity(identity1);
        assert!(result.is_ok());

        let mut identity2 = create_identity_value_2();
        identity2.sid = sid;
        let result = registry.register_remote_identity(identity2);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_identity_success() {
        let mut registry = create_identity_registry();
        let sid1 = 4567_i64;
        let sid2 = 6789_i64;

        let mut identity1 = create_identity_value_1();
        identity1.sid = sid1;
        registry.register_remote_identity(identity1.clone()).unwrap();
        let mut identity2 = create_identity_value_2();
        identity2.sid = sid2;
        registry.register_remote_identity(identity2).unwrap();

        let result = registry.get_identity(sid1 as u64);

        assert_eq!(*result.unwrap(), identity1);
    }

    #[test]
    fn test_get_identity_failed() {
        let mut registry = create_identity_registry();
        let sid1 = 4567_i64;
        let sid2 = 6789_i64;

        let mut identity1 = create_identity_value_1();
        identity1.sid = sid1;
        registry.register_remote_identity(identity1).unwrap();

        let result = registry.get_identity(sid2 as u64);

        assert!(result.is_none());
    }

    #[test]
    fn test_get_identity_by_fqin_success() {
        let mut registry = create_identity_registry();
        let mut fqin1 = create_fqin_value_1();
        fqin1.serviceInstanceName = "instance1".to_string();
        let mut fqin2 = create_fqin_value_2();
        fqin2.serviceInstanceName = "instance2".to_string();

        let mut identity1 = create_identity_value_1();
        identity1.name = fqin1.clone();
        registry.register_remote_identity(identity1.clone()).unwrap();
        let mut identity2 = create_identity_value_2();
        identity2.name = fqin2.clone();
        registry.register_remote_identity(identity2).unwrap();

        let result = registry.get_identity_by_fqin(&fqin1);

        assert_eq!(*result.unwrap(), identity1);
    }

    #[test]
    fn test_get_identity_by_fqin_failed() {
        let mut registry = create_identity_registry();
        let mut fqin1 = create_fqin_value_1();
        fqin1.serviceInstanceName = "instance1".to_string();
        let mut fqin2 = create_fqin_value_2();
        fqin2.serviceInstanceName = "instance2".to_string();

        let mut identity1 = create_identity_value_1();
        identity1.name = fqin1.clone();
        registry.register_remote_identity(identity1).unwrap();

        let result = registry.get_identity_by_fqin(&fqin2);

        assert!(result.is_none());
    }

    #[test]
    fn test_remove_identity_success() {
        let mut registry = create_identity_registry();

        let identity1 = create_identity_value_1();
        registry.register_remote_identity(identity1.clone()).unwrap();
        let identity2 = create_identity_value_2();
        registry.register_remote_identity(identity2.clone()).unwrap();

        let result = registry.remove_identity(&identity2);
        assert!(result.is_ok());

        let result = registry.get_identity(identity1.sid as u64);
        assert_eq!(*result.unwrap(), identity1);
        let result = registry.get_identity(identity2.sid as u64);
        assert!(result.is_none());
    }

    #[test]
    fn test_remove_identity_failed() {
        let mut registry = create_identity_registry();

        let identity1 = create_identity_value_1();
        registry.register_remote_identity(identity1.clone()).unwrap();
        let identity2 = create_identity_value_2();
        // identity2 is not registered.

        let result = registry.remove_identity(&identity2);
        assert!(result.is_err());

        let result = registry.get_identity(identity1.sid as u64);
        assert_eq!(*result.unwrap(), identity1);
    }

    #[test]
    fn test_remove_all_by_vm_name_success() {
        let mut registry = create_identity_registry();
        let vm_name1 = "vm-one".to_string();
        let vm_name2 = "vm-two".to_string();

        let mut identity1 = create_identity_value_1();
        identity1.name.sdvVmName.clone_from(&vm_name1);
        registry.register_remote_identity(identity1.clone()).unwrap();
        let mut identity21 = create_identity_value_2();
        identity21.name.sdvVmName.clone_from(&vm_name2);
        registry.register_remote_identity(identity21.clone()).unwrap();
        let mut identity22 = create_identity_value_3();
        identity22.name.sdvVmName.clone_from(&vm_name2);
        registry.register_remote_identity(identity22.clone()).unwrap();

        let result = registry.remove_all_by_vm_name(&vm_name2);
        assert!(result.is_ok());

        let result = registry.get_identity(identity1.sid as u64);
        assert_eq!(*result.unwrap(), identity1);
        let result = registry.get_identity(identity21.sid as u64);
        assert!(result.is_none());
        let result = registry.get_identity(identity22.sid as u64);
        assert!(result.is_none());
    }

    #[test]
    fn test_remove_all_by_vm_name_failed() {
        let mut registry = create_identity_registry();
        let vm_name1 = "vm-one".to_string();
        let vm_name2 = "vm-two".to_string();

        // No Identities with vm_name1.
        let mut identity21 = create_identity_value_1();
        identity21.name.sdvVmName.clone_from(&vm_name2);
        registry.register_remote_identity(identity21.clone()).unwrap();
        let mut identity22 = create_identity_value_2();
        identity22.name.sdvVmName.clone_from(&vm_name2);
        registry.register_remote_identity(identity22.clone()).unwrap();
        let mut identity23 = create_identity_value_3();
        identity23.name.sdvVmName.clone_from(&vm_name2);
        registry.register_remote_identity(identity23.clone()).unwrap();

        let result = registry.remove_all_by_vm_name(&vm_name1);
        assert!(result.is_err());

        let result = registry.get_identity(identity21.sid as u64);
        assert_eq!(*result.unwrap(), identity21);
        let result = registry.get_identity(identity22.sid as u64);
        assert_eq!(*result.unwrap(), identity22);
        let result = registry.get_identity(identity23.sid as u64);
        assert_eq!(*result.unwrap(), identity23);
    }

    #[test]
    fn test_get_identity_for_process_success() {
        let mut registry = create_identity_registry();
        let pid1 = ProcessIdentifier { pid: 1234, uid: 1234 };
        let pid2 = ProcessIdentifier { pid: 5678, uid: 5678 };

        let identity1 = create_identity_value_1();
        registry.register_identity_internal(identity1.clone(), Some(pid1.clone()), None).unwrap();
        let identity2 = create_identity_value_2();
        registry.register_identity_internal(identity2.clone(), Some(pid2.clone()), None).unwrap();

        let result = registry.get_identity_for_process(&pid1);

        assert_eq!(*result.unwrap(), identity1);
    }

    #[test]
    fn test_get_identity_for_process_failed() {
        let mut registry = create_identity_registry();
        let pid1 = ProcessIdentifier { pid: 1234, uid: 1234 };
        let pid2 = ProcessIdentifier { pid: 5678, uid: 5678 };

        let identity1 = create_identity_value_1();
        registry.register_identity_internal(identity1.clone(), Some(pid1.clone()), None).unwrap();

        let result = registry.get_identity_for_process(&pid2);

        assert!(result.is_none());
    }

    #[test]
    fn get_known_vm_names_returns_empty_list_when_no_identities_registered() {
        let registry = IdentityRegistry::default();
        let vm_names = registry.get_known_vm_names();
        assert!(vm_names.is_empty());
    }

    #[test]
    fn get_known_vm_names_returns_registered_vm_names() {
        let mut registry = IdentityRegistry::default();
        let vm_name1 = "vm-one";
        let vm_name2 = "vm-two";

        let identity1 = create_identity(1, create_fqin(vm_name1, "instance-1"));
        registry.register_remote_identity(identity1).unwrap();

        let identity2 = create_identity(2, create_fqin(vm_name2, "instance-1"));
        registry.register_remote_identity(identity2).unwrap();

        let identity3 = create_identity(3, create_fqin(vm_name1, "instance-2"));
        registry.register_remote_identity(identity3).unwrap();

        let vm_names = registry.get_known_vm_names();
        assert_eq!(vm_names.len(), 2);
        assert!(vm_names.contains(&vm_name1.to_string()));
        assert!(vm_names.contains(&vm_name2.to_string()));
    }

    #[test]
    fn get_known_vm_names_updates_after_identities_removed() {
        let mut registry = IdentityRegistry::default();
        let vm_name1 = "vm-one";
        let vm_name2 = "vm-two";

        let identity1 = create_identity(1, create_fqin(vm_name1, "instance-1"));
        registry.register_remote_identity(identity1).unwrap();

        let identity2 = create_identity(2, create_fqin(vm_name2, "instance-1"));
        registry.register_remote_identity(identity2).unwrap();

        let identity3 = create_identity(3, create_fqin(vm_name1, "instance-2"));
        registry.register_remote_identity(identity3).unwrap();

        registry.remove_all_by_vm_name(vm_name1).unwrap();

        let vm_names = registry.get_known_vm_names();
        assert_eq!(vm_names.len(), 1);
        assert!(vm_names.contains(&vm_name2.to_string()));
        assert!(!vm_names.contains(&vm_name1.to_string()));
    }
}
