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

use std::collections::{HashMap, HashSet};

use protobuf::reflect::FieldDescriptor;
use protobuf::reflect::MessageDescriptor;
use protobuf::reflect::RuntimeFieldType;
use protobuf::reflect::RuntimeType;
use protobuf::reflect::Syntax;
use protobuf::rt::WireType;

use crate::reflection::codec::api::protobuf::ProtobufReader;
use crate::reflection::message::api::protobuf::DynamicMessage;
use crate::reflection::message::api::protobuf::DynamicMessageField;
use crate::reflection::message::api::protobuf::SingularField;
use crate::reflection::metadata::error::MetadataBuildError;
use crate::reflection::metadata::map::MapMetadata;
use crate::reflection::metadata::repeated::parse_repeated_field;
use crate::reflection::metadata::simple::SimpleMetadata;

/// Protobuf Message metadata that contains information relevant to wire format de/serialization, message
/// full name, field names declared in the message and fields whose presence can be tracked
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MessageMetadata {
    // File's syntax, can either be proto2 or proto3
    syntax: Syntax,
    // Map from field number to the metadata associated with the field
    fields_metadata: HashMap<u32, FieldMetadata>,
    // Message's proto full name
    message_full_name: String,
    // Map from field name to the field number
    field_name_to_number: HashMap<String, u32>,
    // Set of fields declared within oneofs, plus message fields in proto3 syntax that track their
    // presence (singular message fields and `optional` fields)
    oneofs_and_proto3_presence_tracked_fields: HashSet<u32>,
}

/// Metadata for a message field
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum FieldMetadata {
    /// Singular field
    Singular(SingularTypeMetadata),
    /// Repeated field
    Repeated(SingularTypeMetadata),
    /// Map field
    Map(Box<MapMetadata>),
}

impl FieldMetadata {
    fn parse_from_reader(
        &self,
        reader: &mut ProtobufReader,
        wire_type: WireType,
    ) -> anyhow::Result<DynamicMessageField> {
        match self {
            FieldMetadata::Singular(singular_metadata) => Ok(DynamicMessageField::Singular(
                singular_metadata.parse_from_reader(reader, wire_type)?,
            )),
            FieldMetadata::Repeated(singular_metadata) => Ok(DynamicMessageField::Repeated(
                parse_repeated_field(reader, wire_type, singular_metadata)?,
            )),
            FieldMetadata::Map(map_metadata) => {
                Ok(DynamicMessageField::Map(map_metadata.parse_from_reader(reader, wire_type)?))
            }
        }
    }
}

/// Singular (non-repeated, non-map) metadata
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SingularTypeMetadata {
    /// Simple field type
    Simple(SimpleMetadata),
    /// Message field type
    Message(Box<MessageMetadata>),
}

impl SingularTypeMetadata {
    pub(crate) fn parse_from_reader(
        &self,
        reader: &mut ProtobufReader,
        wire_type: WireType,
    ) -> anyhow::Result<SingularField> {
        match self {
            SingularTypeMetadata::Simple(simple_metadata) => {
                Ok(SingularField::Simple(simple_metadata.parse_from_reader(reader, wire_type)?))
            }
            SingularTypeMetadata::Message(message_metadata) => Ok(SingularField::Message(
                Box::new(message_metadata.parse_message_field_from_reader(reader, wire_type)?),
            )),
        }
    }
}

impl MessageMetadata {
    fn new(
        syntax: Syntax,
        message_full_name: String,
        fields_metadata: HashMap<u32, FieldMetadata>,
        field_name_to_number: HashMap<String, u32>,
        oneofs_and_proto3_presence_tracked_fields: HashSet<u32>,
    ) -> Self {
        Self {
            syntax,
            message_full_name,
            fields_metadata,
            field_name_to_number,
            oneofs_and_proto3_presence_tracked_fields,
        }
    }

    pub(crate) fn remove_field(&mut self, field_number: &u32) -> Option<FieldMetadata> {
        self.fields_metadata.remove(field_number)
    }

