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

use std::collections::HashMap;
use std::hash::Hash;

use crate::reflection::message::dynamic_message::SingularField;

/// Dynamic protobuf map field
#[derive(Debug, Clone, PartialEq)]
pub enum MapField {
    /// Map with Int32 key type
    Int32(MapFieldContainer<i32>),
    /// Map with Int64 key type
    Int64(MapFieldContainer<i64>),
    /// Map with Uint32 key type
    Uint32(MapFieldContainer<u32>),
    /// Map with Uint64 key type
    Uint64(MapFieldContainer<u64>),
    /// Map with Sint32 key type
    Sint32(MapFieldContainer<i32>),
    /// Map with Sint64 key type
    Sint64(MapFieldContainer<i64>),
    /// Map with Fixed32 key type
    Fixed32(MapFieldContainer<u32>),
    /// Map with Fixed64 key type
    Fixed64(MapFieldContainer<u64>),
    /// Map with Sfixed32 key type
    Sfixed32(MapFieldContainer<i32>),
    /// Map with Sfixed64 key type
    Sfixed64(MapFieldContainer<i64>),
    /// Map with Bool key type
    Bool(MapFieldContainer<bool>),
    /// Map with String key type
    String(MapFieldContainer<String>),
}

impl MapField {
    fn len(&self) -> usize {
        match self {
            MapField::Int32(container) => container.len(),
            MapField::Int64(container) => container.len(),
            MapField::Uint32(container) => container.len(),
            MapField::Uint64(container) => container.len(),
            MapField::Sint32(container) => container.len(),
            MapField::Sint64(container) => container.len(),
            MapField::Fixed32(container) => container.len(),
            MapField::Fixed64(container) => container.len(),
            MapField::Sfixed32(container) => container.len(),
            MapField::Sfixed64(container) => container.len(),
            MapField::Bool(container) => container.len(),
            MapField::String(container) => container.len(),
        }
    }

    // Merging a map field is equivalent to merging the underlying map entries. In particular, this
    // means that in the case of duplicate keys, the entry in `other_map_field` will overwrite the
    // entry in self.
    pub(crate) fn merge(&mut self, other_map_field: MapField) -> anyhow::Result<()> {
        match (self, other_map_field) {
            (MapField::Int32(self_int32_map), MapField::Int32(other_int32_map)) => {
                self_int32_map.merge(other_int32_map)
            }
            (MapField::Int64(self_int64_map), MapField::Int64(other_int64_map)) => {
                self_int64_map.merge(other_int64_map)
            }
            (MapField::Uint32(self_uint32_map), MapField::Uint32(other_uint32_map)) => {
                self_uint32_map.merge(other_uint32_map)
            }
            (MapField::Uint64(self_uint64_map), MapField::Uint64(other_uint64_map)) => {
                self_uint64_map.merge(other_uint64_map)
            }
            (MapField::Sint32(self_sint32_map), MapField::Sint32(other_sint32_map)) => {
                self_sint32_map.merge(other_sint32_map)
            }
            (MapField::Sint64(self_sint64_map), MapField::Sint64(other_sint64_map)) => {
                self_sint64_map.merge(other_sint64_map)
            }
            (MapField::Fixed32(self_fixed32_map), MapField::Fixed32(other_fixed32_map)) => {
                self_fixed32_map.merge(other_fixed32_map)
            }
            (MapField::Fixed64(self_fixed64_map), MapField::Fixed64(other_fixed64_map)) => {
                self_fixed64_map.merge(other_fixed64_map)
            }
            (MapField::Sfixed32(self_sfixed32_map), MapField::Sfixed32(other_sfixed32_map)) => {
                self_sfixed32_map.merge(other_sfixed32_map)
            }
            (MapField::Sfixed64(self_sfixed64_map), MapField::Sfixed64(other_sfixed64_map)) => {
                self_sfixed64_map.merge(other_sfixed64_map)
            }
            (MapField::Bool(self_bool_map), MapField::Bool(other_bool_map)) => {
                self_bool_map.merge(other_bool_map)
            }
            (MapField::String(self_string_map), MapField::String(other_string_map)) => {
                self_string_map.merge(other_string_map)
            }
            (self_map_field, other_map_field) => {
                anyhow::bail!(
                    "Attempting to merge map field with different types: Self is {:?}, other is {:?}",
                    self_map_field,
                    other_map_field
                )
            }
        }
    }
}

/// Container for dynamic protobuf map entries
#[derive(Debug, Clone, PartialEq)]
pub struct MapFieldContainer<K: Eq + Hash> {
    map: HashMap<K, SingularField>,
}

impl<K: Eq + Hash> MapFieldContainer<K> {
    fn new() -> Self {
        Self { map: HashMap::new() }
    }

    /// Gets the value (if it exists) that corresponds to the given key.
    pub fn get(&self, key: &K) -> Option<&SingularField> {
        self.map.get(key)
    }

