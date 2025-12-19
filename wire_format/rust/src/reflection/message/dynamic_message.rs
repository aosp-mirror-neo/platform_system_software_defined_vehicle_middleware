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

use crate::reflection::message::map::MapField;
use crate::reflection::message::repeated::RepeatedField;
use crate::reflection::message::simple::SimpleField;

/// A struct that can be used to represent a Protobuf message in-memory
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DynamicMessage {
    full_name: String,
    fields: HashMap<u32, DynamicMessageField>,
}

impl DynamicMessage {
    pub(crate) fn new(full_name: String) -> Self {
        Self { full_name, fields: HashMap::new() }
    }

    pub fn get_full_name(&self) -> &str {
        &self.full_name
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
}

#[cfg(test)]
mod tests {
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
}