    /// Build a MessageMetadata object from a MessageDescriptor
    pub fn build(message_descriptor: &MessageDescriptor) -> Result<Self, MetadataBuildError> {
        let mut fields_metadata = HashMap::new();
        let mut field_name_to_number = HashMap::new();

        // proto3 optional fields will also have a "synthetic oneof descriptor"
        let mut oneofs_and_proto3_presence_tracked_fields = message_descriptor
            .all_oneofs()
            .flat_map(|oneof| oneof.fields().collect::<Vec<_>>())
            .map(|field| field.number() as u32)
            .collect::<HashSet<u32>>();

        // 'map' fields are assigned a virtual message that encapsulates the key and value types. This is
        // because map fields are wire-format equivalent to a `repeated Message` field. We collect these
        // virtual messages here and pass them when required to `MapMetadata::build`
        let map_entry_fields: HashMap<String, MessageDescriptor> = message_descriptor
            .nested_messages()
            .filter(|m| m.is_map_entry())
            .map(|m| (m.full_name().trim_start_matches('.').to_owned(), m))
            .collect();

        let message_proto_syntax = message_descriptor.file_descriptor().syntax();
        for field_descriptor in message_descriptor.fields() {
            let field_metadata = if field_descriptor.is_repeated() {
                Self::build_repeated_metadata(&field_descriptor)?
            } else if field_descriptor.is_map() {
                Self::build_map_metadata(&field_descriptor, &map_entry_fields)?
            } else {
                Self::build_singular_metadata(&field_descriptor)?
            };
            let field_number = field_descriptor.number() as u32;

            if message_proto_syntax == Syntax::Proto3 {
                if let FieldMetadata::Singular(SingularTypeMetadata::Message(_)) = &field_metadata {
                    // proto3 message field presence is always tracked
                    oneofs_and_proto3_presence_tracked_fields.insert(field_number);
                }
            }
            field_name_to_number.insert(field_descriptor.name().to_owned(), field_number);
            fields_metadata.insert(field_number, field_metadata);
        }

        Ok(Self::new(
            message_proto_syntax,
            message_descriptor.full_name().to_owned(),
            fields_metadata,
            field_name_to_number,
            oneofs_and_proto3_presence_tracked_fields,
        ))
    }

    /// Get metadata for field with number `field_number`
    pub fn get_field_metadata(&self, field_number: &u32) -> Option<&FieldMetadata> {
        self.fields_metadata.get(field_number)
    }

    /// Get metadata for field with name `field_name`
    pub fn get_field_metadata_by_name(&self, field_name: &str) -> Option<&FieldMetadata> {
        self.field_name_to_number
            .get(field_name)
            .and_then(|field_number| self.fields_metadata.get(field_number))
    }

    /// Checks whether a field with number `field_number` tracks its presence in the message.
    /// Field presence is tracked for all oneofs, singular fields of all types in proto2 syntax,
    /// singular fields with an embedded message type in proto3 syntax and fields that have the
    /// `optional` label in proto3 syntax.
    pub fn is_field_presence_tracked(&self, field_number: &u32) -> bool {
        (self.syntax == Syntax::Proto2
            && self
                .get_field_metadata(field_number)
                .is_some_and(|field_metadata| matches!(field_metadata, FieldMetadata::Singular(_))))
            || (self.oneofs_and_proto3_presence_tracked_fields.contains(field_number))
    }

    /// Get the message's full name
    pub fn get_message_full_name(&self) -> &str {
        &self.message_full_name
    }

    fn build_singular_metadata(
        field_descriptor: &FieldDescriptor,
    ) -> Result<FieldMetadata, MetadataBuildError> {
        if let Ok(simple_metadata) = SimpleMetadata::build(field_descriptor.proto().type_()) {
            Ok(FieldMetadata::Singular(SingularTypeMetadata::Simple(simple_metadata)))
        } else {
            let RuntimeFieldType::Singular(runtime_type) = field_descriptor.runtime_field_type()
            else {
                return Err(MetadataBuildError::SingularTypeExpected(field_descriptor.full_name()));
            };
            let RuntimeType::Message(embedded_message_descriptor) = runtime_type else {
                return Err(MetadataBuildError::expected_message_type(
                    field_descriptor.full_name(),
                    &runtime_type,
                ));
            };
            let message_field_type = Self::build(&embedded_message_descriptor)?;
            Ok(FieldMetadata::Singular(SingularTypeMetadata::Message(Box::new(message_field_type))))
        }
    }

    fn build_map_metadata(
        field_descriptor: &FieldDescriptor,
        map_entry_fields: &HashMap<String, MessageDescriptor>,
    ) -> Result<FieldMetadata, MetadataBuildError> {
        let map_entry_type_name = field_descriptor.proto().type_name().trim_start_matches('.');
        let map_entry_descriptor = map_entry_fields.get(map_entry_type_name).ok_or(
            MetadataBuildError::MissingMapEntryDescriptor(
                map_entry_type_name.to_owned(),
                field_descriptor.full_name(),
            ),
        )?;
        Ok(FieldMetadata::Map(Box::new(MapMetadata::build(
            field_descriptor,
            map_entry_descriptor,
        )?)))
    }

