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

//! Service Identity Names Validation module provides a collection of types to validate
//! the Service identity names

pub mod alphanumhyphen_validation;

pub mod instance_name_validation;

pub mod package_identifier_validation;

pub mod service_name_validation;

pub mod service_unit_name_validation;

pub mod unit_type_name_validation;

pub mod vm_name_validation;

pub use alphanumhyphen_validation::AlphanumHyphenValidationError;

pub use instance_name_validation::{
    validate_instance_name, InstanceNameValidationError, INSTANCE_NAME_MAX_LENGTH_BYTES,
};

pub use package_identifier_validation::PACKAGE_IDENTIFIER_MAX_LENGTH_BYTES as PACKAGE_NAME_MAX_LENGTH_BYTES;
pub use package_identifier_validation::PACKAGE_IDENTIFIER_MAX_LENGTH_BYTES as PACKAGE_NAMESPACE_MAX_LENGTH_BYTES;
pub use package_identifier_validation::{
    validate_package_name, validate_package_namespace, PackageIdentifierValidationError,
    PackageNameValidationError, PackageNamespaceValidationError,
};

pub use service_name_validation::{
    validate_service_name, ServiceNameValidationError, SERVICE_NAME_MAX_LENGTH_BYTES,
};

pub use service_unit_name_validation::{
    validate_service_unit_name, ServiceUnitNameValidationError, SERVICE_UNIT_NAME_MAX_LENGTH_BYTES,
};

pub use unit_type_name_validation::{
    validate_unit_type_name, UnitTypeNameValidationError, UNIT_TYPE_NAME_MAX_LENGTH_BYTES,
};

pub use vm_name_validation::{validate_vm_name, VmNameValidationError, VM_NAME_MAX_LENGTH_BYTES};
