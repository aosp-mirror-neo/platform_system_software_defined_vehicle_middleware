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
