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

//! All APIs from the `metadata` module that are meant to be publicly exposed

/// APIs for building reflection metadata
pub mod protobuf {
    pub use crate::reflection::metadata::error::MetadataBuildError;
    pub use crate::reflection::metadata::field_type::ProtobufSimpleType;
    pub use crate::reflection::metadata::map::MapKeyType;
    pub use crate::reflection::metadata::map::MapMetadata;
    pub use crate::reflection::metadata::message::FieldMetadata;
    pub use crate::reflection::metadata::message::MessageMetadata;
    pub use crate::reflection::metadata::message::SingularTypeMetadata;
    pub use crate::reflection::metadata::simple::SimpleMetadata;
}
