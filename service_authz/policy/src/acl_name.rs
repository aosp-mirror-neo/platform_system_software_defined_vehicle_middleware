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

//! Represent ACL Name type.

/// Rust type for the name of an Access Control List (ACL) as defined in the ACL policy.
#[repr(transparent)]
#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct AclName(pub String);

impl<S: Into<String>> From<S> for AclName {
    fn from(value: S) -> Self {
        Self(value.into())
    }
}
