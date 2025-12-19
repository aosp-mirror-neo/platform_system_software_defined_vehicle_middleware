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
