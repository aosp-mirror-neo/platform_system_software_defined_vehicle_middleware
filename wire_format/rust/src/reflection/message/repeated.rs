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

use crate::reflection::codec::api::protobuf::IProtobufWriter;
use crate::reflection::codec::api::protobuf::RpcWriter;
use crate::reflection::message::dynamic_message::DynamicMessage;
use crate::reflection::metadata::api::protobuf::ProtobufSimpleType;

/// Dynamic protobuf repeated field
#[derive(Debug, Clone, PartialEq)]
pub enum RepeatedField {
    /// Simple repeated field
    Simple(SimpleRepeatedField),
    /// Message repeated field
    Message(MessageRepeatedField),
}

impl RepeatedField {
    pub(crate) fn get_type_for_error_msg(&self) -> String {
        match self {
            RepeatedField::Simple(simple_repeated_field) => {
                format!("Simple: {:?}", simple_repeated_field.get_type())
            }
            RepeatedField::Message(message_repeated_field) => {
                format!("Message: {:?}", message_repeated_field.get_message_name())
            }
        }
    }

    /// Returns the length of the repeated field
    pub fn len(&self) -> usize {
        match self {
            RepeatedField::Simple(simple_repeated_field) => simple_repeated_field.len(),
            RepeatedField::Message(message_repeated_field) => message_repeated_field.len(),
        }
    }

    /// Returns true if the repeated field is empty
    pub fn is_empty(&self) -> bool {
        match self {
            RepeatedField::Simple(simple_repeated_field) => simple_repeated_field.len() == 0,
            RepeatedField::Message(message_repeated_field) => message_repeated_field.is_empty(),
        }
    }

    pub(crate) fn merge(&mut self, other_repeated_field: RepeatedField) -> anyhow::Result<()> {
        match (self, other_repeated_field) {
            (
                RepeatedField::Simple(self_simple_repeated_field),
                RepeatedField::Simple(other_simple_repeated_field),
            ) => self_simple_repeated_field.merge(other_simple_repeated_field),
            (
                RepeatedField::Message(self_message_repeated_field),
                RepeatedField::Message(other_message_repeated_field),
            ) => self_message_repeated_field.merge(other_message_repeated_field),
            (self_repeated_field, other_repeated_field) => {
                anyhow::bail!(
                    "Attempting to merge repeated field with different types: Self is {:?}, other is {:?}",
                    self_repeated_field.get_type_for_error_msg(),
                    other_repeated_field.get_type_for_error_msg()
                )
            }
        }
    }

    pub(crate) fn write_repeated_field(
        &self,
        field_number: u32,
        writer: &mut RpcWriter,
    ) -> anyhow::Result<()> {
        match self {
            RepeatedField::Simple(simple_repeated_field) => {
                simple_repeated_field.write_simple_repeated_field(field_number, writer)?;
            }
            RepeatedField::Message(message_repeated_field) => {
                for message in message_repeated_field.get_messages() {
                    message.write_message_field(field_number, writer)?;
                }
            }
        }
        Ok(())
    }
}

/// Dynamic protobuf simple repeated field
#[derive(Debug, Clone, PartialEq)]
pub enum SimpleRepeatedField {
    /// Double
    Double(Vec<f64>),
    /// Float
    Float(Vec<f32>),
    /// Int32
    Int32(Vec<i32>),
    /// Int64
    Int64(Vec<i64>),
    /// Uint32
    Uint32(Vec<u32>),
    /// Uint64
    Uint64(Vec<u64>),
    /// Sint32
    Sint32(Vec<i32>),
    /// Sint64
    Sint64(Vec<i64>),
    /// Fixed32
    Fixed32(Vec<u32>),
    /// Fixed64
    Fixed64(Vec<u64>),
    /// Sfixed32
    Sfixed32(Vec<i32>),
    /// Sfixed64
    Sfixed64(Vec<i64>),
    /// Bool
    Bool(Vec<bool>),
    /// String
    String(Vec<String>),
    /// Bytes
    Bytes(Vec<Vec<u8>>),
    /// Enum
    Enum(Vec<i32>),
}

/// Dynamic protobuf message repeated field
#[derive(Debug, Clone, PartialEq)]
pub struct MessageRepeatedField {
    messages: Vec<DynamicMessage>,
}

impl MessageRepeatedField {
    fn new() -> Self {
        Self { messages: Vec::new() }
    }

