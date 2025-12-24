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
use crate::reflection::metadata::field_type::ProtobufSimpleType;

/// Simple field type metadata (no nested types)
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SimpleMetadata {
    // How to interpret the field from the input buffer
    field_type: ProtobufSimpleType,
}

impl SimpleMetadata {
    pub(crate) fn new(field_type: ProtobufSimpleType) -> Self {
        Self { field_type }
    }

    pub(crate) fn build(field_type: Type) -> Result<Self, MetadataBuildError> {
        let field_type = ProtobufSimpleType::try_from(field_type)?;
        Ok(Self::new(field_type))
    }

    /// Get the type of the simple field
    pub(crate) fn get_field_type(&self) -> ProtobufSimpleType {
        self.field_type
    }
}
