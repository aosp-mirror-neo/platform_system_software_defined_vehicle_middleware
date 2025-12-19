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

/// Container for dynamic protobuf map entries
#[derive(Debug, Clone, PartialEq)]
pub struct MapFieldContainer<K: Eq + Hash> {
    map: HashMap<K, SingularField>,
}

impl<K: Eq + Hash> MapFieldContainer<K> {
    fn new() -> Self {
        Self { map: HashMap::new() }
    }

    pub fn get(&self, key: &K) -> Option<&SingularField> {
        self.map.get(key)
    }

    pub fn merge(&mut self, other: MapFieldContainer<K>) -> anyhow::Result<()> {
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

    pub fn remove(&mut self, key: &K) -> Option<SingularField> {
        self.map.remove(key)
    }

    pub fn len(&self) -> usize {
        self.map.len()
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
}