    fn get_message_name(&self) -> String {
        self.messages
            .first()
            .map(|m| m.get_full_name().to_owned())
            .unwrap_or_else(|| "Unknown".to_owned())
    }

    /// Merges a message repeated field into this one. The merging follows standard protobuf merging rules.
    pub fn merge(&mut self, other: MessageRepeatedField) -> anyhow::Result<()> {
        let self_type = self.messages.first().map(|m| m.get_full_name());
        let other_type = other.messages.first().map(|m| m.get_full_name());

        if let (Some(self_type), Some(other_type)) = (self_type, other_type) {
            if self_type.ne(other_type) {
                anyhow::bail!(
                    "Trying to merge repeated message field with type {} into repeated field with existing type {}",
                    other_type,
                    self_type
                );
            }
        }
        self.messages.extend(other.messages);
        Ok(())
    }

    /// Gets the messages in the repeated message field
    pub fn get_messages(&self) -> &[DynamicMessage] {
        &self.messages
    }

    /// Adds a message to the repeated message field. This is the only way to add a message to a repeated
    /// message field.
    pub fn add_message(&mut self, message: DynamicMessage) -> anyhow::Result<()> {
        if let Some(existing_message) = self.messages.first() {
            if !existing_message.get_full_name().eq(message.get_full_name()) {
                anyhow::bail!(
                    "Trying to insert value with type {} into repeated field with existing type {}",
                    message.get_full_name(),
                    existing_message.get_full_name()
                );
            }
        }
        self.messages.push(message);
        Ok(())
    }

    /// Returns the number of elements in the repeated field
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Returns true if the repeated field is empty
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

impl SimpleRepeatedField {
    fn len(&self) -> usize {
        match self {
            SimpleRepeatedField::Double(double_vec) => double_vec.len(),
            SimpleRepeatedField::Float(float_vec) => float_vec.len(),
            SimpleRepeatedField::Int32(int32_vec) => int32_vec.len(),
            SimpleRepeatedField::Int64(int64_vec) => int64_vec.len(),
            SimpleRepeatedField::Uint32(uint32_vec) => uint32_vec.len(),
            SimpleRepeatedField::Uint64(uint64_vec) => uint64_vec.len(),
            SimpleRepeatedField::Sint32(sint32_vec) => sint32_vec.len(),
            SimpleRepeatedField::Sint64(sint64_vec) => sint64_vec.len(),
            SimpleRepeatedField::Fixed32(fixed32_vec) => fixed32_vec.len(),
            SimpleRepeatedField::Fixed64(fixed64_vec) => fixed64_vec.len(),
            SimpleRepeatedField::Sfixed32(sfixed32_vec) => sfixed32_vec.len(),
            SimpleRepeatedField::Sfixed64(sfixed64_vec) => sfixed64_vec.len(),
            SimpleRepeatedField::Bool(bool_vec) => bool_vec.len(),
            SimpleRepeatedField::String(string_vec) => string_vec.len(),
            SimpleRepeatedField::Bytes(bytes_vec) => bytes_vec.len(),
            SimpleRepeatedField::Enum(enum_vec) => enum_vec.len(),
        }
    }

    fn get_type(&self) -> ProtobufSimpleType {
        match self {
            SimpleRepeatedField::Double(_) => ProtobufSimpleType::Double,
            SimpleRepeatedField::Float(_) => ProtobufSimpleType::Float,
            SimpleRepeatedField::Int32(_) => ProtobufSimpleType::Int32,
            SimpleRepeatedField::Int64(_) => ProtobufSimpleType::Int64,
            SimpleRepeatedField::Uint32(_) => ProtobufSimpleType::Uint32,
            SimpleRepeatedField::Uint64(_) => ProtobufSimpleType::Uint64,
            SimpleRepeatedField::Sint32(_) => ProtobufSimpleType::Sint32,
            SimpleRepeatedField::Sint64(_) => ProtobufSimpleType::Sint64,
            SimpleRepeatedField::Fixed32(_) => ProtobufSimpleType::Fixed32,
            SimpleRepeatedField::Fixed64(_) => ProtobufSimpleType::Fixed64,
            SimpleRepeatedField::Sfixed32(_) => ProtobufSimpleType::Sfixed32,
            SimpleRepeatedField::Sfixed64(_) => ProtobufSimpleType::Sfixed64,
            SimpleRepeatedField::Bool(_) => ProtobufSimpleType::Bool,
            SimpleRepeatedField::String(_) => ProtobufSimpleType::String,
            SimpleRepeatedField::Bytes(_) => ProtobufSimpleType::Bytes,
            SimpleRepeatedField::Enum(_) => ProtobufSimpleType::Enum,
        }
    }

