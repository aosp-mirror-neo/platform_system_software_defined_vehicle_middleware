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

//! Crate for SingleVmAclPolicyProvider that reads ACLs from textproto files

pub mod acl_file_provider;
pub use acl_file_provider::{AclFileMapping, AclFileProvider, VmAclFileMapping};

mod acl_merger;
mod caching_acl_reader;

mod caching_acl_file_provider;
pub use caching_acl_file_provider::CachingAclFileProvider;

mod read_mappings_from_directory;
pub(crate) use read_mappings_from_directory::read_acl_file_mappings_from_directory;
pub use read_mappings_from_directory::read_vm_file_mappings_from_directory;

mod build_acl_provider_from_directories;
pub use build_acl_provider_from_directories::build_acl_provider_from_directories;