    fn build_repeated_metadata(
        field_descriptor: &FieldDescriptor,
    ) -> Result<FieldMetadata, MetadataBuildError> {
        let RuntimeFieldType::Repeated(runtime_type) = field_descriptor.runtime_field_type() else {
            return Err(MetadataBuildError::RepeatedTypeExpected(field_descriptor.full_name()));
        };
        let protobuf_field_type = field_descriptor.proto().type_();

        if let Ok(simple_metadata) = SimpleMetadata::build(protobuf_field_type) {
            Ok(FieldMetadata::Repeated(SingularTypeMetadata::Simple(simple_metadata)))
        } else {
            let RuntimeType::Message(embedded_message_descriptor) = runtime_type else {
                return Err(MetadataBuildError::expected_message_type(
                    field_descriptor.full_name(),
                    &runtime_type,
                ));
            };
            let message_metadata = MessageMetadata::build(&embedded_message_descriptor)?;
            Ok(FieldMetadata::Repeated(SingularTypeMetadata::Message(Box::new(message_metadata))))
        }
    }

    /// Parse a `DynamicMessage` from bytes
    pub fn parse_from_bytes(&self, bytes: &[u8]) -> anyhow::Result<DynamicMessage> {
        let mut reader = ProtobufReader::new(bytes);
        self.parse_message_from_reader(&mut reader, -1)
    }

    /// Parse a `DynamicMessage` from bytes prefixed with the message's serialized length
    pub fn parse_from_bytes_length_delimited(
        &self,
        bytes: &[u8],
    ) -> anyhow::Result<DynamicMessage> {
        let mut reader = ProtobufReader::new(bytes);
        let length = reader.read_raw_varint32()?;
        reader.push_limit(u64::from(length))?;
        self.parse_message_from_reader(&mut reader, -1)
    }

    pub(crate) fn parse_message_field_from_reader(
        &self,
        reader: &mut ProtobufReader,
        wire_type: WireType,
    ) -> anyhow::Result<DynamicMessage> {
        if wire_type != WireType::LengthDelimited {
            anyhow::bail!("Unexpected wire type, expected length delimited wire type for message, found {wire_type:?}")
        }
        let length = reader.read_raw_varint32()?;
        self.parse_message_from_reader(reader, i64::from(length))
    }

