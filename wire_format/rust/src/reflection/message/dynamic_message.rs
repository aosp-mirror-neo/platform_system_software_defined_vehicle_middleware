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

use protobuf::rt::WireType;
use protobuf::well_known_types::any::Any;
use protobuf::MessageFull;

use crate::reflection::codec::api::protobuf::IProtobufWriter;
use crate::reflection::codec::api::protobuf::RpcWriter as ProtobufWriter;
use crate::reflection::message::error::MergeError;
use crate::reflection::message::map::MapField;
use crate::reflection::message::repeated::RepeatedField;
use crate::reflection::message::simple::SimpleField;

/// A struct that can be used to represent a Protobuf message in-memory
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicMessage {
    full_name: String,
    fields: HashMap<u32, DynamicMessageField>,
}

impl DynamicMessage {
    /// Create a new dynamic message with the given `full_name`
    pub(crate) fn new(full_name: String) -> Self {
        Self { full_name, fields: HashMap::new() }
    }

    /// Build a dynamic message corresponding to the 'Any' message type
    /// with the given `message_name` and `value`
    pub fn build_any_dynamic_message(message_name: &str, value: Vec<u8>) -> Self {
        let type_url_field = DynamicMessageField::Singular(SingularField::Simple(
            SimpleField::String(format!("type.googleapis.com/{message_name}")),
        ));
        let value_field =
            DynamicMessageField::Singular(SingularField::Simple(SimpleField::Bytes(value)));
        let mut fields = HashMap::new();
        fields.insert(1, type_url_field);
        fields.insert(2, value_field);
        Self { full_name: Any::descriptor().full_name().to_owned(), fields }
    }

    fn merge_field(
        &mut self,
        field_number: u32,
        message_field: DynamicMessageField,
    ) -> Result<(), MergeError> {
        let message_full_name = self.get_full_name().to_owned();
        match self.fields.entry(field_number) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                entry.get_mut().merge(message_field, &message_full_name, field_number)?;
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(message_field);
            }
        }
        Ok(())
    }

    pub(crate) fn merge(
        &mut self,
        other_dynamic_message: DynamicMessage,
    ) -> Result<(), MergeError> {
        if self.get_full_name().ne(other_dynamic_message.get_full_name()) {
            return Err(MergeError {
                    message_name: self.get_full_name().to_owned(),
                    field_number: 0,
                    source: anyhow::anyhow!(
                        "Attempting to merge messages with different names: Self is {:?}, other is {:?}",
                        self.get_full_name(),
                        other_dynamic_message.get_full_name()
                    ),
                });
        }
        for (field_number, other_message_field) in other_dynamic_message.fields {
            self.merge_field(field_number, other_message_field)?;
        }
        Ok(())
    }

    /// Get dynamic message field with number `field_number`
    pub fn get_message_field(&self, field_number: &u32) -> Option<&DynamicMessageField> {
        self.fields.get(field_number)
    }

    /// Add a message field to the current message with the given `field_number`. If the message already
    /// has a field with the same number, the fields are merged together.
    pub fn add_field(
        &mut self,
        field_number: u32,
        message_field: DynamicMessageField,
    ) -> Result<(), MergeError> {
        self.merge_field(field_number, message_field)
    }

    /// Get the message name
    pub fn get_full_name(&self) -> &str {
        &self.full_name
    }

    fn num_fields(&self) -> usize {
        self.fields.len()
    }

    /// Serialize the dynamic message to a byte array
    pub fn write_to_bytes(&self) -> anyhow::Result<Vec<u8>> {
        let mut buffer = Vec::new();
        // we use a RPC writer since we don't need to prefix the message with a length
        let mut writer = ProtobufWriter::create_rpc_writer(&mut buffer);
        self.write_message(&mut writer)?;
        writer.finalize()?;
        Ok(buffer)
    }

    pub(crate) fn write_message_field(
        &self,
        field_number: u32,
        writer: &mut ProtobufWriter,
    ) -> anyhow::Result<()> {
        writer.write_tag(field_number, WireType::LengthDelimited)?;
        writer.start_length_delimited_message()?;
        self.write_message(writer)?;
        writer.end_length_delimited_message()
    }

    pub(crate) fn write_message(&self, writer: &mut ProtobufWriter) -> anyhow::Result<()> {
        for (field_number, dynamic_message_field) in &self.fields {
            dynamic_message_field.write_field(*field_number, writer)?;
        }
        Ok(())
    }
}

