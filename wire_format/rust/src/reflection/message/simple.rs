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

use crate::reflection::metadata::api::protobuf::ProtobufSimpleType;

/// Dynamic protobuf simple field
#[derive(Debug, Clone, PartialEq)]
pub enum SimpleField {
    /// Double
    Double(f64),
    /// Float
    Float(f32),
    /// Int32
    Int32(i32),
    /// Int64
    Int64(i64),
    /// Uint32
    Uint32(u32),
    /// Uint64
    Uint64(u64),
    /// Sint32
    Sint32(i32),
    /// Sint64
    Sint64(i64),
    /// Fixed32
    Fixed32(u32),
    /// Fixed64
    Fixed64(u64),
    /// Sfixed32
    Sfixed32(i32),
    /// Sfixed64
    Sfixed64(i64),
    /// Bool
    Bool(bool),
    /// String
    String(String),
    /// Bytes
    Bytes(Vec<u8>),
    /// Enum
    Enum(i32),
}

impl SimpleField {
    // Merging a simple field overwrites the previous value according to the
    // `last one wins` rule
    pub(crate) fn merge(&mut self, other_simple_field: SimpleField) -> anyhow::Result<()> {
        match (self, other_simple_field) {
            (SimpleField::Double(self_double), SimpleField::Double(other_double)) => {
                *self_double = other_double;
            }
            (SimpleField::Float(self_float), SimpleField::Float(other_float)) => {
                *self_float = other_float;
            }
            (SimpleField::Int32(self_int32), SimpleField::Int32(other_int32)) => {
                *self_int32 = other_int32;
            }
            (SimpleField::Int64(self_int64), SimpleField::Int64(other_int64)) => {
                *self_int64 = other_int64;
            }
            (SimpleField::Uint32(self_uint32), SimpleField::Uint32(other_uint32)) => {
                *self_uint32 = other_uint32;
            }
            (SimpleField::Uint64(self_uint64), SimpleField::Uint64(other_uint64)) => {
                *self_uint64 = other_uint64;
            }
            (SimpleField::Sint32(self_sint32), SimpleField::Sint32(other_sint32)) => {
                *self_sint32 = other_sint32;
            }
            (SimpleField::Sint64(self_sint64), SimpleField::Sint64(other_sint64)) => {
                *self_sint64 = other_sint64;
            }
            (SimpleField::Fixed32(self_fixed32), SimpleField::Fixed32(other_fixed32)) => {
                *self_fixed32 = other_fixed32;
            }
            (SimpleField::Fixed64(self_fixed64), SimpleField::Fixed64(other_fixed64)) => {
                *self_fixed64 = other_fixed64;
            }
            (SimpleField::Sfixed32(self_sfixed32), SimpleField::Sfixed32(other_sfixed32)) => {
                *self_sfixed32 = other_sfixed32;
            }
            (SimpleField::Sfixed64(self_sfixed64), SimpleField::Sfixed64(other_sfixed64)) => {
                *self_sfixed64 = other_sfixed64;
            }
            (SimpleField::Bool(self_bool), SimpleField::Bool(other_bool)) => {
                *self_bool = other_bool;
            }
            (SimpleField::String(self_string), SimpleField::String(other_string)) => {
                *self_string = other_string;
            }
            (SimpleField::Bytes(self_bytes), SimpleField::Bytes(other_bytes)) => {
                *self_bytes = other_bytes;
            }
            (SimpleField::Enum(self_enum), SimpleField::Enum(other_enum)) => {
                *self_enum = other_enum;
            }
            (self_field_value, other_field_value) => {
                anyhow::bail!(
                    "Attempting to merge simple values with different types: Self type is {:?}, other type is {:?}",
                    self_field_value.get_type(),
                    other_field_value.get_type()
                )
            }
        }
        Ok(())
    }

