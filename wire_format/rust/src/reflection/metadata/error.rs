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

use protobuf::reflect::RuntimeType;

/// Errors that may be encountered when building metadata for a protobuf message
#[derive(Debug, PartialEq, Eq, Clone, thiserror::Error)]
pub enum MetadataBuildError {
    /// Simple type expected
    #[error("Expected a simple type, found type - {0}.")]
    SimpleTypeExpected(&'static str),

    /// Message type expected
    #[error("Expected a message type for field - {0}, found type - {1}.")]
    MessageTypeExpected(String, &'static str),

    /// Missing MapEntry Message
    #[error("Missing a map entry message descriptor with name - {0} for field - {1}.")]
    MissingMapEntryDescriptor(String, String),

    /// A singular field type expected
    #[error("Expected a singular field for field - {0}.")]
    SingularTypeExpected(String),

    /// A repeated field type expected
    #[error("Expected a repeated field type for field - {0}.")]
    RepeatedTypeExpected(String),

    /// A map field type expected
    #[error("Expected a map field type for field - {0}.")]
    MapTypeExpected(String),

    /// MapEntry is invalid
    #[error("Invalid type for map entry for field - {0}.")]
    InvalidMapEntryType(String),

    /// MapEntry key type is missing
    #[error("Missing type for map entry key for field - {0}.")]
    MissingMapKeyMetadata(String),

    /// MapEntry value type is missing
    #[error("Missing type for map entry value for field - {0}.")]
    MissingMapValueMetadata(String),
}

impl MetadataBuildError {
    pub(crate) fn expected_message_type(
        field_full_name: String,
        runtime_type: &RuntimeType,
    ) -> Self {
        MetadataBuildError::MessageTypeExpected(
            field_full_name,
            Self::get_runtime_type_string(runtime_type),
        )
    }

    fn get_runtime_type_string(runtime_type: &RuntimeType) -> &'static str {
        match runtime_type {
            RuntimeType::I32 => "i32",
            RuntimeType::I64 => "i64",
            RuntimeType::U32 => "u32",
            RuntimeType::U64 => "u64",
            RuntimeType::F32 => "f32",
            RuntimeType::F64 => "f64",
            RuntimeType::Bool => "bool",
            RuntimeType::String => "string",
            RuntimeType::VecU8 => "bytes",
            RuntimeType::Enum(_) => "enum",
            RuntimeType::Message(_) => "message",
        }
    }
}
