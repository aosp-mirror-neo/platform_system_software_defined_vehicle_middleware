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

//! All APIs from the `message` module that are meant to be publicly exposed

/// APIs for protobuf dynamic messages
pub mod protobuf {
    pub use crate::reflection::message::dynamic_message::DynamicMessage;
    pub use crate::reflection::message::dynamic_message::DynamicMessageField;
    pub use crate::reflection::message::dynamic_message::SingularField;
    pub use crate::reflection::message::error::MergeError;
    pub use crate::reflection::message::map::MapField;
    pub use crate::reflection::message::map::MapFieldContainer;
    pub use crate::reflection::message::repeated::MessageRepeatedField;
    pub use crate::reflection::message::repeated::RepeatedField;
    pub use crate::reflection::message::repeated::SimpleRepeatedField;
    pub use crate::reflection::message::simple::SimpleField;
}
