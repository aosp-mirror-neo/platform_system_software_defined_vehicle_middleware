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

//! Represent ACL Statement type.

use crate::fqin_statement::FqinStatement;

/// Encapsulates a human readable string for an ACL statement useful for error reporting and
/// debugging.
#[repr(transparent)]
#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub struct AclStatementStr(pub String);

impl From<FqinStatement> for AclStatementStr {
    fn from(value: FqinStatement) -> Self {
        Self(value.to_string())
    }
}

/// This conversion is only allowed in tests for convenience.
#[cfg(test)]
impl<S: Into<String>> From<S> for AclStatementStr {
    fn from(value: S) -> Self {
        Self(value.into())
    }
}