    // Merging a repeated field is a concatenation of repeated values
    pub(crate) fn merge(
        &mut self,
        other_simple_repeated_field: SimpleRepeatedField,
    ) -> anyhow::Result<()> {
        match (self, other_simple_repeated_field) {
            (
                SimpleRepeatedField::Double(self_double_vec),
                SimpleRepeatedField::Double(other_double_vec),
            ) => {
                self_double_vec.extend(other_double_vec);
            }
            (
                SimpleRepeatedField::Float(self_float_vec),
                SimpleRepeatedField::Float(other_float_vec),
            ) => {
                self_float_vec.extend(other_float_vec);
            }
            (
                SimpleRepeatedField::Int32(self_int32_vec),
                SimpleRepeatedField::Int32(other_int32_vec),
            ) => {
                self_int32_vec.extend(other_int32_vec);
            }
            (
                SimpleRepeatedField::Int64(self_int64_vec),
                SimpleRepeatedField::Int64(other_int64_vec),
            ) => {
                self_int64_vec.extend(other_int64_vec);
            }
            (
                SimpleRepeatedField::Uint32(self_uint32_vec),
                SimpleRepeatedField::Uint32(other_uint32_vec),
            ) => {
                self_uint32_vec.extend(other_uint32_vec);
            }
            (
                SimpleRepeatedField::Uint64(self_uint64_vec),
                SimpleRepeatedField::Uint64(other_uint64_vec),
            ) => {
                self_uint64_vec.extend(other_uint64_vec);
            }
            (
                SimpleRepeatedField::Sint32(self_sint32_vec),
                SimpleRepeatedField::Sint32(other_sint32_vec),
            ) => {
                self_sint32_vec.extend(other_sint32_vec);
            }
            (
                SimpleRepeatedField::Sint64(self_sint64_vec),
                SimpleRepeatedField::Sint64(other_sint64_vec),
            ) => {
                self_sint64_vec.extend(other_sint64_vec);
            }
            (
                SimpleRepeatedField::Fixed32(self_fixed32_vec),
                SimpleRepeatedField::Fixed32(other_fixed32_vec),
            ) => {
                self_fixed32_vec.extend(other_fixed32_vec);
            }
            (
                SimpleRepeatedField::Fixed64(self_fixed64_vec),
                SimpleRepeatedField::Fixed64(other_fixed64_vec),
            ) => {
                self_fixed64_vec.extend(other_fixed64_vec);
            }
            (
                SimpleRepeatedField::Sfixed32(self_sfixed32_vec),
                SimpleRepeatedField::Sfixed32(other_sfixed32_vec),
            ) => {
                self_sfixed32_vec.extend(other_sfixed32_vec);
            }
            (
                SimpleRepeatedField::Sfixed64(self_sfixed64_vec),
                SimpleRepeatedField::Sfixed64(other_sfixed64_vec),
            ) => {
                self_sfixed64_vec.extend(other_sfixed64_vec);
            }
            (
                SimpleRepeatedField::Bool(self_bool_vec),
                SimpleRepeatedField::Bool(other_bool_vec),
            ) => {
                self_bool_vec.extend(other_bool_vec);
            }
            (
                SimpleRepeatedField::String(self_string_vec),
                SimpleRepeatedField::String(other_string_vec),
            ) => {
                self_string_vec.extend(other_string_vec);
            }
            (
                SimpleRepeatedField::Bytes(self_bytes_vec),
                SimpleRepeatedField::Bytes(other_bytes_vec),
            ) => {
                self_bytes_vec.extend(other_bytes_vec);
            }
            (
                SimpleRepeatedField::Enum(self_enum_vec),
                SimpleRepeatedField::Enum(other_enum_vec),
            ) => {
                self_enum_vec.extend(other_enum_vec);
            }
            (self_repeated_field, other_repeated_field) => {
                anyhow::bail!(
                    "Attempting to merge simple repeated values with different types: Self is {:?}, other is {:?}",
                    self_repeated_field.get_type(),
                    other_repeated_field.get_type()
                )
            }
        }
        Ok(())
    }

