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

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::TransportMetadata::TransportMetadata;
use google_sdv_service_discovery_discovery::aidl::google::sdv::service_discovery::discovery::ServiceUnitDefinition::ServiceUnitDefinition;

use crate::kv::identity_registry::Sid;
use crate::kv::ServiceUnitIdentifier;

use std::collections::{hash_map::Entry, HashMap};

#[derive(Clone, Debug)]
pub(crate) struct ServiceUnitAndTransport {
    service_unit_definition: ServiceUnitDefinition,
    transport_metadata: TransportMetadata,
}

impl ServiceUnitAndTransport {
    pub(crate) fn new(
        service_unit_definition: ServiceUnitDefinition,
        transport_metadata: TransportMetadata,
    ) -> ServiceUnitAndTransport {
        ServiceUnitAndTransport { transport_metadata, service_unit_definition }
    }

    pub(crate) fn get_service_unit_definition(&self) -> &ServiceUnitDefinition {
        &self.service_unit_definition
    }

    pub(crate) fn get_transport_metadata(&self) -> &TransportMetadata {
        &self.transport_metadata
    }
}

pub type ServiceRegistryItem<'a> = (Sid, &'a str, &'a ServiceUnitAndTransport);

#[derive(Default, Debug)]
pub struct ServiceUnitRegistry {
    inner: HashMap<Sid, HashMap<String, ServiceUnitAndTransport>>,
}

impl ServiceUnitRegistry {
    pub(crate) fn insert(
        &mut self,
        sid: Sid,
        service_unit_name: String,
        service_definition: ServiceUnitAndTransport,
    ) {
        self.inner.entry(sid).or_default().insert(service_unit_name, service_definition);
    }

    pub(crate) fn remove(
        &mut self,
        service_unit_id: &ServiceUnitIdentifier,
    ) -> Option<ServiceUnitDefinition> {
        if let Entry::Occupied(mut sid_entry) = self.inner.entry(service_unit_id.sid) {
            let removed = sid_entry.get_mut().remove(&service_unit_id.name);
            if sid_entry.get().is_empty() {
                sid_entry.remove_entry();
            }
            return removed.map(|s| s.service_unit_definition);
        }

        None
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = ServiceRegistryItem<'_>> {
        self.inner.iter().flat_map(|(sid, inner_map)| {
            inner_map.iter().map(move |(name, definition)| (*sid, name.as_str(), definition))
        })
    }

    pub(crate) fn remove_if<F>(&mut self, mut f: F) -> Vec<ServiceUnitDefinition>
    where
        F: FnMut(Sid, &str, &ServiceUnitAndTransport) -> bool,
    {
        let mut removed = Vec::new();
        for (sid, inner_map) in &mut self.inner {
            // TODO: extract_if fits here, but it is an experimental API
            let mut to_remove = Vec::new();
            for (name, definition) in inner_map.iter() {
                if f(*sid, name.as_str(), definition) {
                    to_remove.push(name.clone());
                    removed.push(definition.service_unit_definition.clone());
                }
            }
            for name in to_remove {
                inner_map.remove(&name);
            }
        }
        self.inner.retain(|_, inner_map| !inner_map.is_empty());
        removed
    }

