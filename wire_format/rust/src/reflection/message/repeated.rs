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

use crate::reflection::message::dynamic_message::DynamicMessage;

/// Dynamic protobuf repeated field
#[derive(Debug, Clone, PartialEq)]
pub enum RepeatedField {
    /// Simple repeated field
    Simple(SimpleRepeatedField),
    /// Message repeated field
    Message(MessageRepeatedField),
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

#[derive(Debug, Clone, PartialEq)]
pub struct MessageRepeatedField {
    messages: Vec<DynamicMessage>,
}

impl MessageRepeatedField {
    fn new() -> Self {
        Self { messages: Vec::new() }
    }

    pub fn merge(&mut self, other: MessageRepeatedField) -> anyhow::Result<()> {
        let self_type = self.messages.first().map(|m| m.get_full_name());
        let other_type = other.messages.first().map(|m| m.get_full_name());

        if let (Some(self_type), Some(other_type)) = (self_type, other_type) {
            if self_type.ne(other_type) {
                anyhow::bail!(
                    "Trying to merge repeated field with type {} into repeated field with existing type {}",
                    other_type,
                    self_type
                );
            }
        }
        self.messages.extend(other.messages);
        Ok(())
    }

    pub fn get_messages(&self) -> &[DynamicMessage] {
        &self.messages
    }

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

    pub fn len(&self) -> usize {
        self.messages.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
