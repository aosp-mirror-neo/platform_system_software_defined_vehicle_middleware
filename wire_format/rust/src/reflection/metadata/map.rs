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

use crate::reflection::metadata::message::SingularTypeMetadata;

/// Metadata for a map entry type. A map field is wire-format equivalent to a repeated message
/// field, where the message has 2 fields: a `key` field with number 1 and a `value` field
/// with number 2.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MapMetadata {
    // A map's key type can only be a subset of scalar types
    key_metadata: MapKeyType,
    // A map's value type can only be a non-repeated type
    value_metadata: SingularTypeMetadata,
}

/// Types that can be used as map keys - these are all scalar types except floats, doubles and
/// bytes fields
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum MapKeyType {
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
}