/// Message field value
#[derive(Debug, Clone, PartialEq)]
pub enum DynamicMessageField {
    /// Singular field
    Singular(SingularField),
    /// Repeated field
    Repeated(RepeatedField),
    /// Map field
    Map(MapField),
}

/// Singular field value
#[derive(Debug, Clone, PartialEq)]
pub enum SingularField {
    /// Simple field
    Simple(SimpleField),
    /// Message field
    Message(Box<DynamicMessage>),
}

impl SingularField {
    pub(crate) fn get_type_string(&self) -> String {
        match self {
            SingularField::Simple(simple) => format!("{:?}", simple.get_type()),
            SingularField::Message(message) => message.get_full_name().to_string(),
        }
    }

    pub(crate) fn type_matches(&self, other: &SingularField) -> bool {
        match (self, other) {
            (SingularField::Simple(self_simple), SingularField::Simple(other_simple)) => {
                self_simple.get_type() == other_simple.get_type()
            }
            (SingularField::Message(self_message), SingularField::Message(other_message)) => {
                self_message.get_full_name().eq(other_message.get_full_name())
            }
            _ => false,
        }
    }

    pub(crate) fn merge(&mut self, other: SingularField) -> anyhow::Result<()> {
        match (self, other) {
            (
                SingularField::Simple(self_simple_field),
                SingularField::Simple(other_simple_field),
            ) => self_simple_field.merge(other_simple_field),
            (
                SingularField::Message(self_dynamic_message),
                SingularField::Message(other_dynamic_message),
            ) => self_dynamic_message
                .merge(*other_dynamic_message)
                .map_err(|source| anyhow::anyhow!("{}", source)),
            (self_singular_field, other_singular_field) => {
                anyhow::bail!("Attempting to merge singular fields of different types: Self is {:?}, other is {:?}", self_singular_field.get_type_string(), other_singular_field.get_type_string())
            }
        }
    }

    pub(crate) fn write_singular_field(
        &self,
        field_number: u32,
        writer: &mut ProtobufWriter,
    ) -> anyhow::Result<()> {
        match self {
            SingularField::Simple(simple_field) => {
                simple_field.write_simple_field(field_number, writer)
            }
            SingularField::Message(message) => message.write_message_field(field_number, writer),
        }
    }
}

impl DynamicMessageField {
    fn get_type_for_error_msg(&self) -> String {
        match self {
            DynamicMessageField::Singular(singular_field) => {
                format!("Singular: {:?}", singular_field.get_type_string())
            }
            DynamicMessageField::Repeated(_) => "Repeated".to_owned(),
            DynamicMessageField::Map(_) => "Map".to_owned(),
        }
    }

    pub(crate) fn merge(
        &mut self,
        other_field: DynamicMessageField,
        message_name: &str,
        field_number: u32,
    ) -> Result<(), MergeError> {
        match (self, other_field) {
            (
                DynamicMessageField::Singular(self_singular_field),
                DynamicMessageField::Singular(other_singular_field),
            ) => self_singular_field.merge(other_singular_field).map_err(|source| MergeError {
                message_name: message_name.to_owned(),
                field_number,
                source,
            }),
            (
                DynamicMessageField::Repeated(self_repeated_field),
                DynamicMessageField::Repeated(other_repeated_field),
            ) => self_repeated_field.merge(other_repeated_field).map_err(|source| MergeError {
                message_name: message_name.to_owned(),
                field_number,
                source,
            }),
            (
                DynamicMessageField::Map(self_map_field),
                DynamicMessageField::Map(other_map_field),
            ) => self_map_field.merge(other_map_field).map_err(|source| MergeError {
                message_name: message_name.to_owned(),
                field_number,
                source,
            }),
            (self_field, other_field) => Err(MergeError {
                message_name: message_name.to_owned(),
                field_number,
                source: anyhow::anyhow!(
                    "Attempting to merge fields of different types: Self is {:?}, other is {:?}",
                    self_field.get_type_for_error_msg(),
                    other_field.get_type_for_error_msg()
                ),
            }),
        }
    }

