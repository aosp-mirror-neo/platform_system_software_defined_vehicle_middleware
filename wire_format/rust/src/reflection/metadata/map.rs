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
// limitations under the License

use protobuf::reflect::FieldDescriptor;
use protobuf::reflect::MessageDescriptor;

use crate::reflection::metadata::error::MetadataBuildError;
use crate::reflection::metadata::field_type::ProtobufSimpleType;
use crate::reflection::metadata::message::FieldMetadata;
use crate::reflection::metadata::message::MessageMetadata;
use crate::reflection::metadata::message::SingularTypeMetadata;

/// Metadata for a map entry type. A map field is wire-format equivalent to a repeated message
/// field, where the message has 2 fields: a `key` field with number 1 and a `value` field
/// with number 2.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MapMetadata {
    // A map's key type can only be a subset of scalar types
    key_metadata: MapKeyType,
    // A map's value type can only be a non-repeated type
    value_metadata: SingularTypeMetadata,
}

/// Types that can be used as map keys - these are all scalar types except floats, doubles and
/// bytes fields
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MapKeyType {
    /// Int32
    Int32,
    /// Int64
    Int64,
    /// Uint32
    Uint32,
    /// Uint64
    Uint64,
    /// Sint32
    Sint32,
    /// Sint64
    Sint64,
    /// Fixed32
    Fixed32,
    /// Fixed64
    Fixed64,
    /// Sfixed32
    Sfixed32,
    /// Sfixed64
    Sfixed64,
    /// Bool
    Bool,
    /// String
    String,
}

impl TryFrom<ProtobufSimpleType> for MapKeyType {
    type Error = anyhow::Error;

    fn try_from(value: ProtobufSimpleType) -> Result<Self, Self::Error> {
        match value {
            ProtobufSimpleType::Int32 => Ok(Self::Int32),
            ProtobufSimpleType::Int64 => Ok(Self::Int64),
            ProtobufSimpleType::Uint32 => Ok(Self::Uint32),
            ProtobufSimpleType::Uint64 => Ok(Self::Uint64),
            ProtobufSimpleType::Sint32 => Ok(Self::Sint32),
            ProtobufSimpleType::Sint64 => Ok(Self::Sint64),
            ProtobufSimpleType::Fixed32 => Ok(Self::Fixed32),
            ProtobufSimpleType::Fixed64 => Ok(Self::Fixed64),
            ProtobufSimpleType::Sfixed32 => Ok(Self::Sfixed32),
            ProtobufSimpleType::Sfixed64 => Ok(Self::Sfixed64),
            ProtobufSimpleType::Bool => Ok(Self::Bool),
            ProtobufSimpleType::String => Ok(Self::String),
            _ => {
                anyhow::bail!("Unsupported type for map key: {value:?}")
            }
        }
    }
}

impl MapMetadata {
    fn new(key_metadata: MapKeyType, value_metadata: SingularTypeMetadata) -> Self {
        Self { key_metadata, value_metadata }
    }

    pub(crate) fn build(
        field_descriptor: &FieldDescriptor,
        map_entry_descriptor: &MessageDescriptor,
    ) -> Result<Self, MetadataBuildError> {
        if !field_descriptor.is_map() {
            return Err(MetadataBuildError::MapTypeExpected(field_descriptor.full_name()));
        }
        if map_entry_descriptor.fields().count() != 2 {
            return Err(MetadataBuildError::InvalidMapEntryType(field_descriptor.full_name()));
        }
        let mut map_entry_metadata = MessageMetadata::build(map_entry_descriptor)?;
        let key_metadata = map_entry_metadata.remove_field(&1).ok_or_else(|| {
            MetadataBuildError::MissingMapKeyMetadata(field_descriptor.full_name())
        })?;
        let FieldMetadata::Singular(SingularTypeMetadata::Simple(key_simple_metadata)) =
            key_metadata
        else {
            return Err(MetadataBuildError::InvalidMapEntryType(field_descriptor.full_name()));
        };
        let key_metadata = MapKeyType::try_from(key_simple_metadata.get_field_type())
            .map_err(|_| MetadataBuildError::InvalidMapEntryType(field_descriptor.full_name()))?;

        let value_metadata = map_entry_metadata.remove_field(&2).ok_or_else(|| {
            MetadataBuildError::MissingMapValueMetadata(field_descriptor.full_name())
        })?;
        let FieldMetadata::Singular(value_metadata) = value_metadata else {
            return Err(MetadataBuildError::InvalidMapEntryType(field_descriptor.full_name()));
        };
        Ok(Self::new(key_metadata, value_metadata))
    }