    pub(crate) fn write_simple_repeated_field(
        &self,
        field_number: u32,
        writer: &mut RpcWriter,
    ) -> anyhow::Result<()> {
        match self {
            SimpleRepeatedField::Double(double_array) => {
                if double_array.is_empty() {
                    return Ok(());
                }

                writer.write_tag(field_number, WireType::LengthDelimited)?;
                writer.write_raw_varint32((double_array.len() << 3) as u32)?;
                for double in double_array {
                    writer.write_double_no_tag(*double)?;
                }
                Ok(())
            }
            SimpleRepeatedField::Float(float_array) => {
                if float_array.is_empty() {
                    return Ok(());
                }

                writer.write_tag(field_number, WireType::LengthDelimited)?;
                writer.write_raw_varint32((float_array.len() << 2) as u32)?;
                for float in float_array {
                    writer.write_float_no_tag(*float)?;
                }
                Ok(())
            }
            SimpleRepeatedField::Int32(int32_array) => {
                if int32_array.is_empty() {
                    return Ok(());
                }

                writer.write_tag(field_number, WireType::LengthDelimited)?;
                writer.start_length_delimited_message()?;
                for int32 in int32_array {
                    writer.write_int32_no_tag(*int32)?;
                }
                writer.end_length_delimited_message()
            }
            SimpleRepeatedField::Int64(int64_array) => {
                if int64_array.is_empty() {
                    return Ok(());
                }

                writer.write_tag(field_number, WireType::LengthDelimited)?;
                writer.start_length_delimited_message()?;
                for int64 in int64_array {
                    writer.write_int64_no_tag(*int64)?;
                }
                writer.end_length_delimited_message()
            }
            SimpleRepeatedField::Uint32(uint32_array) => {
                if uint32_array.is_empty() {
                    return Ok(());
                }

                writer.write_tag(field_number, WireType::LengthDelimited)?;
                writer.start_length_delimited_message()?;
                for uint32 in uint32_array {
                    writer.write_uint32_no_tag(*uint32)?;
                }
                writer.end_length_delimited_message()
            }
            SimpleRepeatedField::Uint64(uint64_array) => {
                if uint64_array.is_empty() {
                    return Ok(());
                }

                writer.write_tag(field_number, WireType::LengthDelimited)?;
                writer.start_length_delimited_message()?;
                for uint64 in uint64_array {
                    writer.write_uint64_no_tag(*uint64)?;
                }
                writer.end_length_delimited_message()
            }
            SimpleRepeatedField::Sint32(sint32_array) => {
                if sint32_array.is_empty() {
                    return Ok(());
                }

                writer.write_tag(field_number, WireType::LengthDelimited)?;
                writer.start_length_delimited_message()?;
                for sint32 in sint32_array {
                    writer.write_sint32_no_tag(*sint32)?;
                }
                writer.end_length_delimited_message()
            }
            SimpleRepeatedField::Sint64(sint64_array) => {
                if sint64_array.is_empty() {
                    return Ok(());
                }

                writer.write_tag(field_number, WireType::LengthDelimited)?;
                writer.start_length_delimited_message()?;
                for sint64 in sint64_array {
                    writer.write_sint64_no_tag(*sint64)?;
                }
                writer.end_length_delimited_message()
            }
            SimpleRepeatedField::Fixed32(fixed32_array) => {
                if fixed32_array.is_empty() {
                    return Ok(());
                }

                writer.write_tag(field_number, WireType::LengthDelimited)?;
                writer.write_raw_varint32((fixed32_array.len() << 2) as u32)?;
                for fixed32 in fixed32_array {
                    writer.write_fixed32_no_tag(*fixed32)?;
                }
                Ok(())
            }
            SimpleRepeatedField::Fixed64(fixed64_array) => {
                if fixed64_array.is_empty() {
                    return Ok(());
                }

                writer.write_tag(field_number, WireType::LengthDelimited)?;
                writer.write_raw_varint32((fixed64_array.len() << 3) as u32)?;
                for fixed64 in fixed64_array {
                    writer.write_fixed64_no_tag(*fixed64)?;
                }
                Ok(())
            }
            SimpleRepeatedField::Sfixed32(sfixed32_array) => {
                if sfixed32_array.is_empty() {
                    return Ok(());
                }

                writer.write_tag(field_number, WireType::LengthDelimited)?;
                writer.write_raw_varint32((sfixed32_array.len() << 2) as u32)?;
                for sfixed32 in sfixed32_array {
                    writer.write_sfixed32_no_tag(*sfixed32)?;
                }
                Ok(())
            }
            SimpleRepeatedField::Sfixed64(sfixed64_array) => {
                if sfixed64_array.is_empty() {
                    return Ok(());
                }
                writer.write_tag(field_number, WireType::LengthDelimited)?;
                writer.write_raw_varint32((sfixed64_array.len() << 3) as u32)?;
                for sfixed64 in sfixed64_array {
                    writer.write_sfixed64_no_tag(*sfixed64)?;
                }
                Ok(())
            }
            SimpleRepeatedField::Bool(bool_array) => {
                if bool_array.is_empty() {
                    return Ok(());
                }

                writer.write_tag(field_number, WireType::LengthDelimited)?;
                writer.write_raw_varint32(bool_array.len() as u32)?;
                for bool_value in bool_array {
                    writer.write_bool_no_tag(*bool_value)?;
                }
                Ok(())
            }
            SimpleRepeatedField::String(string_array) => {
                if string_array.is_empty() {
                    return Ok(());
                }

                for string in string_array {
                    writer.write_string(field_number, string)?;
                }
                Ok(())
            }
            SimpleRepeatedField::Bytes(bytes_array) => {
                if bytes_array.is_empty() {
                    return Ok(());
                }

                for bytes in bytes_array {
                    writer.write_bytes(field_number, bytes)?;
                }
                Ok(())
            }
            SimpleRepeatedField::Enum(enum_array) => {
                if enum_array.is_empty() {
                    return Ok(());
                }
                writer.write_tag(field_number, WireType::LengthDelimited)?;
                writer.start_length_delimited_message()?;
                for enum_value in enum_array {
                    writer.write_enum_no_tag(*enum_value)?;
                }
                writer.end_length_delimited_message()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reflection::codec::api::protobuf::RpcWriter;

    #[test]
    fn test_message_repeated_field_new() {
        let field = MessageRepeatedField::new();
        assert_eq!(field.len(), 0);
        assert!(field.get_messages().is_empty());
    }

    #[test]
    fn test_message_repeated_field_add_message_succeeds() {
        let mut field = MessageRepeatedField::new();
        let msg1 = DynamicMessage::new("test.Message".to_string());
        let msg2 = DynamicMessage::new("test.Message".to_string());

        assert!(field.add_message(msg1.clone()).is_ok());
        assert_eq!(field.len(), 1);
        assert_eq!(field.get_messages(), &vec![msg1.clone()]);

        assert!(field.add_message(msg2.clone()).is_ok());
        assert_eq!(field.len(), 2);
        assert_eq!(field.get_messages(), &vec![msg1, msg2]);
    }

    #[test]
    fn test_message_repeated_field_add_message_type_mismatch_fails() {
        let mut field = MessageRepeatedField::new();
        let msg1 = DynamicMessage::new("test.Message1".to_string());
        let msg2 = DynamicMessage::new("test.Message2".to_string());

        assert!(field.add_message(msg1).is_ok());
        let result = field.add_message(msg2);
        assert!(result.is_err());
    }

    #[test]
    fn test_message_repeated_field_merge_type_mismatch_fails() {
        let mut field1 = MessageRepeatedField::new();
        field1.add_message(DynamicMessage::new("test.Message1".to_string())).unwrap();

        let mut field2 = MessageRepeatedField::new();
        field2.add_message(DynamicMessage::new("test.Message2".to_string())).unwrap();

        let result = field1.merge(field2);
        assert!(result.is_err());
    }

    #[test]
    fn test_message_repeated_field_merge_succeeds() {
        let mut field1 = MessageRepeatedField::new();
        field1.add_message(DynamicMessage::new("test.Message".to_string())).unwrap();

        let mut field2 = MessageRepeatedField::new();
        field2.add_message(DynamicMessage::new("test.Message".to_string())).unwrap();
        field2.add_message(DynamicMessage::new("test.Message".to_string())).unwrap();

        assert!(field1.merge(field2).is_ok());
        assert_eq!(field1.len(), 3);
    }

    #[test]
    fn test_message_repeated_field_merge_into_empty_succeeds() {
        let mut field1 = MessageRepeatedField::new();
        let mut field2 = MessageRepeatedField::new();
        field2.add_message(DynamicMessage::new("test.Message".to_string())).unwrap();

        assert!(field1.merge(field2.clone()).is_ok());
        assert_eq!(field1, field2);
    }

    #[test]
    fn test_merge_simple_repeated_field_succeeds() {
        let map = vec![
            (SimpleRepeatedField::Double(vec![1.0]), SimpleRepeatedField::Double(vec![2.0])),
            (SimpleRepeatedField::Float(vec![1.0]), SimpleRepeatedField::Float(vec![2.0])),
            (SimpleRepeatedField::Bool(vec![false]), SimpleRepeatedField::Bool(vec![true])),
            (SimpleRepeatedField::Int32(vec![1]), SimpleRepeatedField::Int32(vec![2])),
            (SimpleRepeatedField::Int64(vec![1]), SimpleRepeatedField::Int64(vec![2])),
            (SimpleRepeatedField::Uint32(vec![1]), SimpleRepeatedField::Uint32(vec![2])),
            (SimpleRepeatedField::Uint64(vec![1]), SimpleRepeatedField::Uint64(vec![2])),
            (SimpleRepeatedField::Sint32(vec![1]), SimpleRepeatedField::Sint32(vec![2])),
            (SimpleRepeatedField::Sint64(vec![1]), SimpleRepeatedField::Sint64(vec![2])),
            (SimpleRepeatedField::Fixed32(vec![1]), SimpleRepeatedField::Fixed32(vec![2])),
            (SimpleRepeatedField::Fixed64(vec![1]), SimpleRepeatedField::Fixed64(vec![2])),
            (SimpleRepeatedField::Sfixed32(vec![1]), SimpleRepeatedField::Sfixed32(vec![2])),
            (SimpleRepeatedField::Sfixed64(vec![1]), SimpleRepeatedField::Sfixed64(vec![2])),
            (
                SimpleRepeatedField::String(vec!["1".to_owned()]),
                SimpleRepeatedField::String(vec!["2".to_owned()]),
            ),
            (SimpleRepeatedField::Bytes(vec![vec![1]]), SimpleRepeatedField::Bytes(vec![vec![2]])),
            (SimpleRepeatedField::Enum(vec![1]), SimpleRepeatedField::Enum(vec![2])),
        ];
        for (mut original, update) in map {
            assert_eq!(original.len(), 1);
            original.merge(update).unwrap_or_else(|_| {
                panic!(
                    "Merging simple repeated field of type {:?} should succeed",
                    original.get_type()
                )
            });
            assert_eq!(original.len(), 2);
        }
    }

    #[test]
    fn test_merge_simple_repeated_field_on_mismatch_fails() {
        let values = vec![
            SimpleRepeatedField::Double(vec![1.0]),
            SimpleRepeatedField::Float(vec![1.0]),
            SimpleRepeatedField::Bool(vec![false]),
            SimpleRepeatedField::Int32(vec![1]),
            SimpleRepeatedField::Int64(vec![1]),
            SimpleRepeatedField::Uint32(vec![1]),
            SimpleRepeatedField::Uint64(vec![1]),
            SimpleRepeatedField::Sint32(vec![1]),
            SimpleRepeatedField::Sint64(vec![1]),
            SimpleRepeatedField::Fixed32(vec![1]),
            SimpleRepeatedField::Fixed64(vec![1]),
            SimpleRepeatedField::Sfixed32(vec![1]),
            SimpleRepeatedField::Sfixed64(vec![1]),
            SimpleRepeatedField::String(vec!["1".to_owned()]),
            SimpleRepeatedField::Bytes(vec![vec![1]]),
            SimpleRepeatedField::Enum(vec![1]),
        ];
        for value_one in &values {
            for value_two in &values {
                if value_one.get_type() == value_two.get_type() {
                    continue;
                }
                let result = value_one.clone().merge(value_two.clone());
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn test_merge_message_repeated_message_succeeds() {
        let message_name = "test_message".to_owned();

        let original_repeated_field_value = vec![DynamicMessage::new(message_name.clone())];
        let update_repeated_field_value = vec![DynamicMessage::new(message_name.clone())];

        let mut original_repeated_field = RepeatedField::Message(MessageRepeatedField {
            messages: original_repeated_field_value,
        });
        let update_repeated_field =
            RepeatedField::Message(MessageRepeatedField { messages: update_repeated_field_value });

        assert_eq!(original_repeated_field.len(), 1);
        original_repeated_field
            .merge(update_repeated_field)
            .expect("Merging message repeated field should succeed");
        assert_eq!(original_repeated_field.len(), 2);
    }

    #[test]
    fn test_merge_message_repeated_message_on_mismatch_fails() {
        let original_message_name = "test_message_one".to_owned();
        let update_message_name = "test_message_two".to_owned();

        let original_repeated_field_value = vec![DynamicMessage::new(original_message_name)];
        let update_repeated_field_value = vec![DynamicMessage::new(update_message_name)];

        let mut original_repeated_field = RepeatedField::Message(MessageRepeatedField {
            messages: original_repeated_field_value,
        });
        let update_repeated_field =
            RepeatedField::Message(MessageRepeatedField { messages: update_repeated_field_value });

        assert!(original_repeated_field.merge(update_repeated_field).is_err())
    }

    #[test]
    fn test_merge_repeated_field_on_mismatch_fails() {
        let mut original_repeated_field =
            RepeatedField::Simple(SimpleRepeatedField::Bool(vec![false]));
        let update_repeated_field = RepeatedField::Message(MessageRepeatedField {
            messages: vec![DynamicMessage::new("test_message".to_owned())],
        });
        let result = original_repeated_field.merge(update_repeated_field);
        assert!(result.is_err());
    }

    #[test]
    fn test_write_simple_repeated_field() {
        let cases = vec![
            (SimpleRepeatedField::Int32(vec![150, 1]), 1, vec![0x0A, 0x03, 0x96, 0x01, 0x01]),
            (
                SimpleRepeatedField::String(vec!["a".to_string(), "b".to_string()]),
                2,
                vec![0x12, 0x01, 0x61, 0x12, 0x01, 0x62],
            ),
            (
                SimpleRepeatedField::Fixed32(vec![1, 2]),
                3,
                vec![0x1A, 0x08, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00],
            ),
            (SimpleRepeatedField::Int32(vec![]), 4, vec![]),
            (
                SimpleRepeatedField::Double(vec![1.0, 2.0]),
                5,
                vec![
                    0x2A, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x40,
                ],
            ),
            (
                SimpleRepeatedField::Float(vec![1.0, 2.0]),
                6,
                vec![0x32, 0x08, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x40],
            ),
            (SimpleRepeatedField::Int64(vec![150, 1]), 7, vec![0x3A, 0x03, 0x96, 0x01, 0x01]),
            (SimpleRepeatedField::Uint32(vec![150, 1]), 8, vec![0x42, 0x03, 0x96, 0x01, 0x01]),
            (SimpleRepeatedField::Uint64(vec![150, 1]), 9, vec![0x4A, 0x03, 0x96, 0x01, 0x01]),
            (SimpleRepeatedField::Sint32(vec![-1, 1]), 10, vec![0x52, 0x02, 0x01, 0x02]),
            (SimpleRepeatedField::Sint64(vec![-1, 1]), 11, vec![0x5A, 0x02, 0x01, 0x02]),
            (
                SimpleRepeatedField::Fixed64(vec![1, 2]),
                12,
                vec![
                    0x62, 0x10, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00,
                ],
            ),
            (
                SimpleRepeatedField::Sfixed32(vec![-1, 1]),
                13,
                vec![0x6A, 0x08, 0xff, 0xff, 0xff, 0xff, 0x01, 0x00, 0x00, 0x00],
            ),
            (
                SimpleRepeatedField::Sfixed64(vec![-1, 1]),
                14,
                vec![
                    0x72, 0x10, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00,
                ],
            ),
            (SimpleRepeatedField::Bool(vec![true, false]), 15, vec![0x7A, 0x02, 0x01, 0x00]),
            (
                SimpleRepeatedField::Bytes(vec![vec![1, 2], vec![3]]),
                16,
                vec![0x82, 0x01, 0x02, 0x01, 0x02, 0x82, 0x01, 0x01, 0x03],
            ),
            (SimpleRepeatedField::Enum(vec![1, 2]), 17, vec![0x8A, 0x01, 0x02, 0x01, 0x02]),
        ];

        for (field, field_number, expected) in cases {
            let mut buffer = Vec::new();
            let mut writer = RpcWriter::create_rpc_writer(&mut buffer);
            field
                .write_simple_repeated_field(field_number, &mut writer)
                .expect("Writing simple repeated field should succeed");
            writer.finalize().expect("Finalizing writer should succeed");
            assert_eq!(buffer, expected, "Mismatch for {:?}", field);
        }
    }
}
