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

use protobuf::rt::WireType;

use crate::reflection::codec::api::protobuf::ProtobufReader;
use crate::reflection::message::api::protobuf::MessageRepeatedField;
use crate::reflection::message::api::protobuf::RepeatedField;
use crate::reflection::message::api::protobuf::SimpleRepeatedField;

use crate::reflection::metadata::api::protobuf::SingularTypeMetadata;
use crate::reflection::metadata::field_type::ProtobufSimpleType;
use crate::reflection::metadata::message::MessageMetadata;
use crate::reflection::metadata::simple::SimpleMetadata;

pub(crate) fn parse_repeated_field(
    reader: &mut ProtobufReader,
    wire_type: WireType,
    singular_metadata: &SingularTypeMetadata,
) -> anyhow::Result<RepeatedField> {
    match singular_metadata {
        SingularTypeMetadata::Simple(simple_metadata) => {
            if wire_type == WireType::LengthDelimited {
                // This corresponds to a packed primitive array
                Ok(RepeatedField::Simple(parse_packed_repeated_values(reader, simple_metadata)?))
            } else {
                Ok(RepeatedField::Simple(parse_simple_repeated_value(
                    reader,
                    simple_metadata,
                    wire_type,
                )?))
            }
        }
        SingularTypeMetadata::Message(message_metadata) => Ok(RepeatedField::Message(
            parse_message_repeated_value(reader, message_metadata, wire_type)?,
        )),
    }
}

fn parse_packed_repeated_values(
    reader: &mut ProtobufReader,
    simple_metadata: &SimpleMetadata,
) -> anyhow::Result<SimpleRepeatedField> {
    match simple_metadata.get_field_type() {
        ProtobufSimpleType::Double => {
            let mut values = Vec::new();
            reader.read_repeated_packed_double_into(&mut values)?;
            Ok(SimpleRepeatedField::Double(values))
        }
        ProtobufSimpleType::Float => {
            let mut values = Vec::new();
            reader.read_repeated_packed_float_into(&mut values)?;
            Ok(SimpleRepeatedField::Float(values))
        }
        ProtobufSimpleType::Int32 => {
            let mut values = Vec::new();
            reader.read_repeated_packed_int32_into(&mut values)?;
            Ok(SimpleRepeatedField::Int32(values))
        }
        ProtobufSimpleType::Int64 => {
            let mut values = Vec::new();
            reader.read_repeated_packed_int64_into(&mut values)?;
            Ok(SimpleRepeatedField::Int64(values))
        }
        ProtobufSimpleType::Uint32 => {
            let mut values = Vec::new();
            reader.read_repeated_packed_uint32_into(&mut values)?;
            Ok(SimpleRepeatedField::Uint32(values))
        }
        ProtobufSimpleType::Uint64 => {
            let mut values = Vec::new();
            reader.read_repeated_packed_uint64_into(&mut values)?;
            Ok(SimpleRepeatedField::Uint64(values))
        }
        ProtobufSimpleType::Sint32 => {
            let mut values = Vec::new();
            reader.read_repeated_packed_sint32_into(&mut values)?;
            Ok(SimpleRepeatedField::Sint32(values))
        }
        ProtobufSimpleType::Sint64 => {
            let mut values = Vec::new();
            reader.read_repeated_packed_sint64_into(&mut values)?;
            Ok(SimpleRepeatedField::Sint64(values))
        }
        ProtobufSimpleType::Fixed32 => {
            let mut values = Vec::new();
            reader.read_repeated_packed_fixed32_into(&mut values)?;
            Ok(SimpleRepeatedField::Fixed32(values))
        }
        ProtobufSimpleType::Fixed64 => {
            let mut values = Vec::new();
            reader.read_repeated_packed_fixed64_into(&mut values)?;
            Ok(SimpleRepeatedField::Fixed64(values))
        }
        ProtobufSimpleType::Sfixed32 => {
            let mut values = Vec::new();
            reader.read_repeated_packed_sfixed32_into(&mut values)?;
            Ok(SimpleRepeatedField::Sfixed32(values))
        }
        ProtobufSimpleType::Sfixed64 => {
            let mut values = Vec::new();
            reader.read_repeated_packed_sfixed64_into(&mut values)?;
            Ok(SimpleRepeatedField::Sfixed64(values))
        }
        ProtobufSimpleType::Bool => {
            let mut values = Vec::new();
            reader.read_repeated_packed_bool_into(&mut values)?;
            Ok(SimpleRepeatedField::Bool(values))
        }
        ProtobufSimpleType::Enum => {
            let mut values = Vec::new();
            reader.read_repeated_packed_int32_into(&mut values)?;
            Ok(SimpleRepeatedField::Enum(values))
        }
        ProtobufSimpleType::String | ProtobufSimpleType::Bytes => {
            anyhow::bail!("Unexpected length delimited type for array")
        }
    }
}

