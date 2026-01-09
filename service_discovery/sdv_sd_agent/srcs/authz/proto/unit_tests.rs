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

#[cfg(test)]
mod tests {
    use super::super::{from_peer_local_acls_proto, to_peer_local_acls_proto};

    use std::collections::HashMap;
    use std::str::FromStr;

    use sdv_authz::SingleVmAclPolicyProvider;
    use sdv_authz_policy_proto_rust::acl_policy::AclPolicy as AclPolicyProto;
    use sdv_comms_id::names::{PackageName, ServiceName as ServiceBundleName};

    use anyhow::anyhow;
    use protobuf::text_format;

    struct TestSingleVmAclPolicyProvider {
        mapping: HashMap<(PackageName, Option<ServiceBundleName>), AclPolicyProto>,
    }

    impl TestSingleVmAclPolicyProvider {
        fn new() -> Self {
            Self { mapping: HashMap::new() }
        }

        fn insert(
            &mut self,
            package_name: PackageName,
            service_bundle_name: Option<ServiceBundleName>,
            acl_policy: AclPolicyProto,
        ) {
            self.mapping.insert((package_name, service_bundle_name), acl_policy);
        }
    }

    impl SingleVmAclPolicyProvider for TestSingleVmAclPolicyProvider {
        fn get_acl_policy_proto(
            &self,
            package_name: &PackageName,
            service_bundle_name: Option<&ServiceBundleName>,
        ) -> Result<AclPolicyProto, anyhow::Error> {
            self.mapping
                .get(&(package_name.clone(), service_bundle_name.cloned()))
                .cloned()
                .ok_or_else(|| anyhow!("No ACL policy found for the given service bundle"))
        }

        fn get_namespace_with_acl_policy(
            &self,
        ) -> Result<Vec<(PackageName, Option<ServiceBundleName>)>, anyhow::Error> {
            Ok(self.mapping.keys().cloned().collect())
        }
    }

    #[test]
    fn test_to_proto_and_from_proto_success() {
        let expected_acl_policy = text_format::parse_from_str::<AclPolicyProto>(
            r#"
                acls {
                    name: "TirePressureSubs"
                    include: "vm-cdc:*"
                    include: "vm-telemetry:*"
                    never_include: "vm-ivi:*"
                }
                service_units {
                    name: "tire-pressure-publisher"
                    acl_names: "TirePressureSubs"
                }
            "#,
        )
        .unwrap();

        let package_name = PackageName::from_str("com.google.test").unwrap();
        let service_bundle_name = ServiceBundleName::from_str("DataProvider").unwrap();

        let mut local_provider = TestSingleVmAclPolicyProvider::new();
        local_provider.insert(
            package_name.clone(),
            Some(service_bundle_name.clone()),
            expected_acl_policy.clone(),
        );

        let proto = to_peer_local_acls_proto(&local_provider).unwrap();
        let remote_provider = from_peer_local_acls_proto(proto).unwrap();

        let actual_acl_policy = remote_provider
            .get_acl_policy_proto(&package_name, Some(&service_bundle_name))
            .unwrap();

        assert_eq!(expected_acl_policy, actual_acl_policy);
    }
}
