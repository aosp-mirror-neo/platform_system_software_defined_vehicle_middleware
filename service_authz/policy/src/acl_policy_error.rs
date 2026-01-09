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

//! Represent ACL policy errors.

use thiserror::Error;

use crate::acl_name::AclName;
use crate::acl_statement_str::AclStatementStr;
use crate::fqin_statement::FqinStatementParsingError;

/// Error that can arise during PolicyCollection creation.
#[derive(Error, Debug, PartialEq)]
pub enum ParseAclsError {
    /// Indicates error during ACL parsing.
    ///
    /// `protobuf::text_format::ParseError` does not support PartialEq, so
    ///  encode that error as a `String`.
    #[error("failed to parse ACL policy: {0}")]
    ParseAclPolicyError(String),
    /// Indicates error during ACL statement validation.
    #[error("failed to parse ACL statement {0} from {1:?} ACL list: {2}")]
    ValidateAclParsingError(String, AclName, FqinStatementParsingError),
    /// Indicates that the ACL Name appears at least twice in the ACL Policy
    #[error("ACL policy contains ACLs with the same name: {0:?}")]
    DuplicateAclName(AclName),
    /// Indicates that `never_include` statement specifies more than just
    /// the VM name, which is not allowed.
    #[error("ACL list {0:?} contains never_include statement {1:?} which specifies more than just the VM name, which is not allowed")]
    NeverIncludeSpecificationError(AclName, AclStatementStr),
}
