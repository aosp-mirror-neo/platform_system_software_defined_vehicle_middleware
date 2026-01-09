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

//! Represents ACL constant value type used to define ACL statements.

use sdv_comms_id::ServiceFqin;
use sdv_comms_id_validator::{validate_package_namespace, PackageNamespaceValidationError};
use thiserror::Error;

use std::fmt;
use std::str::FromStr;

use crate::instance_name_part::{InstanceNamePart, InstanceNamePartParsingError};
use crate::package_name_part::{PackageNamePart, PackageNamePartParsingError};
use crate::selector::StarSelector;
use crate::selector_or_constant_value::SelectorOrConstantValue;
use crate::service_name_part::{ServiceNamePart, ServiceNamePartParsingError};
use crate::vm_name_part::{VmNamePart, VmNamePartParsingError};

/// FqinStatement is used to describe a Set of FQINs which are matching
/// underlying FQIN part rules.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) struct FqinStatement {
    vm_name: VmNamePart,
    package_name: PackageNamePart,
    service_name: ServiceNamePart,
    instance_name: InstanceNamePart,
}

impl FqinStatement {
    pub fn new(
        vm_name: VmNamePart,
        package_name: PackageNamePart,
        service_name: ServiceNamePart,
        instance_name: InstanceNamePart,
    ) -> FqinStatement {
        FqinStatement { vm_name, package_name, service_name, instance_name }
    }

    pub fn from_fqin(fqin: &ServiceFqin) -> FqinStatement {
        FqinStatement {
            vm_name: VmNamePart(SelectorOrConstantValue::ConstantValue(
                fqin.vm_name().name().into(),
            )),
            package_name: PackageNamePart::ConstantValue(fqin.package_name().name().into()),
            service_name: ServiceNamePart(SelectorOrConstantValue::ConstantValue(
                fqin.service_name().name().into(),
            )),
            instance_name: InstanceNamePart(SelectorOrConstantValue::ConstantValue(
                fqin.instance_name().name().into(),
            )),
        }
    }

    pub fn has_intersection(&self, other: &FqinStatement) -> bool {
        self.vm_name.0.has_intersection(&other.vm_name.0)
            && self.package_name.has_intersection(&other.package_name)
            && self.service_name.0.has_intersection(&other.service_name.0)
            && self.instance_name.0.has_intersection(&other.instance_name.0)
    }
}

/// Returns whether FQIN statement has VM name specified as a constant
/// value, and other parts of the FQIN statements are Star selectors.
pub(crate) fn has_vm_name_only_specified(fqin_statement: &FqinStatement) -> bool {
    matches!(fqin_statement.vm_name.0, SelectorOrConstantValue::ConstantValue(_))
        && matches!(fqin_statement.package_name, PackageNamePart::StarSelector(StarSelector))
        && matches!(
            fqin_statement.service_name.0,
            SelectorOrConstantValue::StarSelector(StarSelector)
        )
        && matches!(
            fqin_statement.instance_name.0,
            SelectorOrConstantValue::StarSelector(StarSelector)
        )
}

