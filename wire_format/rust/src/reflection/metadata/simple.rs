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

use protobuf::descriptor::field_descriptor_proto::Type;
use protobuf::rt::WireType;

use crate::reflection::codec::api::protobuf::ProtobufReader;
use crate::reflection::message::api::protobuf::SimpleField;
use crate::reflection::metadata::error::MetadataBuildError;
use crate::reflection::metadata::field_type::ProtobufSimpleType;

/// Simple field type metadata (no nested types)
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SimpleMetadata {
    // How to interpret the field from the input buffer
    field_type: ProtobufSimpleType,
}

impl SimpleMetadata {
    pub(crate) fn new(field_type: ProtobufSimpleType) -> Self {
        Self { field_type }
    }

    pub(crate) fn build(field_type: Type) -> Result<Self, MetadataBuildError> {
        let field_type = ProtobufSimpleType::try_from(field_type)?;
        Ok(Self::new(field_type))
    }

    /// Get the type of the simple field
    pub fn get_field_type(&self) -> ProtobufSimpleType {
        self.field_type
    }

    pub(crate) fn parse_from_reader(
        &self,
        reader: &mut ProtobufReader,
        actual_wire_type: WireType,
    ) -> anyhow::Result<SimpleField> {
        match self.field_type {
            ProtobufSimpleType::Double => {
                if actual_wire_type != WireType::Fixed64 {
                    anyhow::bail!(
                        "Unexpected wire type, expected fixed64 wire type for double, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Double(reader.read_double()?))
            }
            ProtobufSimpleType::Float => {
                if actual_wire_type != WireType::Fixed32 {
                    anyhow::bail!(
                        "Unexpected wire type, expected fixed32 wire type for float, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Float(reader.read_float()?))
            }
            ProtobufSimpleType::Int32 => {
                if actual_wire_type != WireType::Varint {
                    anyhow::bail!(
                        "Unexpected wire type, expected varint wire type for int32, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Int32(reader.read_int32()?))
            }
            ProtobufSimpleType::Int64 => {
                if actual_wire_type != WireType::Varint {
                    anyhow::bail!(
                        "Unexpected wire type, expected varint wire type for int64, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Int64(reader.read_int64()?))
            }
            ProtobufSimpleType::Uint32 => {
                if actual_wire_type != WireType::Varint {
                    anyhow::bail!(
                        "Unexpected wire type, expected varint wire type for uint32, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Uint32(reader.read_uint32()?))
            }
            ProtobufSimpleType::Uint64 => {
                if actual_wire_type != WireType::Varint {
                    anyhow::bail!(
                        "Unexpected wire type, expected varint wire type for uint64, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Uint64(reader.read_uint64()?))
            }
            ProtobufSimpleType::Sint32 => {
                if actual_wire_type != WireType::Varint {
                    anyhow::bail!(
                        "Unexpected wire type, expected varint wire type for sint32, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Sint32(reader.read_sint32()?))
            }
            ProtobufSimpleType::Sint64 => {
                if actual_wire_type != WireType::Varint {
                    anyhow::bail!(
                        "Unexpected wire type, expected varint wire type for sint64, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Sint64(reader.read_sint64()?))
            }
            ProtobufSimpleType::Fixed32 => {
                if actual_wire_type != WireType::Fixed32 {
                    anyhow::bail!(
                        "Unexpected wire type, expected fixed32 wire type for fixed32, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Fixed32(reader.read_fixed32()?))
            }
            ProtobufSimpleType::Fixed64 => {
                if actual_wire_type != WireType::Fixed64 {
                    anyhow::bail!(
                        "Unexpected wire type, expected fixed64 wire type for fixed64, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Fixed64(reader.read_fixed64()?))
            }
            ProtobufSimpleType::Sfixed32 => {
                if actual_wire_type != WireType::Fixed32 {
                    anyhow::bail!(
                        "Unexpected wire type, expected fixed32 wire type for sfixed32, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Sfixed32(reader.read_sfixed32()?))
            }
            ProtobufSimpleType::Sfixed64 => {
                if actual_wire_type != WireType::Fixed64 {
                    anyhow::bail!(
                        "Unexpected wire type, expected fixed64 wire type for sfixed64, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Sfixed64(reader.read_sfixed64()?))
            }
            ProtobufSimpleType::Bool => {
                if actual_wire_type != WireType::Varint {
                    anyhow::bail!(
                        "Unexpected wire type, expected varint wire type for bool, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Bool(reader.read_bool()?))
            }
            ProtobufSimpleType::Enum => {
                if actual_wire_type != WireType::Varint {
                    anyhow::bail!(
                        "Unexpected wire type, expected varint wire type for enum, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Enum(reader.read_int32()?))
            }
            ProtobufSimpleType::String => {
                if actual_wire_type != WireType::LengthDelimited {
                    anyhow::bail!(
                        "Unexpected wire type, expected length delimited wire type for string, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::String(reader.read_string()?))
            }
            ProtobufSimpleType::Bytes => {
                if actual_wire_type != WireType::LengthDelimited {
                    anyhow::bail!(
                        "Unexpected wire type, expected length delimited wire type for bytes, found {actual_wire_type:?}"
                    )
                }
                Ok(SimpleField::Bytes(reader.read_bytes()?))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reflection::message::api::protobuf::SimpleField;
    use protobuf::rt::WireType;

    #[test]
    fn test_parse_from_reader_succeeds() {
        let cases = vec![
            (
                ProtobufSimpleType::Double,
                WireType::Fixed64,
                vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f], // 1.0
                SimpleField::Double(1.0),
            ),
            (
                ProtobufSimpleType::Float,
                WireType::Fixed32,
                vec![0x00, 0x00, 0x80, 0x3f], // 1.0
                SimpleField::Float(1.0),
            ),
            (
                ProtobufSimpleType::Int32,
                WireType::Varint,
                vec![0x96, 0x01], // 150
                SimpleField::Int32(150),
            ),
            (
                ProtobufSimpleType::Int64,
                WireType::Varint,
                vec![0x96, 0x01], // 150
                SimpleField::Int64(150),
            ),
            (
                ProtobufSimpleType::Uint32,
                WireType::Varint,
                vec![0x96, 0x01], // 150
                SimpleField::Uint32(150),
            ),
            (
                ProtobufSimpleType::Uint64,
                WireType::Varint,
                vec![0x96, 0x01], // 150
                SimpleField::Uint64(150),
            ),
            (
                ProtobufSimpleType::Sint32,
                WireType::Varint,
                vec![0x01], // -1 (ZigZag encoded is 1)
                SimpleField::Sint32(-1),
            ),
            (
                ProtobufSimpleType::Sint64,
                WireType::Varint,
                vec![0x01], // -1 (ZigZag encoded is 1)
                SimpleField::Sint64(-1),
            ),
            (
                ProtobufSimpleType::Fixed32,
                WireType::Fixed32,
                vec![0x64, 0x00, 0x00, 0x00], // 100
                SimpleField::Fixed32(100),
            ),
            (
                ProtobufSimpleType::Fixed64,
                WireType::Fixed64,
                vec![0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // 100
                SimpleField::Fixed64(100),
            ),
            (
                ProtobufSimpleType::Sfixed32,
                WireType::Fixed32,
                vec![0xff, 0xff, 0xff, 0xff], // -1
                SimpleField::Sfixed32(-1),
            ),
            (
                ProtobufSimpleType::Sfixed64,
                WireType::Fixed64,
                vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff], // -1
                SimpleField::Sfixed64(-1),
            ),
            (
                ProtobufSimpleType::Bool,
                WireType::Varint,
                vec![0x01], // true
                SimpleField::Bool(true),
            ),
            (
                ProtobufSimpleType::Enum,
                WireType::Varint,
                vec![0x01], // 1
                SimpleField::Enum(1),
            ),
            (
                ProtobufSimpleType::String,
                WireType::LengthDelimited,
                vec![0x03, 0x66, 0x6f, 0x6f], // "foo"
                SimpleField::String("foo".to_string()),
            ),
            (
                ProtobufSimpleType::Bytes,
                WireType::LengthDelimited,
                vec![0x02, 0x01, 0x02], // [1, 2]
                SimpleField::Bytes(vec![0x01, 0x02]),
            ),
        ];

        for (field_type, wire_type, bytes, expected) in cases {
            let metadata = SimpleMetadata::new(field_type);
            let mut reader = ProtobufReader::new(&bytes);
            let result = metadata.parse_from_reader(&mut reader, wire_type);
            assert!(result.is_ok(), "Failed to parse {:?}: {:?}", field_type, result.err());
            assert_eq!(result.unwrap(), expected);
        }
    }

    #[test]
    fn test_parse_from_reader_wrong_wire_type_fails() {
        let cases = vec![
            (ProtobufSimpleType::Double, WireType::Varint),
            (ProtobufSimpleType::Float, WireType::Varint),
            (ProtobufSimpleType::Int32, WireType::Fixed32),
            (ProtobufSimpleType::Int64, WireType::Fixed64),
            (ProtobufSimpleType::Uint32, WireType::Fixed32),
            (ProtobufSimpleType::Uint64, WireType::Fixed64),
            (ProtobufSimpleType::Sint32, WireType::Fixed32),
            (ProtobufSimpleType::Sint64, WireType::Fixed64),
            (ProtobufSimpleType::Fixed32, WireType::Varint),
            (ProtobufSimpleType::Fixed64, WireType::Varint),
            (ProtobufSimpleType::Sfixed32, WireType::Varint),
            (ProtobufSimpleType::Sfixed64, WireType::Varint),
            (ProtobufSimpleType::Bool, WireType::Fixed32),
            (ProtobufSimpleType::Enum, WireType::Fixed32),
            (ProtobufSimpleType::String, WireType::Varint),
            (ProtobufSimpleType::Bytes, WireType::Varint),
        ];

        for (field_type, wire_type) in cases {
            let metadata = SimpleMetadata::new(field_type);
            let mut reader = ProtobufReader::new(&[]);
            let result = metadata.parse_from_reader(&mut reader, wire_type);
            assert!(
                result.is_err(),
                "Expected error for type {:?} with wire type {:?}",
                field_type,
                wire_type
            );
        }
    }
}
