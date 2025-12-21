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
use protobuf::rt::WireType;

use crate::reflection::codec::api::protobuf::ProtobufReader;
use crate::reflection::message::api::protobuf::MapField;
use crate::reflection::message::api::protobuf::MapFieldContainer;
use crate::reflection::message::api::protobuf::SimpleField;
use crate::reflection::message::api::protobuf::SingularField;
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

    pub(crate) fn parse_from_reader(
        &self,
        reader: &mut ProtobufReader,
        wire_type: WireType,
    ) -> anyhow::Result<MapField> {
        // Map entries are serialized as a virtual message with 2 fields
        if wire_type != WireType::LengthDelimited {
            anyhow::bail!("Unexpected wire type, expected length delimited wire type for map entry, found {wire_type:?}")
        }
        let length = reader.read_raw_varint32()?;
        self.parse_map_entry_from_reader(reader, i64::from(length))
    }

    fn parse_map_entry_from_reader(
        &self,
        reader: &mut ProtobufReader,
        length: i64,
    ) -> anyhow::Result<MapField> {
        let mut key_field: Option<SimpleField> = None;
        let mut value_field: Option<SingularField> = None;

        let initial_reader_position = reader.pos();
        let mut read_bytes = 0;
        while !reader.eof()? && read_bytes < length {
            let Some(field_tag) = reader.read_raw_tag_or_eof()? else {
                anyhow::bail!("Unexpected EOF")
            };
            let wire_type = WireType::new(field_tag & 0x7).ok_or_else(|| {
                anyhow::anyhow!("Invalid wire type: {}. Field tag: {}", field_tag & 0x7, field_tag)
            })?;
            let field_number = field_tag >> 3;

            if field_number == 1 {
                // read the key field
                let key: SimpleField = match self.key_metadata {
                    MapKeyType::Int32 => SimpleField::Int32(reader.read_int32()?),
                    MapKeyType::Int64 => SimpleField::Int64(reader.read_int64()?),
                    MapKeyType::Uint32 => SimpleField::Uint32(reader.read_uint32()?),
                    MapKeyType::Uint64 => SimpleField::Uint64(reader.read_uint64()?),
                    MapKeyType::Sint32 => SimpleField::Sint32(reader.read_sint32()?),
                    MapKeyType::Sint64 => SimpleField::Sint64(reader.read_sint64()?),
                    MapKeyType::Fixed32 => SimpleField::Fixed32(reader.read_fixed32()?),
                    MapKeyType::Fixed64 => SimpleField::Fixed64(reader.read_fixed64()?),
                    MapKeyType::Sfixed32 => SimpleField::Sfixed32(reader.read_sfixed32()?),
                    MapKeyType::Sfixed64 => SimpleField::Sfixed64(reader.read_sfixed64()?),
                    MapKeyType::Bool => SimpleField::Bool(reader.read_bool()?),
                    MapKeyType::String => SimpleField::String(reader.read_string()?),
                };
                if let Some(ref mut existing_key_field) = key_field {
                    existing_key_field.merge(key)?;
                } else {
                    key_field = Some(key);
                }
            } else if field_number == 2 {
                // read the value field
                let value = self.value_metadata.parse_from_reader(reader, wire_type)?;
                if let Some(ref mut existing_value_field) = value_field {
                    existing_value_field.merge(value)?;
                } else {
                    value_field = Some(value);
                }
            } else {
                reader.skip_field(wire_type)?;
            }
            read_bytes = (reader.pos() - initial_reader_position) as i64;
        }

        if read_bytes != length {
            anyhow::bail!(
                "Length mismatch: expected to read {length} bytes, but read {read_bytes} bytes",
            )
        }
        build_map_field(key_field, value_field)
    }
}

fn build_map_field(
    key_field: Option<SimpleField>,
    value_field: Option<SingularField>,
) -> anyhow::Result<MapField> {
    let Some(key_field) = key_field else { anyhow::bail!("Missing key for map entry") };
    let Some(value) = value_field else { anyhow::bail!("Missing value for map entry") };
    let map_field = match key_field {
        SimpleField::Int32(key) => {
            let mut map_field_container = MapFieldContainer::new();
            map_field_container.insert(key, value)?;
            MapField::Int32(map_field_container)
        }
        SimpleField::Int64(key) => {
            let mut map_field_container = MapFieldContainer::new();
            map_field_container.insert(key, value)?;
            MapField::Int64(map_field_container)
        }
        SimpleField::Uint32(key) => {
            let mut map_field_container = MapFieldContainer::new();
            map_field_container.insert(key, value)?;
            MapField::Uint32(map_field_container)
        }
        SimpleField::Uint64(key) => {
            let mut map_field_container = MapFieldContainer::new();
            map_field_container.insert(key, value)?;
            MapField::Uint64(map_field_container)
        }
        SimpleField::Sint32(key) => {
            let mut map_field_container = MapFieldContainer::new();
            map_field_container.insert(key, value)?;
            MapField::Sint32(map_field_container)
        }
        SimpleField::Sint64(key) => {
            let mut map_field_container = MapFieldContainer::new();
            map_field_container.insert(key, value)?;
            MapField::Sint64(map_field_container)
        }
        SimpleField::Fixed32(key) => {
            let mut map_field_container = MapFieldContainer::new();
            map_field_container.insert(key, value)?;
            MapField::Fixed32(map_field_container)
        }
        SimpleField::Fixed64(key) => {
            let mut map_field_container = MapFieldContainer::new();
            map_field_container.insert(key, value)?;
            MapField::Fixed64(map_field_container)
        }
        SimpleField::Sfixed32(key) => {
            let mut map_field_container = MapFieldContainer::new();
            map_field_container.insert(key, value)?;
            MapField::Sfixed32(map_field_container)
        }
        SimpleField::Sfixed64(key) => {
            let mut map_field_container = MapFieldContainer::new();
            map_field_container.insert(key, value)?;
            MapField::Sfixed64(map_field_container)
        }
        SimpleField::Bool(key) => {
            let mut map_field_container = MapFieldContainer::new();
            map_field_container.insert(key, value)?;
            MapField::Bool(map_field_container)
        }
        SimpleField::String(key) => {
            let mut map_field_container = MapFieldContainer::new();
            map_field_container.insert(key, value)?;
            MapField::String(map_field_container)
        }
        _ => {
            anyhow::bail!("Internal error")
        }
    };
    Ok(map_field)
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