fn parse_simple_repeated_value(
    reader: &mut ProtobufReader,
    simple_metadata: &SimpleMetadata,
    wire_type: WireType,
) -> anyhow::Result<SimpleRepeatedField> {
    let simple_repeated_field = match simple_metadata.get_field_type() {
        ProtobufSimpleType::Double => {
            if wire_type != WireType::Fixed64 {
                anyhow::bail!(
                        "Unexpected wire type, expected fixed64 wire type for double, found {wire_type:?}"
                    )
            }
            SimpleRepeatedField::Double(vec![reader.read_double()?])
        }
        ProtobufSimpleType::Float => {
            if wire_type != WireType::Fixed32 {
                anyhow::bail!(
                        "Unexpected wire type, expected fixed32 wire type for float, found {wire_type:?}"
                    )
            }
            SimpleRepeatedField::Float(vec![reader.read_float()?])
        }
        ProtobufSimpleType::Int32 => {
            if wire_type != WireType::Varint {
                anyhow::bail!(
                        "Unexpected wire type, expected varint wire type for int32, found {wire_type:?}"
                    )
            }
            SimpleRepeatedField::Int32(vec![reader.read_int32()?])
        }
        ProtobufSimpleType::Int64 => {
            if wire_type != WireType::Varint {
                anyhow::bail!(
                        "Unexpected wire type, expected varint wire type for int64, found {wire_type:?}"
                    )
            }
            SimpleRepeatedField::Int64(vec![reader.read_int64()?])
        }
        ProtobufSimpleType::Uint32 => {
            if wire_type != WireType::Varint {
                anyhow::bail!(
                        "Unexpected wire type, expected varint wire type for uint32, found {wire_type:?}"
                    )
            }
            SimpleRepeatedField::Uint32(vec![reader.read_uint32()?])
        }
        ProtobufSimpleType::Uint64 => {
            if wire_type != WireType::Varint {
                anyhow::bail!(
                        "Unexpected wire type, expected varint wire type for uint64, found {wire_type:?}"
                    )
            }
            SimpleRepeatedField::Uint64(vec![reader.read_uint64()?])
        }
        ProtobufSimpleType::Sint32 => {
            if wire_type != WireType::Varint {
                anyhow::bail!(
                        "Unexpected wire type, expected varint wire type for sint32, found {wire_type:?}"
                    )
            }
            SimpleRepeatedField::Sint32(vec![reader.read_sint32()?])
        }
        ProtobufSimpleType::Sint64 => {
            if wire_type != WireType::Varint {
                anyhow::bail!(
                        "Unexpected wire type, expected varint wire type for sint64, found {wire_type:?}"
                    )
            }
            SimpleRepeatedField::Sint64(vec![reader.read_sint64()?])
        }
        ProtobufSimpleType::Fixed32 => {
            if wire_type != WireType::Fixed32 {
                anyhow::bail!(
                        "Unexpected wire type, expected fixed32 wire type for fixed32, found {wire_type:?}"
                    )
            }
            SimpleRepeatedField::Fixed32(vec![reader.read_fixed32()?])
        }
        ProtobufSimpleType::Fixed64 => {
            if wire_type != WireType::Fixed64 {
                anyhow::bail!(
                        "Unexpected wire type, expected fixed64 wire type for fixed64, found {wire_type:?}"
                    )
            }
            SimpleRepeatedField::Fixed64(vec![reader.read_fixed64()?])
        }
        ProtobufSimpleType::Sfixed32 => {
            if wire_type != WireType::Fixed32 {
                anyhow::bail!(
                        "Unexpected wire type, expected fixed32 wire type for sfixed32, found {wire_type:?}"
                    )
            }
            SimpleRepeatedField::Sfixed32(vec![reader.read_sfixed32()?])
        }
        ProtobufSimpleType::Sfixed64 => {
            if wire_type != WireType::Fixed64 {
                anyhow::bail!(
                        "Unexpected wire type, expected fixed64 wire type for sfixed64, found {wire_type:?}"
                    )
            }
            SimpleRepeatedField::Sfixed64(vec![reader.read_sfixed64()?])
        }
        ProtobufSimpleType::Bool => {
            if wire_type != WireType::Varint {
                anyhow::bail!(
                    "Unexpected wire type, expected varint wire type for bool, found {wire_type:?}"
                )
            }
            SimpleRepeatedField::Bool(vec![reader.read_bool()?])
        }
        ProtobufSimpleType::Enum => {
            if wire_type != WireType::Varint {
                anyhow::bail!(
                    "Unexpected wire type, expected varint wire type for enum, found {wire_type:?}"
                )
            }
            SimpleRepeatedField::Enum(vec![reader.read_int32()?])
        }
        ProtobufSimpleType::String => {
            if wire_type != WireType::LengthDelimited {
                anyhow::bail!(
                        "Unexpected wire type, expected length delimited wire type for string, found {wire_type:?}"
                    )
            }
            SimpleRepeatedField::String(vec![reader.read_string()?])
        }
        ProtobufSimpleType::Bytes => {
            if wire_type != WireType::LengthDelimited {
                anyhow::bail!(
                        "Unexpected wire type, expected length delimited wire type for bytes, found {wire_type:?}"
                    )
            }
            SimpleRepeatedField::Bytes(vec![reader.read_bytes()?])
        }
    };

    Ok(simple_repeated_field)
}