    /// Gets an immutable reference to the underlying map.
    pub fn get_map(&self) -> &HashMap<K, SingularField> {
        &self.map
    }

    pub(crate) fn merge(&mut self, other: MapFieldContainer<K>) -> anyhow::Result<()> {
        let self_existing_value = self.map.values().next();
        let other_existing_value = other.map.values().next();

        if let (Some(self_value), Some(other_value)) = (self_existing_value, other_existing_value) {
            if !self_value.type_matches(other_value) {
                anyhow::bail!(
                    "Trying to merge map field with value type {} into map field with existing value type {}",
                    other_value.get_type_string(),
                    self_value.get_type_string()
                );
            }
        }
        self.map.extend(other.map);
        Ok(())
    }

    /// Inserts a key-value pair into the map. This is the only way to add entries to the MapFieldContainer.
    pub fn insert(&mut self, key: K, value: SingularField) -> anyhow::Result<()> {
        if let Some(existing_value) = self.map.values().next() {
            if !existing_value.type_matches(&value) {
                anyhow::bail!(
                    "Trying to insert value with type {} into map with existing type {}",
                    value.get_type_string(),
                    existing_value.get_type_string()
                );
            }
        }
        self.map.insert(key, value);
        Ok(())
    }

    /// Removes a key-value pair from the map.
    pub fn remove(&mut self, key: &K) -> Option<SingularField> {
        self.map.remove(key)
    }