    /// Map key metadata
    pub fn get_key_metadata(&self) -> MapKeyType {
        self.key_metadata
    }

    /// Map value metadata
    pub fn get_value_metadata(&self) -> &SingularTypeMetadata {
        &self.value_metadata
    }
}

#[cfg(test)]
mod tests {
    use protobuf::reflect::FileDescriptor;
    use std::collections::HashMap;

    use crate::reflection::metadata::{field_type::ProtobufSimpleType, simple::SimpleMetadata};

    use super::*;

    fn mock_file_descriptor(file_name: &str, file_content: &str) -> FileDescriptor {
        let temp_dir = tempfile::tempdir().unwrap();
        let tempfile = temp_dir.path().join(file_name);
        std::fs::write(&tempfile, file_content).unwrap();

        let file_descriptor_protos = protobuf_parse::Parser::new()
            .pure()
            .include(&temp_dir)
            .input(&tempfile)
            .parse_and_typecheck()
            .unwrap()
            .file_descriptors;

        FileDescriptor::new_dynamic(file_descriptor_protos[0].clone(), &[]).unwrap()
    }

    #[test]
    fn test_build_map_metadata_with_non_map_metadata_fails() {
        let file_content = "syntax = 'proto3'; message TestProto { uint32 a = 1; }";
        let file_descriptor = mock_file_descriptor("test.proto", file_content);
        let message_descriptor = file_descriptor
            .message_by_full_name(".TestProto")
            .unwrap_or_else(|| panic!("Message descriptor must be created"));
        let field_descriptor = message_descriptor
            .field_by_name("a")
            .unwrap_or_else(|| panic!("Field descriptor must be created"));

        let map_entry_metadata = MapMetadata::build(&field_descriptor, &message_descriptor);

        assert!(map_entry_metadata.is_err());
        assert_eq!(
            map_entry_metadata.unwrap_err(),
            MetadataBuildError::MapTypeExpected(field_descriptor.full_name())
        )
    }

    #[test]
    fn test_build_map_metadata_for_supported_keys_succeeds() {
        let map_key_types = HashMap::from([
            ("int32", MapKeyType::Int32),
            ("int64", MapKeyType::Int64),
            ("uint32", MapKeyType::Uint32),
            ("uint64", MapKeyType::Uint64),
            ("sint32", MapKeyType::Sint32),
            ("sint64", MapKeyType::Sint64),
            ("fixed32", MapKeyType::Fixed32),
            ("fixed64", MapKeyType::Fixed64),
            ("sfixed32", MapKeyType::Sfixed32),
            ("sfixed64", MapKeyType::Sfixed64),
            ("bool", MapKeyType::Bool),
            ("string", MapKeyType::String),
        ]);

        let file_name_template = |type_name: &str| format!("test_{type_name}.proto");
        let file_content_template = |type_name: &str| {
            format!("syntax = 'proto3'; message TestProto {{ map<{type_name}, string> a = 1; }}")
        };

        for (type_name, type_value) in map_key_types {
            let file_descriptor = mock_file_descriptor(
                &file_name_template(type_name),
                &file_content_template(type_name),
            );
            let message_descriptor = file_descriptor
                .message_by_full_name(".TestProto")
                .unwrap_or_else(|| panic!("Message descriptor must be created, type: {type_name}"));
            let field_descriptor = message_descriptor
                .field_by_name("a")
                .unwrap_or_else(|| panic!("Field descriptor must be created, type: {type_name}"));

            let map_entry_descriptors: Vec<MessageDescriptor> =
                message_descriptor.nested_messages().filter(|m| m.is_map_entry()).take(1).collect();
            let map_entry_descriptor =
                map_entry_descriptors.first().expect("Map Entry descriptor must be created");

            let map_metadata = MapMetadata::build(&field_descriptor, map_entry_descriptor)
                .unwrap_or_else(|_| panic!("Map Metadata should be created, type: {type_name}"));

            assert_eq!(map_metadata.get_key_metadata(), type_value);
            assert_eq!(
                map_metadata.get_value_metadata(),
                &SingularTypeMetadata::Simple(SimpleMetadata::new(ProtobufSimpleType::String))
            );
        }
    }
}