    pub(crate) fn get_type(&self) -> ProtobufSimpleType {
        match self {
            SimpleField::Double(_) => ProtobufSimpleType::Double,
            SimpleField::Float(_) => ProtobufSimpleType::Float,
            SimpleField::Int32(_) => ProtobufSimpleType::Int32,
            SimpleField::Int64(_) => ProtobufSimpleType::Int64,
            SimpleField::Uint32(_) => ProtobufSimpleType::Uint32,
            SimpleField::Uint64(_) => ProtobufSimpleType::Uint64,
            SimpleField::Sint32(_) => ProtobufSimpleType::Sint32,
            SimpleField::Sint64(_) => ProtobufSimpleType::Sint64,
            SimpleField::Fixed32(_) => ProtobufSimpleType::Fixed32,
            SimpleField::Fixed64(_) => ProtobufSimpleType::Fixed64,
            SimpleField::Sfixed32(_) => ProtobufSimpleType::Sfixed32,
            SimpleField::Sfixed64(_) => ProtobufSimpleType::Sfixed64,
            SimpleField::Bool(_) => ProtobufSimpleType::Bool,
            SimpleField::String(_) => ProtobufSimpleType::String,
            SimpleField::Bytes(_) => ProtobufSimpleType::Bytes,
            SimpleField::Enum(_) => ProtobufSimpleType::Enum,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_simple_field_succeeds() {
        let map = vec![
            (SimpleField::Bool(false), SimpleField::Bool(true)),
            (SimpleField::Int32(1), SimpleField::Int32(2)),
            (SimpleField::Int64(1), SimpleField::Int64(2)),
            (SimpleField::Uint32(1), SimpleField::Uint32(2)),
            (SimpleField::Uint64(1), SimpleField::Uint64(2)),
            (SimpleField::Sint32(1), SimpleField::Sint32(2)),
            (SimpleField::Sint64(1), SimpleField::Sint64(2)),
            (SimpleField::Fixed32(1), SimpleField::Fixed32(2)),
            (SimpleField::Fixed64(1), SimpleField::Fixed64(2)),
            (SimpleField::Sfixed32(1), SimpleField::Sfixed32(2)),
            (SimpleField::Sfixed64(1), SimpleField::Sfixed64(2)),
            (SimpleField::String("1".to_owned()), SimpleField::String("2".to_owned())),
            (SimpleField::Bytes(vec![1]), SimpleField::Bytes(vec![2])),
            (SimpleField::Enum(1), SimpleField::Enum(2)),
        ];
        for (mut original, updated) in map {
            original.merge(updated.clone()).unwrap_or_else(|_| {
                panic!("Merging simple field of type {:?} should succeed", original.get_type())
            });
            assert_eq!(original, updated);
        }

        let mut simple_double_field = SimpleField::Double(1.0);
        let other_double_field = SimpleField::Double(2.0);
        simple_double_field.merge(other_double_field).unwrap();
        let SimpleField::Double(double_value) = simple_double_field else {
            panic!("Expected a double type, got {:?}", simple_double_field);
        };
        assert!(double_value > 1.5);

        let mut simple_float_field = SimpleField::Float(1.0);
        let other_float_field = SimpleField::Float(2.0);
        simple_float_field.merge(other_float_field).unwrap();
        let SimpleField::Float(float_value) = simple_float_field else {
            panic!("Expected a float type, got {:?}", simple_float_field);
        };
        assert!(float_value > 1.5);
    }

    #[test]
    fn test_merge_simple_field_on_mismatch_fails() {
        let values = vec![
            SimpleField::Double(1.0),
            SimpleField::Float(1.0),
            SimpleField::Bool(false),
            SimpleField::Int32(1),
            SimpleField::Int64(1),
            SimpleField::Uint32(1),
            SimpleField::Uint64(1),
            SimpleField::Sint32(1),
            SimpleField::Sint64(1),
            SimpleField::Fixed32(1),
            SimpleField::Fixed64(1),
            SimpleField::Sfixed32(1),
            SimpleField::Sfixed64(1),
            SimpleField::String("1".to_owned()),
            SimpleField::Bytes(vec![1]),
            SimpleField::Enum(1),
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
}
