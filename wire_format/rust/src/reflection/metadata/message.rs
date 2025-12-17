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

use std::collections::{HashMap, HashSet};

use protobuf::reflect::Syntax;

use crate::reflection::metadata::map::MapMetadata;
use crate::reflection::metadata::simple::SimpleMetadata;

/// Protobuf Message metadata that contains information relevant to wire format de/serialization, message
/// full name, field names declared in the message and fields whose presence can be tracked
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MessageMetadata {
    // File's syntax, can either be proto2 or proto3
    syntax: Syntax,
    // Map from field number to the metadata associated with the field
    fields_metadata: HashMap<u32, FieldMetadata>,
    // Message's proto full name
    message_full_name: String,
    // Map from field name to the field number
    field_name_to_number: HashMap<String, u32>,
    // Set of fields declared within oneofs, plus message fields in proto3 syntax that have the
    // `optional` label
    oneofs_and_proto3_optional_fields: HashSet<u32>,
}

/// Metadata for a message field
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum FieldMetadata {
    /// Singular field
    Singular(SingularTypeMetadata),
    /// Repeated field
    Repeated(SingularTypeMetadata),
    /// Map field
    Map(Box<MapMetadata>),
}

/// Singular (non-repeated, non-map) metadata
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SingularTypeMetadata {
    Simple(SimpleMetadata),
    Message(Box<MessageMetadata>),
}