    pub(crate) fn find_by_sid_and_name<'a>(
        &'a self,
        sid: Sid,
        name: &str,
    ) -> Option<&'a ServiceUnitAndTransport> {
        self.inner.get(&sid)?.get(name)
    }

    #[allow(unused)]
    pub(crate) fn find_by_sid(
        &self,
        sid: Sid,
    ) -> Option<impl Iterator<Item = &ServiceUnitAndTransport>> {
        Some(self.inner.get(&sid)?.values())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::ServerUnitMetadata::ServerUnitMetadata;
    use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::metadata::ApplicationMetadata::TypedMetadata::TypedMetadata::ServerUnit;
    use googletest::prelude::*;
    use googletest::matcher::{Matcher, MatcherBase, MatcherResult};
    use googletest::description::Description;

    fn create_test_service_unit_definition(sid: Sid, unit_name: &str) -> ServiceUnitDefinition {
        let mut service_sud = ServiceUnitDefinition::default();
        service_sud.identity.sid = sid.into();
        service_sud.serviceUnitName = unit_name.to_string();
        service_sud.appMetadata.typedMetadata = ServerUnit(ServerUnitMetadata::default());
        service_sud
    }

    fn create_test_service_unit_and_transport(
        sid: Sid,
        unit_name: &str,
    ) -> ServiceUnitAndTransport {
        ServiceUnitAndTransport::new(
            create_test_service_unit_definition(sid, unit_name),
            TransportMetadata::default(),
        )
    }

    #[derive(MatcherBase)]
    struct ServiceRegistryItemEqMatcher {
        expected_definition: ServiceUnitDefinition,
    }

    impl<'a> Matcher<&&'a ServiceUnitAndTransport> for ServiceRegistryItemEqMatcher {
        // Compares values, ignoring the TransportMetadata part as it is not relevant
        fn matches(&self, actual: &&'a ServiceUnitAndTransport) -> MatcherResult {
            if actual.get_service_unit_definition() != &self.expected_definition {
                return MatcherResult::NoMatch;
            }
            MatcherResult::Match
        }

        fn describe(&self, matcher_result: MatcherResult) -> Description {
            match matcher_result {
                MatcherResult::Match => format!(
                    "is equal to {:?} as it defined by ServiceRegistryItemEqMatcher",
                    self.expected_definition
                )
                .into(),
                MatcherResult::NoMatch => format!(
                    "isn't equal to {:?} as it defined by ServiceRegistryItemEqMatcher",
                    self.expected_definition
                )
                .into(),
            }
        }
    }

    fn value_eq(expected_definition: ServiceUnitDefinition) -> ServiceRegistryItemEqMatcher {
        ServiceRegistryItemEqMatcher { expected_definition }
    }

    #[test]
    fn find_by_sid_and_name_after_insert_to_different_sid_and_units_ok() {
        let mut registry = ServiceUnitRegistry::default();

        let service_unit_1 = create_test_service_unit_and_transport(1.into(), "test_unit_1");
        let service_unit_2 = create_test_service_unit_and_transport(1.into(), "test_unit_2");
        let service_unit_3 = create_test_service_unit_and_transport(2.into(), "test_unit_3");
        registry.insert(1.into(), "test_unit_1".to_string(), service_unit_1.clone());
        registry.insert(1.into(), "test_unit_2".to_string(), service_unit_2.clone());
        registry.insert(2.into(), "test_unit_3".to_string(), service_unit_3.clone());

        assert_that!(
            registry
                .find_by_sid_and_name(1.into(), "test_unit_1")
                .unwrap()
                .get_service_unit_definition(),
            eq(service_unit_1.get_service_unit_definition())
        );
        assert_that!(
            registry
                .find_by_sid_and_name(1.into(), "test_unit_2")
                .unwrap()
                .get_service_unit_definition(),
            eq(service_unit_2.get_service_unit_definition())
        );
        assert_that!(
            registry
                .find_by_sid_and_name(2.into(), "test_unit_3")
                .unwrap()
                .get_service_unit_definition(),
            eq(service_unit_3.get_service_unit_definition())
        );
    }

    #[test]
    fn remove_removes_only_specified_item() {
        let mut registry = ServiceUnitRegistry::default();
        let service_unit_1 = create_test_service_unit_and_transport(1.into(), "test_unit_1");
        let service_unit_2 = create_test_service_unit_and_transport(1.into(), "test_unit_2");
        let service_unit_3 = create_test_service_unit_and_transport(2.into(), "test_unit_3");
        registry.insert(1.into(), "test_unit_1".to_string(), service_unit_1.clone());
        registry.insert(1.into(), "test_unit_2".to_string(), service_unit_2.clone());
        registry.insert(2.into(), "test_unit_3".to_string(), service_unit_3.clone());

        // Remove returns true when removing existing element
        assert_that!(
            registry
                .remove(&ServiceUnitIdentifier { sid: 1.into(), name: "test_unit_1".to_string() }),
            some(anything())
        );

        assert_that!(registry.find_by_sid_and_name(1.into(), "test_unit_1"), none());

        let iter = registry.iter().collect::<Vec<_>>();
        assert_that!(
            iter,
            unordered_elements_are![
                (
                    eq(&1.into()),
                    eq(&"test_unit_2"),
                    value_eq(create_test_service_unit_definition(1.into(), "test_unit_2")),
                ),
                (
                    eq(&2.into()),
                    eq(&"test_unit_3"),
                    value_eq(create_test_service_unit_definition(2.into(), "test_unit_3")),
                )
            ]
        );
    }

    #[test]
    fn remove_does_not_panic_if_item_not_found() {
        let mut registry = ServiceUnitRegistry::default();

        // Remove return false when removing non-existing element
        assert_that!(
            registry
                .remove(&ServiceUnitIdentifier { sid: 1.into(), name: "test_unit_1".to_string() }),
            none()
        );

        let iter: Vec<(_, _, _)> = registry.iter().collect();
        assert_that!(iter, empty());
    }

    #[test]
    fn find_by_sid_and_name_not_found() {
        let mut registry = ServiceUnitRegistry::default();

        let service_unit_1 = create_test_service_unit_and_transport(1.into(), "test_unit_1");
        registry.insert(1.into(), "test_unit_1".to_string(), service_unit_1.clone());

        assert_that!(registry.find_by_sid_and_name(1.into(), "test_unit_2"), none());
        assert_that!(registry.find_by_sid_and_name(2.into(), "test_unit_1"), none());
    }

    #[test]
    fn iter_empty() {
        let registry = ServiceUnitRegistry::default();
        let iter: Vec<(_, _, _)> = registry.iter().collect();
        assert_that!(iter, empty());
    }

    #[test]
    fn iter_multiple_items() {
        let mut registry = ServiceUnitRegistry::default();
        let service_unit_1 = create_test_service_unit_and_transport(1.into(), "test_unit_1");
        let service_unit_2 = create_test_service_unit_and_transport(2.into(), "test_unit_2");
        let service_unit_3 = create_test_service_unit_and_transport(3.into(), "test_unit_3");
        registry.insert(1.into(), "test_unit_1".to_string(), service_unit_1.clone());
        registry.insert(2.into(), "test_unit_2".to_string(), service_unit_2.clone());
        registry.insert(3.into(), "test_unit_3".to_string(), service_unit_3.clone());

        let iter = registry.iter().collect::<Vec<_>>();
        assert_that!(
            iter,
            unordered_elements_are![
                (
                    eq(&1.into()),
                    eq(&"test_unit_1"),
                    value_eq(create_test_service_unit_definition(1.into(), "test_unit_1")),
                ),
                (
                    eq(&2.into()),
                    eq(&"test_unit_2"),
                    value_eq(create_test_service_unit_definition(2.into(), "test_unit_2")),
                ),
                (
                    eq(&3.into()),
                    eq(&"test_unit_3"),
                    value_eq(create_test_service_unit_definition(3.into(), "test_unit_3")),
                )
            ]
        );
    }

    #[test]
    fn remove_if_removes_all_but_one_items() {
        let mut registry = ServiceUnitRegistry::default();
        let service_unit_1 = create_test_service_unit_and_transport(1.into(), "test_unit_1");
        let service_unit_2 = create_test_service_unit_and_transport(2.into(), "test_unit_2");
        let service_unit_3 = create_test_service_unit_and_transport(3.into(), "test_unit_3");
        registry.insert(1.into(), "test_unit_1".to_string(), service_unit_1.clone());
        registry.insert(2.into(), "test_unit_2".to_string(), service_unit_2.clone());
        registry.insert(3.into(), "test_unit_3".to_string(), service_unit_3.clone());

        let removed =
            registry.remove_if(|_, name, _| name == "test_unit_1" || name == "test_unit_3");

        assert_that!(removed, len(eq(2)));

        let iter = registry.iter().collect::<Vec<_>>();
        assert_that!(
            iter,
            unordered_elements_are![(
                eq(&2.into()),
                eq(&"test_unit_2"),
                value_eq(create_test_service_unit_definition(2.into(), "test_unit_2")),
            )]
        );
    }

    #[test]
    fn find_by_sid_returns_iterator_for_existing_sid() {
        let mut registry = ServiceUnitRegistry::default();
        let service_unit_1 = create_test_service_unit_and_transport(1.into(), "test_unit_1");
        let service_unit_2 = create_test_service_unit_and_transport(1.into(), "test_unit_2");
        let service_unit_3 = create_test_service_unit_and_transport(2.into(), "test_unit_3");
        registry.insert(1.into(), "test_unit_1".to_string(), service_unit_1.clone());
        registry.insert(1.into(), "test_unit_2".to_string(), service_unit_2.clone());
        registry.insert(2.into(), "test_unit_3".to_string(), service_unit_3.clone());

        let iter = registry.find_by_sid(1.into()).unwrap();
        let iter: Vec<&ServiceUnitAndTransport> = iter.collect();
        assert_that!(
            iter,
            unordered_elements_are![
                value_eq(create_test_service_unit_definition(1.into(), "test_unit_1")),
                value_eq(create_test_service_unit_definition(1.into(), "test_unit_2")),
            ]
        );
    }

    #[test]
    fn find_by_sid_returns_none_for_non_existing_sid() {
        let registry = ServiceUnitRegistry::default();
        assert!(registry.find_by_sid(1.into()).is_none());
    }
}
