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

use crate::reflection::metadata::error::MetadataBuildError;

/// Simple protobuf types - these represent all scalar protobuf types and enums
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ProtobufSimpleType {
    /// Double
    Double,
    /// Float
    Float,
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
    /// Bytes
    Bytes,
    /// Enum
    Enum,
}

impl TryFrom<Type> for ProtobufSimpleType {
    type Error = MetadataBuildError;

    fn try_from(value: Type) -> Result<Self, Self::Error> {
        let simple_type = match value {
            Type::TYPE_DOUBLE => ProtobufSimpleType::Double,
            Type::TYPE_FLOAT => ProtobufSimpleType::Float,
            Type::TYPE_INT64 => ProtobufSimpleType::Int64,
            Type::TYPE_UINT64 => ProtobufSimpleType::Uint64,
            Type::TYPE_INT32 => ProtobufSimpleType::Int32,
            Type::TYPE_FIXED64 => ProtobufSimpleType::Fixed64,
            Type::TYPE_FIXED32 => ProtobufSimpleType::Fixed32,
            Type::TYPE_BOOL => ProtobufSimpleType::Bool,
            Type::TYPE_STRING => ProtobufSimpleType::String,
            Type::TYPE_BYTES => ProtobufSimpleType::Bytes,
            Type::TYPE_UINT32 => ProtobufSimpleType::Uint32,
            Type::TYPE_ENUM => ProtobufSimpleType::Enum,
            Type::TYPE_SFIXED32 => ProtobufSimpleType::Sfixed32,
            Type::TYPE_SFIXED64 => ProtobufSimpleType::Sfixed64,
            Type::TYPE_SINT32 => ProtobufSimpleType::Sint32,
            Type::TYPE_SINT64 => ProtobufSimpleType::Sint64,
            Type::TYPE_GROUP => {
                return Err(MetadataBuildError::SimpleTypeExpected("Group"));
            }
            Type::TYPE_MESSAGE => {
                return Err(MetadataBuildError::SimpleTypeExpected("Message"));
            }
        };
        Ok(simple_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    use protobuf::descriptor::field_descriptor_proto::Type;

    #[test]
    fn test_try_from_simple_type_succeeds() {
        let simple_type_map = HashMap::from([
            (Type::TYPE_DOUBLE, ProtobufSimpleType::Double),
            (Type::TYPE_FLOAT, ProtobufSimpleType::Float),
            (Type::TYPE_INT64, ProtobufSimpleType::Int64),
            (Type::TYPE_UINT64, ProtobufSimpleType::Uint64),
            (Type::TYPE_INT32, ProtobufSimpleType::Int32),
            (Type::TYPE_FIXED64, ProtobufSimpleType::Fixed64),
            (Type::TYPE_FIXED32, ProtobufSimpleType::Fixed32),
            (Type::TYPE_BOOL, ProtobufSimpleType::Bool),
            (Type::TYPE_STRING, ProtobufSimpleType::String),
            (Type::TYPE_BYTES, ProtobufSimpleType::Bytes),
            (Type::TYPE_UINT32, ProtobufSimpleType::Uint32),
            (Type::TYPE_ENUM, ProtobufSimpleType::Enum),
            (Type::TYPE_SFIXED32, ProtobufSimpleType::Sfixed32),
            (Type::TYPE_SFIXED64, ProtobufSimpleType::Sfixed64),
            (Type::TYPE_SINT32, ProtobufSimpleType::Sint32),
            (Type::TYPE_SINT64, ProtobufSimpleType::Sint64),
        ]);

        for (type_value, expected_simple_type) in simple_type_map {
            let actual_simple_type = ProtobufSimpleType::try_from(type_value);
            assert!(actual_simple_type.is_ok());
            assert_eq!(actual_simple_type.unwrap(), expected_simple_type);
        }
    }

    #[test]
    fn test_try_from_non_simple_type_fails() {
        let group_result = ProtobufSimpleType::try_from(Type::TYPE_GROUP);
        assert!(group_result.is_err());
        assert_eq!(group_result.unwrap_err(), MetadataBuildError::SimpleTypeExpected("Group"));

        let message_result = ProtobufSimpleType::try_from(Type::TYPE_MESSAGE);
        assert!(message_result.is_err());
        assert_eq!(message_result.unwrap_err(), MetadataBuildError::SimpleTypeExpected("Message"));
    }
}