fn parse_message_repeated_value(
    reader: &mut ProtobufReader,
    message_metadata: &MessageMetadata,
    repeated_wire_type: WireType,
) -> anyhow::Result<MessageRepeatedField> {
    let mut message_repeated_field = MessageRepeatedField::new();
    message_repeated_field.add_message(
        message_metadata.parse_message_field_from_reader(reader, repeated_wire_type)?,
    )?;
    Ok(message_repeated_field)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reflection::codec::api::protobuf::ProtobufReader;
    use crate::reflection::message::api::protobuf::SimpleRepeatedField;
    use crate::reflection::metadata::field_type::ProtobufSimpleType;
    use crate::reflection::metadata::simple::SimpleMetadata;

    #[test]
    fn test_parse_packed_repeated_values_succeeds() {
        let cases = vec![
            (
                ProtobufSimpleType::Double,
                vec![
                    0x10, // Length 16
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f, // 1.0
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, // 2.0
                ],
                SimpleRepeatedField::Double(vec![1.0, 2.0]),
            ),
            (
                ProtobufSimpleType::Float,
                vec![
                    0x08, // Length 8
                    0x00, 0x00, 0x80, 0x3f, // 1.0
                    0x00, 0x00, 0x00, 0x40, // 2.0
                ],
                SimpleRepeatedField::Float(vec![1.0, 2.0]),
            ),
            (
                ProtobufSimpleType::Int32,
                vec![0x03, 0x01, 0x96, 0x01], // Length 3, [1, 150]
                SimpleRepeatedField::Int32(vec![1, 150]),
            ),
            (
                ProtobufSimpleType::Int64,
                vec![0x03, 0x01, 0x96, 0x01], // Length 3, [1, 150]
                SimpleRepeatedField::Int64(vec![1, 150]),
            ),
            (
                ProtobufSimpleType::Uint32,
                vec![0x03, 0x01, 0x96, 0x01], // Length 3, [1, 150]
                SimpleRepeatedField::Uint32(vec![1, 150]),
            ),
            (
                ProtobufSimpleType::Uint64,
                vec![0x03, 0x01, 0x96, 0x01], // Length 3, [1, 150]
                SimpleRepeatedField::Uint64(vec![1, 150]),
            ),
            (
                ProtobufSimpleType::Sint32,
                vec![0x02, 0x01, 0x02], // Length 2, [-1, 1] (ZigZag: 1, 2)
                SimpleRepeatedField::Sint32(vec![-1, 1]),
            ),
            (
                ProtobufSimpleType::Sint64,
                vec![0x02, 0x01, 0x02], // Length 2, [-1, 1] (ZigZag: 1, 2)
                SimpleRepeatedField::Sint64(vec![-1, 1]),
            ),
            (
                ProtobufSimpleType::Fixed32,
                vec![
                    0x08, // Length 8
                    0x01, 0x00, 0x00, 0x00, // 1
                    0x02, 0x00, 0x00, 0x00, // 2
                ],
                SimpleRepeatedField::Fixed32(vec![1, 2]),
            ),
            (
                ProtobufSimpleType::Fixed64,
                vec![
                    0x10, // Length 16
                    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 1
                    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 2
                ],
                SimpleRepeatedField::Fixed64(vec![1, 2]),
            ),
            (
                ProtobufSimpleType::Sfixed32,
                vec![
                    0x08, // Length 8
                    0xff, 0xff, 0xff, 0xff, // -1
                    0x01, 0x00, 0x00, 0x00, // 1
                ],
                SimpleRepeatedField::Sfixed32(vec![-1, 1]),
            ),
            (
                ProtobufSimpleType::Sfixed64,
                vec![
                    0x10, // Length 16
                    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // -1
                    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 1
                ],
                SimpleRepeatedField::Sfixed64(vec![-1, 1]),
            ),
            (
                ProtobufSimpleType::Bool,
                vec![0x02, 0x01, 0x00], // Length 2, [true, false]
                SimpleRepeatedField::Bool(vec![true, false]),
            ),
            (
                ProtobufSimpleType::Enum,
                vec![0x02, 0x01, 0x02], // Length 2, [1, 2]
                SimpleRepeatedField::Enum(vec![1, 2]),
            ),
        ];

        for (field_type, bytes, expected) in cases {
            let metadata = SimpleMetadata::new(field_type);
            let mut reader = ProtobufReader::new(&bytes);
            let result = parse_packed_repeated_values(&mut reader, &metadata);
            assert!(result.is_ok(), "Failed to parse {:?}: {:?}", field_type, result.err());
            assert_eq!(result.unwrap(), expected);
        }
    }

    #[test]
    fn test_parse_packed_repeated_values_fails_for_non_packable() {
        let cases = vec![ProtobufSimpleType::String, ProtobufSimpleType::Bytes];

        for field_type in cases {
            let metadata = SimpleMetadata::new(field_type);
            let mut reader = ProtobufReader::new(&[]);
            let result = parse_packed_repeated_values(&mut reader, &metadata);
            assert!(result.is_err(), "Expected error for type {:?}", field_type);
        }
    }

    #[test]
    fn test_parse_simple_repeated_value_succeeds() {
        let cases = vec![
            (
                ProtobufSimpleType::Double,
                WireType::Fixed64,
                vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f], // 1.0
                SimpleRepeatedField::Double(vec![1.0]),
            ),
            (
                ProtobufSimpleType::Float,
                WireType::Fixed32,
                vec![0x00, 0x00, 0x80, 0x3f], // 1.0
                SimpleRepeatedField::Float(vec![1.0]),
            ),
            (
                ProtobufSimpleType::Int32,
                WireType::Varint,
                vec![0x96, 0x01], // 150
                SimpleRepeatedField::Int32(vec![150]),
            ),
            (
                ProtobufSimpleType::Int64,
                WireType::Varint,
                vec![0x96, 0x01], // 150
                SimpleRepeatedField::Int64(vec![150]),
            ),
            (
                ProtobufSimpleType::Uint32,
                WireType::Varint,
                vec![0x96, 0x01], // 150
                SimpleRepeatedField::Uint32(vec![150]),
            ),
            (
                ProtobufSimpleType::Uint64,
                WireType::Varint,
                vec![0x96, 0x01], // 150
                SimpleRepeatedField::Uint64(vec![150]),
            ),
            (
                ProtobufSimpleType::Sint32,
                WireType::Varint,
                vec![0x01], // -1 (ZigZag encoded is 1)
                SimpleRepeatedField::Sint32(vec![-1]),
            ),
            (
                ProtobufSimpleType::Sint64,
                WireType::Varint,
                vec![0x01], // -1 (ZigZag encoded is 1)
                SimpleRepeatedField::Sint64(vec![-1]),
            ),
            (
                ProtobufSimpleType::Fixed32,
                WireType::Fixed32,
                vec![0x64, 0x00, 0x00, 0x00], // 100
                SimpleRepeatedField::Fixed32(vec![100]),
            ),
            (
                ProtobufSimpleType::Fixed64,
                WireType::Fixed64,
                vec![0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // 100
                SimpleRepeatedField::Fixed64(vec![100]),
            ),
            (
                ProtobufSimpleType::Sfixed32,
                WireType::Fixed32,
                vec![0xff, 0xff, 0xff, 0xff], // -1
                SimpleRepeatedField::Sfixed32(vec![-1]),
            ),
            (
                ProtobufSimpleType::Sfixed64,
                WireType::Fixed64,
                vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff], // -1
                SimpleRepeatedField::Sfixed64(vec![-1]),
            ),
            (
                ProtobufSimpleType::Bool,
                WireType::Varint,
                vec![0x01], // true
                SimpleRepeatedField::Bool(vec![true]),
            ),
            (
                ProtobufSimpleType::Enum,
                WireType::Varint,
                vec![0x01], // 1
                SimpleRepeatedField::Enum(vec![1]),
            ),
            (
                ProtobufSimpleType::String,
                WireType::LengthDelimited,
                vec![0x03, 0x66, 0x6f, 0x6f], // "foo"
                SimpleRepeatedField::String(vec!["foo".to_string()]),
            ),
            (
                ProtobufSimpleType::Bytes,
                WireType::LengthDelimited,
                vec![0x02, 0x01, 0x02], // [1, 2]
                SimpleRepeatedField::Bytes(vec![vec![0x01, 0x02]]),
            ),
        ];

        for (field_type, wire_type, bytes, expected) in cases {
            let metadata = SimpleMetadata::new(field_type);
            let mut reader = ProtobufReader::new(&bytes);
            let result = parse_simple_repeated_value(&mut reader, &metadata, wire_type);
            assert!(result.is_ok(), "Failed to parse {:?}: {:?}", field_type, result.err());
            assert_eq!(result.unwrap(), expected);
        }
    }

    #[test]
    fn test_parse_simple_repeated_value_wrong_wire_type_fails() {
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
            let result = parse_simple_repeated_value(&mut reader, &metadata, wire_type);
            assert!(
                result.is_err(),
                "Expected error for type {:?} with wire type {:?}",
                field_type,
                wire_type
            );
        }
    }
}