/// Error that can arise during FQIN statement parsing.
#[derive(Error, Debug, PartialEq)]
pub enum FqinStatementParsingError {
    /// Indicates that FQIN statement is empty.
    #[error("should not be empty")]
    IsEmpty,
    /// Indicates error occurred during parsing VM name part of FQIN statement.
    #[error("VM name part is neither selector nor constant value: {0}")]
    InvalidVmNamePart(#[from] VmNamePartParsingError),
    /// Indicates error occurred during parsing Package name part of FQIN
    /// statement.
    #[error("Package name part is neither selector nor constant value nor package namespace: {0}")]
    InvalidPackageNamePart(#[from] PackageNamePartParsingError),
    /// Indicates error occurred during parsing Service name part of FQIN
    /// statement.
    #[error("Service name part is neither selector nor constant value: {0}")]
    InvalidServiceNamePart(#[from] ServiceNamePartParsingError),
    /// Indicates error occurred during parsing Instance name part of FQIN
    /// statement.
    #[error("Instance name part is neither selector nor constant value: {0}")]
    InvalidInstanceNamePart(#[from] InstanceNamePartParsingError),
    /// Indicates that FQIN statement should not specify instance name part.
    #[error("should not specify instance name part")]
    InstanceNameShouldNotBeSpecified,
    /// Indicates that FQIN statement should not specify service name part.
    #[error("should not specify service name part")]
    ServiceNameShouldNotBeSpecified,
}

impl FromStr for FqinStatement {
    type Err = FqinStatementParsingError;

    fn from_str(mut s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(FqinStatementParsingError::IsEmpty);
        }

        let vm_name = extract_vm_name(&mut s)?;
        let (instance_name, instance_name_specified) = extract_instance_name(&mut s)?;
        let (service_name, service_name_specified) = extract_service_name(&mut s)?;
        let package_name = extract_package_name(s)?;

        if !matches!(service_name.0, SelectorOrConstantValue::ConstantValue(_))
            && instance_name_specified
        {
            return Err(FqinStatementParsingError::InstanceNameShouldNotBeSpecified);
        }

        if !matches!(package_name, PackageNamePart::ConstantValue(_)) {
            if instance_name_specified {
                return Err(FqinStatementParsingError::InstanceNameShouldNotBeSpecified);
            }
            if service_name_specified {
                return Err(FqinStatementParsingError::ServiceNameShouldNotBeSpecified);
            }
        }

        Ok(FqinStatement { vm_name, package_name, service_name, instance_name })
    }
}

fn extract_vm_name(s: &mut &str) -> Result<VmNamePart, FqinStatementParsingError> {
    match s.find(':') {
        Some(index) => {
            let part = &s[..index];
            *s = &s[index + 1..];
            Ok(VmNamePart::from_str(part)?)
        }
        None => Ok(VmNamePart(SelectorOrConstantValue::StarSelector(StarSelector))),
    }
}

fn extract_instance_name(
    s: &mut &str,
) -> Result<(InstanceNamePart, bool), FqinStatementParsingError> {
    Ok(match s.rfind('/') {
        Some(index) => {
            let part = &s[index + 1..];
            *s = &s[..index];
            (InstanceNamePart::from_str(part)?, true)
        }
        None => (InstanceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)), false),
    })
}

fn extract_service_name(
    s: &mut &str,
) -> Result<(ServiceNamePart, bool), FqinStatementParsingError> {
    match s.rfind('.') {
        Some(index) => {
            let part = &s[index + 1..];

            match ServiceNamePart::from_str(part) {
                Ok(service_name_part) => {
                    let offset: usize = {
                        if let SelectorOrConstantValue::StarSelector(StarSelector) =
                            service_name_part.0
                        {
                            // Here we have `s` ending with ".*" which indicates a namespace.
                            s.len()
                        } else {
                            index
                        }
                    };
                    // Figure out whether service_name is specified before updating `s`.
                    let service_name_specified = offset != s.len();
                    *s = &s[..offset];

                    Ok((service_name_part, service_name_specified))
                }

                Err(err) => {
                    // `ServiceName` might not be specified and thus `part` could be part
                    // of the package name.
                    //
                    // Package name segment is a valid package namespace. Let's check if `part`
                    // is a valid package namespace, otherwise return original error related
                    // to the Service Name part parsing.
                    if let Err(PackageNamespaceValidationError(_)) =
                        validate_package_namespace(part)
                    {
                        return Err(FqinStatementParsingError::InvalidServiceNamePart(err));
                    }

                    Ok((
                        ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
                        false,
                    ))
                }
            }
        }
        None => Ok((ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)), false)),
    }
}

fn extract_package_name(s: &str) -> Result<PackageNamePart, FqinStatementParsingError> {
    Ok(PackageNamePart::from_str(s)?)
}