    pub(crate) fn parse_message_from_reader(
        &self,
        reader: &mut ProtobufReader,
        length: i64,
    ) -> anyhow::Result<DynamicMessage> {
        let mut message = DynamicMessage::new(self.get_message_full_name().to_owned());
        let initial_reader_position = reader.pos();
        let mut read_bytes = 0;
        while !reader.eof()? && (length == -1 || read_bytes < length) {
            let Some(field_tag) = reader.read_raw_tag_or_eof()? else {
                anyhow::bail!("Unexpected EOF")
            };
            let wire_type = WireType::new(field_tag & 0x7).ok_or_else(|| {
                anyhow::anyhow!("Invalid wire type: {}. Field tag: {}", field_tag & 0x7, field_tag)
            })?;
            let field_number = field_tag >> 3;

            if let Some(field_metadata) = self.get_field_metadata(&field_number) {
                let message_field = field_metadata.parse_from_reader(reader, wire_type)?;
                message.add_field(field_number, message_field)?;
            } else {
                reader.skip_field(wire_type)?;
            }
            read_bytes = (reader.pos() - initial_reader_position) as i64;
        }
        if length != -1 && read_bytes != length {
            anyhow::bail!(
                "Length mismatch: expected to read {length} bytes, but read {read_bytes} bytes",
            )
        }
        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use crate::reflection::metadata::{field_type::ProtobufSimpleType, map::MapKeyType};
    use protobuf::{descriptor::field_descriptor_proto::Type, reflect::FileDescriptor};

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
    fn test_build_message_metadata_with_simple_field_succeeds() {
        const ENUM_TYPE_NAME: &str = "TestEnum";

        let simple_field_map = HashMap::from([
            ("int32", Type::TYPE_INT32),
            ("int64", Type::TYPE_INT64),
            ("uint32", Type::TYPE_UINT32),
            ("uint64", Type::TYPE_UINT64),
            ("sint32", Type::TYPE_SINT32),
            ("sint64", Type::TYPE_SINT64),
            ("fixed32", Type::TYPE_FIXED32),
            ("fixed64", Type::TYPE_FIXED64),
            ("sfixed32", Type::TYPE_SFIXED32),
            ("sfixed64", Type::TYPE_SFIXED64),
            ("double", Type::TYPE_DOUBLE),
            ("float", Type::TYPE_FLOAT),
            ("bool", Type::TYPE_BOOL),
            ("string", Type::TYPE_STRING),
            ("bytes", Type::TYPE_BYTES),
            (ENUM_TYPE_NAME, Type::TYPE_ENUM),
        ]);

        let file_name_template = |type_name: &str| format!("test_{type_name}.proto");
        let file_content_template = |type_name: &str| {
            format!("syntax = 'proto3'; message TestProto {{ {type_name} a = 1; }} enum {ENUM_TYPE_NAME} {{ UNKNOWN = 0; VALUE_1 = 1; VALUE_2 = 2;}}")
        };

        for (type_name, type_value) in simple_field_map {
            let file_descriptor = mock_file_descriptor(
                &file_name_template(type_name),
                &file_content_template(type_name),
            );
            let message_descriptor = file_descriptor
                .message_by_full_name(".TestProto")
                .unwrap_or_else(|| panic!("Message descriptor must be created, type: {type_name}"));

            let message_metadata =
                MessageMetadata::build(&message_descriptor).unwrap_or_else(|_| {
                    panic!("Message Metadata should be created, type: {type_name}")
                });
            assert_eq!(message_metadata.get_message_full_name(), "TestProto");
            assert_eq!(message_metadata.fields_metadata.len(), 1);
            assert_eq!(message_metadata.field_name_to_number.len(), 1);
            assert_eq!(message_metadata.oneofs_and_proto3_presence_tracked_fields.len(), 0);

            let field_metadata =
                message_metadata.get_field_metadata(&1).expect("Field metadata must exist");
            assert!(matches!(
                field_metadata,
                FieldMetadata::Singular(SingularTypeMetadata::Simple(_))
            ));

            let field_metadata_by_name = message_metadata
                .get_field_metadata_by_name("a")
                .expect("Field metadata must exist");
            assert_eq!(field_metadata_by_name, field_metadata);

            let FieldMetadata::Singular(SingularTypeMetadata::Simple(actual_simple_metadata)) =
                field_metadata
            else {
                panic!("Expected simple metadata, type: {type_name}");
            };
            let expected_simple_metadata = SimpleMetadata::build(type_value).unwrap_or_else(|_| {
                panic!("Expected simple metadata should be created, type: {type_name}")
            });
            assert_eq!(*actual_simple_metadata, expected_simple_metadata);
        }
    }

    #[test]
    fn test_build_message_metadata_with_repeated_field_succeeds() {
        let file_content = "syntax = 'proto3'; message TestProto { repeated uint32 a = 1; }";
        let file_descriptor = mock_file_descriptor("test.proto", file_content);
        let message_descriptor = file_descriptor
            .message_by_full_name(".TestProto")
            .expect("Message descriptor must be created");

        let message_metadata = MessageMetadata::build(&message_descriptor)
            .expect("Message metadata should be created");

        assert_eq!(message_metadata.get_message_full_name(), "TestProto");
        assert_eq!(message_metadata.fields_metadata.len(), 1);
        assert_eq!(message_metadata.field_name_to_number.len(), 1);
        assert_eq!(message_metadata.oneofs_and_proto3_presence_tracked_fields.len(), 0);

        let field_metadata =
            message_metadata.get_field_metadata(&1).expect("Field metadata must exist");
        assert!(matches!(field_metadata, FieldMetadata::Repeated(_)));

        let FieldMetadata::Repeated(actual_repeated_metadata) = field_metadata else {
            panic!("Expected repeated metadata");
        };
        let expected_repeated_metadata =
            SingularTypeMetadata::Simple(SimpleMetadata::new(ProtobufSimpleType::Uint32));

        assert_eq!(*actual_repeated_metadata, expected_repeated_metadata);
    }

    #[test]
    fn test_build_message_metadata_with_nested_message_field_succeeds() {
        let file_content = "syntax = 'proto3'; message TestProto { NestedMessage a = 1; } message NestedMessage { uint32 nested_a = 1; }";
        let file_descriptor = mock_file_descriptor("test.proto", file_content);
        let message_descriptor = file_descriptor
            .message_by_full_name(".TestProto")
            .expect("Message descriptor must be created");

        let message_metadata = MessageMetadata::build(&message_descriptor)
            .expect("Message metadata should be created");

        assert_eq!(message_metadata.get_message_full_name(), "TestProto");
        assert_eq!(message_metadata.fields_metadata.len(), 1);
        assert_eq!(message_metadata.field_name_to_number.len(), 1);
        assert_eq!(message_metadata.oneofs_and_proto3_presence_tracked_fields.len(), 1);
        assert!(message_metadata.is_field_presence_tracked(&1));

        let field_metadata =
            message_metadata.get_field_metadata(&1).expect("Field metadata must exist");
        assert!(matches!(
            field_metadata,
            FieldMetadata::Singular(SingularTypeMetadata::Message(_))
        ));

        let FieldMetadata::Singular(SingularTypeMetadata::Message(nested_message_metadata)) =
            field_metadata
        else {
            panic!("Expected message metadata");
        };

        assert_eq!(nested_message_metadata.get_message_full_name(), "NestedMessage");
        assert_eq!(nested_message_metadata.fields_metadata.len(), 1);
        assert_eq!(nested_message_metadata.field_name_to_number.len(), 1);
        assert_eq!(nested_message_metadata.oneofs_and_proto3_presence_tracked_fields.len(), 0);

        let nested_field_metadata =
            nested_message_metadata.get_field_metadata(&1).expect("Field metadata must exist");
        assert!(matches!(
            nested_field_metadata,
            FieldMetadata::Singular(SingularTypeMetadata::Simple(_))
        ));

        let FieldMetadata::Singular(SingularTypeMetadata::Simple(actual_simple_metadata)) =
            nested_field_metadata
        else {
            panic!("Expected simple metadata");
        };

        let expected_simple_metadata = SimpleMetadata::build(Type::TYPE_UINT32)
            .expect("Expected simple metadata should be created");
        assert_eq!(*actual_simple_metadata, expected_simple_metadata);
    }

    #[test]
    fn test_build_message_metadata_with_optional_field_in_proto3_syntax_succeeds() {
        let file_content = "syntax = 'proto3'; message TestProto { optional uint32 a = 1; }";
        let file_descriptor = mock_file_descriptor("test.proto", file_content);
        let message_descriptor = file_descriptor
            .message_by_full_name(".TestProto")
            .expect("Message descriptor must be created");

        let message_metadata = MessageMetadata::build(&message_descriptor)
            .expect("Message metadata should be created");

        assert_eq!(message_metadata.get_message_full_name(), "TestProto");
        assert_eq!(message_metadata.fields_metadata.len(), 1);
        assert_eq!(message_metadata.field_name_to_number.len(), 1);
        assert_eq!(message_metadata.oneofs_and_proto3_presence_tracked_fields.len(), 1);

        assert!(message_metadata.is_field_presence_tracked(&1));
    }

    #[test]
    fn test_build_message_metadata_with_optional_field_in_proto2_syntax_succeeds() {
        let file_content = "syntax = 'proto2'; message TestProto { required uint32 a = 1; }";
        let file_descriptor = mock_file_descriptor("test.proto", file_content);
        let message_descriptor = file_descriptor
            .message_by_full_name(".TestProto")
            .expect("Message descriptor must be created");

        let message_metadata = MessageMetadata::build(&message_descriptor)
            .expect("Message metadata should be created");

        assert_eq!(message_metadata.get_message_full_name(), "TestProto");
        assert_eq!(message_metadata.fields_metadata.len(), 1);
        assert_eq!(message_metadata.field_name_to_number.len(), 1);

        assert!(message_metadata.is_field_presence_tracked(&1));
    }

    #[test]
    fn test_build_message_metadata_with_oneof_succeeds() {
        let file_content =
            "syntax = 'proto3'; message TestProto { oneof value { uint32 a = 1; float b = 2;} }";
        let file_descriptor = mock_file_descriptor("test.proto", file_content);
        let message_descriptor = file_descriptor
            .message_by_full_name(".TestProto")
            .expect("Message descriptor must be created");

        let message_metadata = MessageMetadata::build(&message_descriptor)
            .expect("Message metadata should be created");

        assert_eq!(message_metadata.get_message_full_name(), "TestProto");
        assert_eq!(message_metadata.fields_metadata.len(), 2);
        assert_eq!(message_metadata.field_name_to_number.len(), 2);
        assert_eq!(message_metadata.oneofs_and_proto3_presence_tracked_fields.len(), 2);

        assert!(message_metadata.is_field_presence_tracked(&1));
        assert!(message_metadata.is_field_presence_tracked(&2));
    }

    #[test]
    fn test_build_message_metadata_with_map_field_succeeds() {
        let file_content = "syntax = 'proto3'; message TestProto { map<int32, string> a = 1; }";
        let file_descriptor = mock_file_descriptor("test.proto", file_content);
        let message_descriptor = file_descriptor
            .message_by_full_name(".TestProto")
            .expect("Message descriptor must be created");

        let mut message_metadata = MessageMetadata::build(&message_descriptor)
            .expect("Message metadata should be created");

        assert_eq!(message_metadata.get_message_full_name(), "TestProto");
        assert_eq!(message_metadata.fields_metadata.len(), 1);
        assert_eq!(message_metadata.field_name_to_number.len(), 1);
        assert_eq!(message_metadata.oneofs_and_proto3_presence_tracked_fields.len(), 0);

        assert!(matches!(message_metadata.get_field_metadata(&1), Some(FieldMetadata::Map(_))));

        let Some(FieldMetadata::Map(map_metadata)) = message_metadata.remove_field(&1) else {
            panic!("Field metadata must exist");
        };
        assert_eq!(map_metadata.get_key_metadata(), MapKeyType::Int32);

        let value_metadata = map_metadata.get_value_metadata();
        assert!(matches!(value_metadata, SingularTypeMetadata::Simple(_)));

        let SingularTypeMetadata::Simple(simple_metadata) = value_metadata else {
            panic!("Expected simple metadata for value type");
        };
        assert_eq!(simple_metadata.get_field_type(), ProtobufSimpleType::String);
    }

    #[test]
    fn test_build_simple_repeated_metadata_succeeds() {
        let file_content = "syntax = 'proto3'; message TestProto { repeated uint32 a = 1; }";
        let file_descriptor = mock_file_descriptor("test.proto", file_content);
        let message_descriptor = file_descriptor
            .message_by_full_name(".TestProto")
            .expect("Message descriptor must be created");

        let field_descriptor =
            message_descriptor.field_by_name("a").expect("Field descriptor must be created");

        let repeated_field_metadata = MessageMetadata::build_repeated_metadata(&field_descriptor);
        assert!(repeated_field_metadata.is_ok());
        let repeated_metadata = repeated_field_metadata.unwrap();
        assert!(matches!(
            repeated_metadata,
            FieldMetadata::Repeated(SingularTypeMetadata::Simple(_))
        ));
    }

    #[test]
    fn test_build_message_repeated_metadata_succeeds() {
        let file_content = "syntax = 'proto3'; message TestProto { repeated NestedMessage a = 1; } message NestedMessage { uint32 a = 1; }";
        let file_descriptor = mock_file_descriptor("test.proto", file_content);
        let message_descriptor = file_descriptor
            .message_by_full_name(".TestProto")
            .expect("Message descriptor must be created");

        let field_descriptor =
            message_descriptor.field_by_name("a").expect("Field descriptor must be created");

        let repeated_field_metadata = MessageMetadata::build_repeated_metadata(&field_descriptor);
        assert!(repeated_field_metadata.is_ok());
        let repeated_metadata = repeated_field_metadata.unwrap();
        assert!(matches!(
            repeated_metadata,
            FieldMetadata::Repeated(SingularTypeMetadata::Message(_))
        ));
    }

    #[test]
    fn test_build_non_repeated_metadata_fails() {
        let file_content = "syntax = 'proto3'; message TestProto { uint32 a = 1; }";
        let file_descriptor = mock_file_descriptor("test.proto", file_content);
        let message_descriptor = file_descriptor
            .message_by_full_name(".TestProto")
            .expect("Message descriptor must be created");

        let field_descriptor =
            message_descriptor.field_by_name("a").expect("Field descriptor must be created");

        let repeated_field_metadata = MessageMetadata::build_repeated_metadata(&field_descriptor);
        assert!(repeated_field_metadata.is_err());
        assert_eq!(
            repeated_field_metadata.unwrap_err(),
            MetadataBuildError::RepeatedTypeExpected("TestProto.a".to_owned())
        );
    }
}