    /// Returns the number of key-value pairs in the map.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns true if the map is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reflection::message::{
        dynamic_message::{DynamicMessage, SingularField},
        simple::SimpleField,
    };

    #[test]
    fn test_map_field_container_new() {
        let container: MapFieldContainer<i32> = MapFieldContainer::new();
        assert!(container.map.is_empty());
    }

    #[test]
    fn test_map_field_container_insert_and_get() {
        let mut container = MapFieldContainer::new();
        let key = 1;
        let value = SingularField::Simple(SimpleField::Int32(100));

        assert!(container.insert(key, value.clone()).is_ok());
        assert_eq!(container.len(), 1);
        assert_eq!(container.get(&key), Some(&value));
        assert_eq!(container.get(&2), None);
    }

    #[test]
    fn test_map_field_container_insert_type_mismatch_fails() {
        let mut container = MapFieldContainer::new();
        let value1 = SingularField::Simple(SimpleField::Int32(100));
        let value2 = SingularField::Simple(SimpleField::Float(200.0));

        assert!(container.insert(1, value1).is_ok());
        let result = container.insert(2, value2);
        assert!(result.is_err());
    }

    #[test]
    fn test_map_field_container_merge_type_mismatch_fails() {
        let mut container1 = MapFieldContainer::new();
        container1.insert(1, SingularField::Simple(SimpleField::Int32(100))).unwrap();

        let mut container2 = MapFieldContainer::new();
        container2.insert(2, SingularField::Simple(SimpleField::Float(200.0))).unwrap();

        let result = container1.merge(container2);
        assert!(result.is_err());
    }

    #[test]
    fn test_map_field_container_insert_message_type_mismatch_fails() {
        let mut container = MapFieldContainer::new();
        let msg1 = DynamicMessage::new("test.Message1".to_string());
        let msg2 = DynamicMessage::new("test.Message2".to_string());
        let value1 = SingularField::Message(Box::new(msg1));
        let value2 = SingularField::Message(Box::new(msg2));

        assert!(container.insert(1, value1).is_ok());
        let result = container.insert(2, value2);
        assert!(result.is_err());
    }

    #[test]
    fn test_map_field_container_merge() {
        let mut container1 = MapFieldContainer::new();
        container1.insert(1, SingularField::Simple(SimpleField::Int32(100))).unwrap();
        container1.insert(2, SingularField::Simple(SimpleField::Int32(200))).unwrap();

        let mut container2 = MapFieldContainer::new();
        container2.insert(2, SingularField::Simple(SimpleField::Int32(300))).unwrap(); // Overwrites
        container2.insert(3, SingularField::Simple(SimpleField::Int32(400))).unwrap();

        assert!(container1.merge(container2).is_ok());
        assert_eq!(container1.len(), 3);
        assert_eq!(container1.get(&1), Some(&SingularField::Simple(SimpleField::Int32(100))));
        assert_eq!(container1.get(&2), Some(&SingularField::Simple(SimpleField::Int32(300))));
        assert_eq!(container1.get(&3), Some(&SingularField::Simple(SimpleField::Int32(400))));
    }

    #[test]
    fn test_map_field_container_remove() {
        let mut container = MapFieldContainer::new();
        let key = 1;
        let value = SingularField::Simple(SimpleField::Int32(100));
        container.insert(key, value.clone()).unwrap();

        assert_eq!(container.len(), 1);
        let removed = container.remove(&key);
        assert_eq!(removed, Some(value));
        assert_eq!(container.len(), 0);
        assert_eq!(container.get(&key), None);

        let removed_again = container.remove(&key);
        assert_eq!(removed_again, None);
    }

    #[test]
    fn test_map_field_container_len() {
        let mut container = MapFieldContainer::new();
        assert_eq!(container.len(), 0);

        container.insert(1, SingularField::Simple(SimpleField::Int32(100))).unwrap();
        assert_eq!(container.len(), 1);

        container.insert(2, SingularField::Simple(SimpleField::Int32(200))).unwrap();
        assert_eq!(container.len(), 2);

        container.remove(&1);
        assert_eq!(container.len(), 1);

        container.remove(&2);
        assert_eq!(container.len(), 0);
    }

    #[test]
    fn test_merge_map_field_succeeds() {
        let mut container1 = MapFieldContainer::new();
        container1.insert(1, SingularField::Simple(SimpleField::Int32(100))).unwrap();
        container1.insert(2, SingularField::Simple(SimpleField::Int32(200))).unwrap();

        let mut container2 = MapFieldContainer::new();
        container2.insert(2, SingularField::Simple(SimpleField::Int32(300))).unwrap(); // Overwrites
        container2.insert(3, SingularField::Simple(SimpleField::Int32(400))).unwrap();

        let mut map_field_one = MapField::Int32(container1);
        let map_field_two = MapField::Int32(container2);

        assert!(map_field_one.merge(map_field_two).is_ok());
        assert_eq!(map_field_one.len(), 3);
    }

    #[test]
    fn test_merge_map_field_on_key_type_mismatch_fails() {
        let mut container1 = MapFieldContainer::new();
        container1.insert(1, SingularField::Simple(SimpleField::Int32(100))).unwrap();
        container1.insert(2, SingularField::Simple(SimpleField::Int32(200))).unwrap();

        let mut container2 = MapFieldContainer::new();
        container2.insert(2, SingularField::Simple(SimpleField::Int32(300))).unwrap();
        container2.insert(3, SingularField::Simple(SimpleField::Int32(400))).unwrap();

        let mut map_field_one = MapField::Int32(container1);
        let map_field_two = MapField::Sint32(container2);

        assert!(map_field_one.merge(map_field_two).is_err());
        assert_eq!(map_field_one.len(), 2);
    }

    #[test]
    fn test_merge_map_field_on_value_type_mismatch_fails() {
        let mut container1 = MapFieldContainer::new();
        container1.insert(1, SingularField::Simple(SimpleField::Int32(100))).unwrap();

        let mut container2 = MapFieldContainer::new();
        container2
            .insert(
                3,
                SingularField::Message(Box::new(DynamicMessage::new(
                    "test_message_two".to_owned(),
                ))),
            )
            .unwrap();

        let mut map_field_one = MapField::Int32(container1);
        let map_field_two = MapField::Int32(container2);

        assert!(map_field_one.merge(map_field_two).is_err());
        assert_eq!(map_field_one.len(), 1);
    }

    #[test]
    fn test_merge_map_field_on_simple_value_type_mismatch_fails() {
        let mut container1 = MapFieldContainer::new();
        container1.insert(1, SingularField::Simple(SimpleField::Int32(100))).unwrap();
        container1.insert(2, SingularField::Simple(SimpleField::Int32(200))).unwrap();

        let mut container2 = MapFieldContainer::new();
        container2.insert(2, SingularField::Simple(SimpleField::Fixed32(300))).unwrap();
        container2.insert(3, SingularField::Simple(SimpleField::Fixed32(400))).unwrap();

        let mut map_field_one = MapField::Int32(container1);
        let map_field_two = MapField::Int32(container2);

        assert!(map_field_one.merge(map_field_two).is_err());
        assert_eq!(map_field_one.len(), 2);
    }

    #[test]
    fn test_merge_map_field_on_message_value_type_mismatch_fails() {
        let mut container1 = MapFieldContainer::new();
        container1
            .insert(
                1,
                SingularField::Message(Box::new(DynamicMessage::new(
                    "test_message_one".to_owned(),
                ))),
            )
            .unwrap();
        container1
            .insert(
                2,
                SingularField::Message(Box::new(DynamicMessage::new(
                    "test_message_one".to_owned(),
                ))),
            )
            .unwrap();

        let mut container2 = MapFieldContainer::new();
        container2
            .insert(
                3,
                SingularField::Message(Box::new(DynamicMessage::new(
                    "test_message_two".to_owned(),
                ))),
            )
            .unwrap();
        container2
            .insert(
                4,
                SingularField::Message(Box::new(DynamicMessage::new(
                    "test_message_two".to_owned(),
                ))),
            )
            .unwrap();

        let mut map_field_one = MapField::Int32(container1);
        let map_field_two = MapField::Int32(container2);

        assert!(map_field_one.merge(map_field_two).is_err());
        assert_eq!(map_field_one.len(), 2);
    }
}