impl fmt::Display for FqinStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // VM name part should be dropped if it is `*`.
        let vm_name_part = {
            let vm_name = self.vm_name.to_string();
            if vm_name == "*" {
                String::new()
            } else {
                vm_name + ":"
            }
        };

        // Unlike other parts, package name part should not be dropped even if it is `*`.
        let package_name_part = self.package_name.to_string();

        // Service name part should be dropped if it is `*`.
        let service_name_part = {
            let service_name = self.service_name.to_string();
            if service_name == "*" {
                String::new()
            } else {
                String::from(".") + &service_name
            }
        };

        // Instance name part should be dropped if it is `*`.
        let instance_name_part = {
            let instance_name = self.instance_name.to_string();
            if instance_name == "*" {
                String::new()
            } else {
                String::from("/") + &instance_name
            }
        };

        write!(f, "{vm_name_part}{package_name_part}{service_name_part}{instance_name_part}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_intersection_true() {
        let statement1 = FqinStatement::from_str("com.google.sdv.WindowManager").unwrap();
        let statement2 = FqinStatement::from_str("vm-front-left:*").unwrap();

        assert!(statement1.has_intersection(&statement2));
        assert!(statement2.has_intersection(&statement1));
    }

    #[test]
    fn test_has_intersection_false_all_parts_not_matching() {
        let statement1 = FqinStatement {
            vm_name: VmNamePart(SelectorOrConstantValue::ConstantValue("vm".into())),
            package_name: PackageNamePart::ConstantValue("com.google.sdv".into()),
            service_name: ServiceNamePart(SelectorOrConstantValue::ConstantValue(
                "WindowManager".into(),
            )),
            instance_name: InstanceNamePart(SelectorOrConstantValue::ConstantValue("main".into())),
        };

        let statement2 = FqinStatement {
            vm_name: VmNamePart(SelectorOrConstantValue::ConstantValue("vm-ivi".into())),
            package_name: PackageNamePart::ConstantValue("com.oem.sdv".into()),
            service_name: ServiceNamePart(SelectorOrConstantValue::ConstantValue(
                "TirePressure".into(),
            )),
            instance_name: InstanceNamePart(SelectorOrConstantValue::ConstantValue(
                "default".into(),
            )),
        };

        assert!(!statement1.has_intersection(&statement2));
        assert!(!statement2.has_intersection(&statement1));
    }

    #[test]
    fn test_has_intersection_false_vm_name_not_matching() {
        let statement1 = FqinStatement {
            vm_name: VmNamePart(SelectorOrConstantValue::ConstantValue("vm-1".into())),
            package_name: PackageNamePart::StarSelector(StarSelector),
            service_name: ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
            instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
        };
        let statement2 = FqinStatement {
            vm_name: VmNamePart(SelectorOrConstantValue::ConstantValue("vm-2".into())),
            package_name: PackageNamePart::StarSelector(StarSelector),
            service_name: ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
            instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
        };

        assert!(!statement1.has_intersection(&statement2));
        assert!(!statement2.has_intersection(&statement1));
    }

    #[test]
    fn test_has_intersection_false_package_name_not_matching() {
        let statement1 = FqinStatement {
            vm_name: VmNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
            package_name: PackageNamePart::ConstantValue("com.oem1.sdv".into()),
            service_name: ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
            instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
        };
        let statement2 = FqinStatement {
            vm_name: VmNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
            package_name: PackageNamePart::ConstantValue("com.oem2.sdv".into()),
            service_name: ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
            instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
        };

        assert!(!statement1.has_intersection(&statement2));
        assert!(!statement2.has_intersection(&statement1));
    }

    #[test]
    fn test_has_intersection_false_service_name_not_matching() {
        let statement1 = FqinStatement {
            vm_name: VmNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
            package_name: PackageNamePart::StarSelector(StarSelector),
            service_name: ServiceNamePart(SelectorOrConstantValue::ConstantValue(
                "WindowManager".into(),
            )),
            instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
        };
        let statement2 = FqinStatement {
            vm_name: VmNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
            package_name: PackageNamePart::StarSelector(StarSelector),
            service_name: ServiceNamePart(SelectorOrConstantValue::ConstantValue(
                "TirePressure".into(),
            )),
            instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
        };

        assert!(!statement1.has_intersection(&statement2));
        assert!(!statement2.has_intersection(&statement1));
    }

    #[test]
    fn test_has_intersection_false_instance_name_not_matching() {
        let statement1 = FqinStatement {
            vm_name: VmNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
            package_name: PackageNamePart::StarSelector(StarSelector),
            service_name: ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
            instance_name: InstanceNamePart(SelectorOrConstantValue::ConstantValue("main".into())),
        };
        let statement2 = FqinStatement {
            vm_name: VmNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
            package_name: PackageNamePart::StarSelector(StarSelector),
            service_name: ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
            instance_name: InstanceNamePart(SelectorOrConstantValue::ConstantValue(
                "default".into(),
            )),
        };

        assert!(!statement1.has_intersection(&statement2));
        assert!(!statement2.has_intersection(&statement1));
    }

    #[test]
    fn test_from_fqin() {
        let fqin = ServiceFqin::from_str("vm:com.google.sdv.WindowManager/main").unwrap();
        let statement = FqinStatement::from_fqin(&fqin);
        assert_eq!(statement.to_string(), "vm:com.google.sdv.WindowManager/main");
        assert_eq!(
            statement,
            FqinStatement {
                vm_name: VmNamePart(SelectorOrConstantValue::ConstantValue("vm".into())),
                package_name: PackageNamePart::ConstantValue("com.google.sdv".into()),
                service_name: ServiceNamePart(SelectorOrConstantValue::ConstantValue(
                    "WindowManager".into(),
                )),
                instance_name: InstanceNamePart(SelectorOrConstantValue::ConstantValue(
                    "main".into(),
                )),
            }
        );
    }

    #[test]
    fn test_from_str_success_all_constant_values() {
        let statement = FqinStatement::from_str("vm:com.google.sdv.WindowManager/main").unwrap();
        assert_eq!(statement.to_string(), "vm:com.google.sdv.WindowManager/main");
        assert_eq!(
            statement,
            FqinStatement {
                vm_name: VmNamePart(SelectorOrConstantValue::ConstantValue("vm".into())),
                package_name: PackageNamePart::ConstantValue("com.google.sdv".into()),
                service_name: ServiceNamePart(SelectorOrConstantValue::ConstantValue(
                    "WindowManager".into()
                )),
                instance_name: InstanceNamePart(SelectorOrConstantValue::ConstantValue(
                    "main".into()
                )),
            }
        );
    }

    #[test]
    fn test_from_str_success_instance_name_is_explicit_selector() {
        let statement = FqinStatement::from_str("vm:com.google.sdv.WindowManager/*").unwrap();
        assert_eq!(statement.to_string(), "vm:com.google.sdv.WindowManager");
        assert_eq!(
            statement,
            FqinStatement {
                vm_name: VmNamePart(SelectorOrConstantValue::ConstantValue("vm".into())),
                package_name: PackageNamePart::ConstantValue("com.google.sdv".into()),
                service_name: ServiceNamePart(SelectorOrConstantValue::ConstantValue(
                    "WindowManager".into()
                )),
                instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(
                    StarSelector
                )),
            }
        );
    }

    #[test]
    fn test_from_str_success_instance_name_is_implicit_selector() {
        let statement = FqinStatement::from_str("vm:com.google.sdv.WindowManager").unwrap();
        assert_eq!(statement.to_string(), "vm:com.google.sdv.WindowManager");
        assert_eq!(
            statement,
            FqinStatement {
                vm_name: VmNamePart(SelectorOrConstantValue::ConstantValue("vm".into())),
                package_name: PackageNamePart::ConstantValue("com.google.sdv".into()),
                service_name: ServiceNamePart(SelectorOrConstantValue::ConstantValue(
                    "WindowManager".into()
                )),
                instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(
                    StarSelector
                )),
            }
        );
    }

    #[test]
    fn test_from_str_success_vm_name_is_explicit_selector() {
        let statement = FqinStatement::from_str("*:com.google.sdv.WindowManager/main").unwrap();
        assert_eq!(statement.to_string(), "com.google.sdv.WindowManager/main");
        assert_eq!(
            statement,
            FqinStatement {
                vm_name: VmNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
                package_name: PackageNamePart::ConstantValue("com.google.sdv".into()),
                service_name: ServiceNamePart(SelectorOrConstantValue::ConstantValue(
                    "WindowManager".into()
                )),
                instance_name: InstanceNamePart(SelectorOrConstantValue::ConstantValue(
                    "main".into()
                )),
            }
        );
    }

    #[test]
    fn test_from_str_success_vm_name_is_implicit_selector() {
        let statement = FqinStatement::from_str("com.google.sdv.WindowManager/main").unwrap();
        assert_eq!(statement.to_string(), "com.google.sdv.WindowManager/main");
        assert_eq!(
            statement,
            FqinStatement {
                vm_name: VmNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
                package_name: PackageNamePart::ConstantValue("com.google.sdv".into()),
                service_name: ServiceNamePart(SelectorOrConstantValue::ConstantValue(
                    "WindowManager".into()
                )),
                instance_name: InstanceNamePart(SelectorOrConstantValue::ConstantValue(
                    "main".into()
                )),
            }
        );
    }

    #[test]
    fn test_from_str_success_service_name_is_explicit_selector() {
        let statement = FqinStatement::from_str("vm:com.google.sdv.*").unwrap();
        assert_eq!(statement.to_string(), "vm:com.google.sdv.*");
        assert_eq!(
            statement,
            FqinStatement {
                vm_name: VmNamePart(SelectorOrConstantValue::ConstantValue("vm".into())),
                package_name: PackageNamePart::PackageNamespace("com.google.sdv".into()),
                service_name: ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
                instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(
                    StarSelector
                )),
            }
        );
    }

    #[test]
    fn test_from_str_success_service_name_is_implicit_selector() {
        let statement = FqinStatement::from_str("vm:com.google.sdv").unwrap();
        assert_eq!(statement.to_string(), "vm:com.google.sdv");
        assert_eq!(
            statement,
            FqinStatement {
                vm_name: VmNamePart(SelectorOrConstantValue::ConstantValue("vm".into())),
                package_name: PackageNamePart::ConstantValue("com.google.sdv".into()),
                service_name: ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
                instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(
                    StarSelector
                )),
            }
        );
    }

    #[test]
    fn test_from_str_success_package_namespace() {
        let statement = FqinStatement::from_str("vm:com.google.sdv.*").unwrap();
        assert_eq!(statement.to_string(), "vm:com.google.sdv.*");
        assert_eq!(
            statement,
            FqinStatement {
                vm_name: VmNamePart(SelectorOrConstantValue::ConstantValue("vm".into())),
                package_name: PackageNamePart::PackageNamespace("com.google.sdv".into()),
                service_name: ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
                instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(
                    StarSelector
                )),
            }
        );
    }

    #[test]
    fn test_from_str_success_only_explicit_vm_name() {
        let statement = FqinStatement::from_str("vm:*").unwrap();
        assert_eq!(statement.to_string(), "vm:*");
        assert_eq!(
            statement,
            FqinStatement {
                vm_name: VmNamePart(SelectorOrConstantValue::ConstantValue("vm".into())),
                package_name: PackageNamePart::StarSelector(StarSelector),
                service_name: ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
                instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(
                    StarSelector
                )),
            }
        );
    }

    #[test]
    fn test_from_str_success_only_explicit_package_name() {
        let statement = FqinStatement::from_str("com.google.sdv").unwrap();
        assert_eq!(statement.to_string(), "com.google.sdv");
        assert_eq!(
            statement,
            FqinStatement {
                vm_name: VmNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
                package_name: PackageNamePart::ConstantValue("com.google.sdv".into()),
                service_name: ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
                instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(
                    StarSelector
                )),
            }
        );
    }

    #[test]
    fn test_from_str_success_only_explicit_package_namespace() {
        let statement = FqinStatement::from_str("com.google.sdv.*").unwrap();
        assert_eq!(statement.to_string(), "com.google.sdv.*");
        assert_eq!(
            statement,
            FqinStatement {
                vm_name: VmNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
                package_name: PackageNamePart::PackageNamespace("com.google.sdv".into()),
                service_name: ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
                instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(
                    StarSelector
                )),
            }
        );
    }

    #[test]
    fn test_from_str_success_everything_is_allowed() {
        for s in ["*", "*:*"] {
            let statement = FqinStatement::from_str(s).unwrap();
            assert_eq!(statement.to_string(), "*");
            assert_eq!(
                statement,
                FqinStatement {
                    vm_name: VmNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
                    package_name: PackageNamePart::StarSelector(StarSelector),
                    service_name: ServiceNamePart(SelectorOrConstantValue::StarSelector(
                        StarSelector
                    )),
                    instance_name: InstanceNamePart(SelectorOrConstantValue::StarSelector(
                        StarSelector
                    )),
                }
            );
        }
    }

    #[test]
    fn test_from_str_failure_is_empty() {
        let err = FqinStatement::from_str("").unwrap_err();
        assert_eq!(err, FqinStatementParsingError::IsEmpty);
    }

    #[test]
    fn test_from_str_failure_invalid_vm_name_part() {
        let err = FqinStatement::from_str("vm*:com.google.sdv").unwrap_err();
        assert!(
            matches!(err, FqinStatementParsingError::InvalidVmNamePart(_)),
            "Unexpected error: {err}"
        );
    }

    #[test]
    fn test_from_str_failure_invalid_package_name_part() {
        for s in [
            // Empty package name part.
            "vm:",
            // Selector inside package name part.
            "vm:com.google.*.sdv",
            // Selector service name and package namespace.
            "vm:com.google.*.*",
            // Empty package namespace.
            ".*",
            // 2 stars in the package namespace.
            "*:*.*",
            // 2 stars in the package namespace and instance name is star.
            "*:*.*/*",
        ] {
            let err = FqinStatement::from_str(s).unwrap_err();
            assert!(
                matches!(err, FqinStatementParsingError::InvalidPackageNamePart(_)),
                "Unexpected error during parsing {s}: {err}"
            );
        }
    }

    #[test]
    fn test_from_str_failure_invalid_service_name_part() {
        let err = FqinStatement::from_str("vm:com.google.sdv.Window*").unwrap_err();
        // Even though `err` here could be treated as service name error, it also might be a treated as package name error because
        assert!(
            matches!(err, FqinStatementParsingError::InvalidServiceNamePart(_)),
            "Unexpected error: {err}"
        );
    }

    #[test]
    fn test_from_str_failure_invalid_instance_name_part() {
        let err = FqinStatement::from_str("vm:com.google.sdv.WindowManager/front*").unwrap_err();
        assert!(
            matches!(err, FqinStatementParsingError::InvalidInstanceNamePart(_)),
            "Unexpected error: {err}"
        );
    }

    #[test]
    fn test_from_str_failure_instance_name_should_not_be_specified() {
        for s in [
            // Constant instance name and package namespace.
            "vm:com.google.sdv.*/front",
            // Constant instance name and package name selector.
            "vm:*/front",
        ] {
            let err = FqinStatement::from_str(s).unwrap_err();
            assert_eq!(
                err,
                FqinStatementParsingError::InstanceNameShouldNotBeSpecified,
                "Unexpected error during parsing {s}: {err}"
            );
        }
    }

    #[test]
    fn test_from_str_failure_service_name_should_not_be_specified() {
        for s in [
            // Constant service name and package namespace.
            "vm:com.google.*.WindowManager",
            // Constant service name and package name selector.
            "vm:*.WindowManager",
        ] {
            let err = FqinStatement::from_str(s).unwrap_err();
            assert_eq!(
                err,
                FqinStatementParsingError::ServiceNameShouldNotBeSpecified,
                "Unexpected error during parsing {s}: {err}"
            );
        }
    }

    #[test]
    fn test_has_vm_name_only_specified_true() {
        let statement = FqinStatement::from_str("vm:*").unwrap();
        assert!(has_vm_name_only_specified(&statement));
    }

    #[test]
    fn test_has_vm_name_only_specified_false_vm_name_is_star_and_package_name_const() {
        let statement = FqinStatement::from_str("com.sdv.sample").unwrap();
        assert!(!has_vm_name_only_specified(&statement));
    }

    #[test]
    fn test_has_vm_name_only_specified_false_vm_name_is_star_and_package_namespace() {
        let statement = FqinStatement::from_str("com.sdv.sample.*").unwrap();
        assert!(!has_vm_name_only_specified(&statement));
    }

    #[test]
    fn test_has_vm_name_only_specified_false_package_name_is_const() {
        let statement = FqinStatement::from_str("vm:com.sdv.sample").unwrap();
        assert!(!has_vm_name_only_specified(&statement));
    }

    #[test]
    fn test_has_vm_name_only_specified_false_package_namespace() {
        let statement = FqinStatement::from_str("vm:com.sdv.sample.*").unwrap();
        assert!(!has_vm_name_only_specified(&statement));
    }

    #[test]
    fn test_has_vm_name_only_specified_false_bundle_name_is_const() {
        let statement = FqinStatement::from_str("vm:com.sdv.sample.WindowManager").unwrap();
        assert!(!has_vm_name_only_specified(&statement));
    }

    #[test]
    fn test_has_vm_name_only_specified_false_instance_name_is_const() {
        let statement = FqinStatement::from_str("vm:com.sdv.sample.WindowManager/main").unwrap();
        assert!(!has_vm_name_only_specified(&statement));
    }
}