    pub(crate) fn write_field(
        &self,
        field_number: u32,
        writer: &mut ProtobufWriter,
    ) -> anyhow::Result<()> {
        match self {
            DynamicMessageField::Singular(singular_field) => {
                singular_field.write_singular_field(field_number, writer)
            }
            DynamicMessageField::Repeated(repeated_field) => {
                repeated_field.write_repeated_field(field_number, writer)
            }
            DynamicMessageField::Map(map_field) => map_field.write_map_field(field_number, writer),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::reflection::message::repeated::SimpleRepeatedField;

    use super::*;

    #[test]
    fn test_dynamic_message_new() {
        let full_name = "test.Message".to_string();
        let message = DynamicMessage::new(full_name.clone());
        assert_eq!(message.get_full_name(), &full_name);
        assert!(message.fields.is_empty());
    }

    #[test]
    fn test_singular_field_get_type_string() {
        let simple_field = SingularField::Simple(SimpleField::Int32(42));
        assert_eq!(simple_field.get_type_string(), "Int32");

        let message_field =
            SingularField::Message(Box::new(DynamicMessage::new("test.Nested".to_string())));
        assert_eq!(message_field.get_type_string(), "test.Nested");
    }

    #[test]
    fn test_singular_field_type_matches_simple() {
        let field1 = SingularField::Simple(SimpleField::Int32(100));
        let field2 = SingularField::Simple(SimpleField::Int32(200));
        let field3 = SingularField::Simple(SimpleField::Float(1.0));

        assert!(field1.type_matches(&field2));
        assert!(!field1.type_matches(&field3));
    }

    #[test]
    fn test_singular_field_type_matches_message() {
        let msg1 = DynamicMessage::new("test.MessageA".to_string());
        let msg2 = DynamicMessage::new("test.MessageA".to_string());
        let msg3 = DynamicMessage::new("test.MessageB".to_string());

        let field1 = SingularField::Message(Box::new(msg1));
        let field2 = SingularField::Message(Box::new(msg2));
        let field3 = SingularField::Message(Box::new(msg3));

        assert!(field1.type_matches(&field2));
        assert!(!field1.type_matches(&field3));
    }

    #[test]
    fn test_singular_field_type_matches_mixed() {
        let simple_field = SingularField::Simple(SimpleField::Bool(true));
        let message_field =
            SingularField::Message(Box::new(DynamicMessage::new("test.Message".to_string())));

        assert!(!simple_field.type_matches(&message_field));
        assert!(!message_field.type_matches(&simple_field));
    }

    #[test]
    fn test_merge_simple_field_succeeds() {
        let simple_field_one = SimpleField::Fixed32(1);
        let simple_field_two = SimpleField::Fixed32(2);

        let message_field_one =
            DynamicMessageField::Singular(SingularField::Simple(simple_field_one));
        let message_field_two =
            DynamicMessageField::Singular(SingularField::Simple(simple_field_two));

        let message_name = "test_message".to_owned();
        let field_number = 1;

        let mut dynamic_message = DynamicMessage::new(message_name);
        assert!(dynamic_message.merge_field(field_number, message_field_one.clone()).is_ok());

        assert_eq!(dynamic_message.num_fields(), 1);
        assert_eq!(dynamic_message.get_message_field(&field_number), Some(&message_field_one));

        assert!(dynamic_message.merge_field(field_number, message_field_two.clone()).is_ok());
        assert_eq!(dynamic_message.num_fields(), 1);

        assert_eq!(dynamic_message.get_message_field(&field_number), Some(&message_field_two));
    }

    #[test]
    fn test_merge_repeated_field_succeeds() {
        let repeated_field_one = RepeatedField::Simple(SimpleRepeatedField::Fixed32(vec![1]));
        let repeated_field_two = RepeatedField::Simple(SimpleRepeatedField::Fixed32(vec![2]));
        let expected_merged_repeated_field =
            RepeatedField::Simple(SimpleRepeatedField::Fixed32(vec![1, 2]));

        let message_field_one = DynamicMessageField::Repeated(repeated_field_one);
        let message_field_two = DynamicMessageField::Repeated(repeated_field_two);
        let expected_merged_field = DynamicMessageField::Repeated(expected_merged_repeated_field);

        let message_name = "test_message".to_owned();
        let field_number = 1;

        let mut dynamic_message = DynamicMessage::new(message_name);
        assert!(dynamic_message.merge_field(field_number, message_field_one.clone()).is_ok());

        assert_eq!(dynamic_message.num_fields(), 1);
        assert_eq!(dynamic_message.get_message_field(&field_number), Some(&message_field_one));

        assert!(dynamic_message.merge_field(field_number, message_field_two).is_ok());
        assert_eq!(dynamic_message.num_fields(), 1);

        assert_eq!(dynamic_message.get_message_field(&field_number), Some(&expected_merged_field));
    }

    #[test]
    fn test_merge_message_field_succeeds() {
        let mut message_field_one = DynamicMessage::new("test_submessage".to_owned());
        assert!(message_field_one
            .add_field(
                1,
                DynamicMessageField::Singular(SingularField::Simple(SimpleField::Bool(true)))
            )
            .is_ok());
        assert!(message_field_one
            .add_field(
                2,
                DynamicMessageField::Repeated(RepeatedField::Simple(SimpleRepeatedField::Float(
                    vec![1.0, 2.0, 3.0,]
                ))),
            )
            .is_ok());
        assert_eq!(message_field_one.num_fields(), 2);

        let mut message_field_two = DynamicMessage::new("test_submessage".to_owned());
        assert!(message_field_two
            .add_field(
                1,
                DynamicMessageField::Singular(SingularField::Simple(SimpleField::Bool(false)))
            )
            .is_ok());
        assert!(message_field_two
            .add_field(
                2,
                DynamicMessageField::Repeated(RepeatedField::Simple(SimpleRepeatedField::Float(
                    vec![10.0, 20.0, 30.0,]
                ))),
            )
            .is_ok());
        assert_eq!(message_field_two.num_fields(), 2);

        let message_field_one =
            DynamicMessageField::Singular(SingularField::Message(Box::new(message_field_one)));
        let message_field_two =
            DynamicMessageField::Singular(SingularField::Message(Box::new(message_field_two)));

        let mut dynamic_message = DynamicMessage::new("test_message".to_owned());
        assert!(dynamic_message.merge_field(1, message_field_one.clone()).is_ok());
        assert_eq!(dynamic_message.num_fields(), 1);
        assert!(dynamic_message.get_message_field(&1).is_some());

        assert!(dynamic_message.merge_field(1, message_field_two.clone()).is_ok());
        assert_eq!(dynamic_message.num_fields(), 1);
        assert!(dynamic_message.get_message_field(&1).is_some());

        let merged_message_field =
            dynamic_message.get_message_field(&1).expect("Merged message field should exist");
        let DynamicMessageField::Singular(SingularField::Message(merged_message_value)) =
            merged_message_field
        else {
            panic!("Merged message field should be a message field");
        };
        assert_eq!(merged_message_value.num_fields(), 2);
        assert_eq!(
            merged_message_value.get_message_field(&1),
            Some(&DynamicMessageField::Singular(SingularField::Simple(SimpleField::Bool(false))))
        );
        let DynamicMessageField::Repeated(repeated_field) =
            merged_message_value.get_message_field(&2).expect("Repeated field should exist")
        else {
            panic!("Repeated field should exist");
        };
        assert_eq!(repeated_field.len(), 6);
    }

    #[test]
    fn test_write_to_bytes() {
        let mut message = DynamicMessage::new("test.Message".to_string());
        message
            .add_field(
                1,
                DynamicMessageField::Singular(SingularField::Simple(SimpleField::Int32(150))),
            )
            .expect("Failed to add field");

        let bytes = message.write_to_bytes().unwrap();
        assert_eq!(bytes, vec![0x08, 0x96, 0x01]);
    }

    #[test]
    fn test_write_to_bytes_nested() {
        let mut inner = DynamicMessage::new("test.Inner".to_string());
        inner
            .add_field(
                1,
                DynamicMessageField::Singular(SingularField::Simple(SimpleField::Int32(150))),
            )
            .expect("Failed to add field");

        let mut outer = DynamicMessage::new("test.Outer".to_string());
        outer
            .add_field(2, DynamicMessageField::Singular(SingularField::Message(Box::new(inner))))
            .expect("Failed to add field");

        let bytes = outer.write_to_bytes().unwrap();
        // Field 2 (tag 0x12), length 3, payload 0x08 0x96 0x01
        assert_eq!(bytes, vec![0x12, 0x03, 0x08, 0x96, 0x01]);
    }

    #[test]
    fn test_write_to_bytes_multiple_fields() {
        let mut message = DynamicMessage::new("test.Message".to_string());
        message
            .add_field(
                1,
                DynamicMessageField::Singular(SingularField::Simple(SimpleField::Int32(150))),
            )
            .expect("Failed to add field");
        message
            .add_field(
                2,
                DynamicMessageField::Singular(SingularField::Simple(SimpleField::String(
                    "foo".to_string(),
                ))),
            )
            .expect("Failed to add field");

        let bytes = message.write_to_bytes().unwrap();
        // Field 1: 08 96 01
        // Field 2: 12 03 66 6f 6f
        // Order is not guaranteed due to HashMap storage of fields
        let expected1 = vec![0x08, 0x96, 0x01, 0x12, 0x03, 0x66, 0x6f, 0x6f];
        let expected2 = vec![0x12, 0x03, 0x66, 0x6f, 0x6f, 0x08, 0x96, 0x01];
        assert!(bytes == expected1 || bytes == expected2);
    }

    #[test]
    fn test_write_to_bytes_repeated_field() {
        let mut message = DynamicMessage::new("test.Message".to_string());
        message
            .add_field(
                1,
                DynamicMessageField::Repeated(RepeatedField::Simple(SimpleRepeatedField::Int32(
                    vec![1, 2],
                ))),
            )
            .expect("Failed to add field");

        let bytes = message.write_to_bytes().expect("Failed to serialize message to bytes");
        // Tag: 1 (00001) | WireType::LengthDelimited (010) -> 0x0A
        // Length: 2
        // Payload: 01 02
        assert_eq!(bytes, vec![0x0A, 0x02, 0x01, 0x02]);
    }

    #[test]
    fn test_write_to_bytes_map_field() {
        use crate::reflection::message::map::MapFieldContainer;
        let mut message = DynamicMessage::new("test.Message".to_string());
        let mut map_container = MapFieldContainer::new();
        map_container
            .insert(1, SingularField::Simple(SimpleField::Int32(2)))
            .expect("Failed to insert value");
        message.add_field(1, DynamicMessageField::Map(MapField::Int32(map_container))).unwrap();

        let bytes = message.write_to_bytes().expect("Failed to serialize message to bytes");
        // Tag: 0x0A, Length: 4, Key: 08 01, Value: 10 02
        assert_eq!(bytes, vec![0x0A, 0x04, 0x08, 0x01, 0x10, 0x02]);
    }

    #[test]
    fn test_build_any_dynamic_message() {
        let message_name = "test.Message";
        let value = vec![1, 2, 3, 4];
        let dynamic_message =
            DynamicMessage::build_any_dynamic_message(message_name, value.clone());

        assert_eq!(dynamic_message.get_full_name(), "google.protobuf.Any");
        assert_eq!(dynamic_message.num_fields(), 2);

        let type_url_field = dynamic_message.get_message_field(&1).expect("type_url field missing");
        if let DynamicMessageField::Singular(SingularField::Simple(SimpleField::String(type_url))) =
            type_url_field
        {
            assert_eq!(type_url, "type.googleapis.com/test.Message");
        } else {
            panic!("Unexpected type for type_url field");
        }

        let value_field = dynamic_message.get_message_field(&2).expect("value field missing");
        if let DynamicMessageField::Singular(SingularField::Simple(SimpleField::Bytes(bytes))) =
            value_field
        {
            assert_eq!(bytes, &value);
        } else {
            panic!("Unexpected type for value field");
        }
    }
}
