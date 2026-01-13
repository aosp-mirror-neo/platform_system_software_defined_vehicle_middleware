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

//! Service Identity Names module provides a collection of types to represent
//! the Service identity names.

mod channel_name;

mod instance_name;

mod package_name;

mod package_namespace;

mod service_name;

mod service_unit_name;

mod unit_type_name;

mod vm_name;

pub use channel_name::ChannelName;

pub use instance_name::InstanceName;

pub use package_name::PackageName;
pub use package_namespace::PackageNamespace;

pub use service_name::ServiceName;

pub use service_unit_name::ServiceUnitName;

pub use unit_type_name::UnitTypeName;

pub use vm_name::VmName;
