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

//! Represent public interface related types of Authz policy crate.

use sdv_comms_id::names::VmName;
use sdv_comms_id::ServiceFqin;

use std::str::FromStr;

use crate::acl_name::AclName;
use crate::acl_policy_error::ParseAclsError;
use crate::acl_statement_str::AclStatementStr;
use crate::fqin_set::{FqinSet, FqinSetsIntersectionResult};
use crate::fqin_statement::{has_vm_name_only_specified, FqinStatement};
use crate::instance_name_part::InstanceNamePart;
use crate::package_name_part::PackageNamePart;
use crate::selector::StarSelector;
use crate::selector_or_constant_value::SelectorOrConstantValue;
use crate::service_name_part::ServiceNamePart;
use crate::vm_name_part::VmNamePart;

/// Acl contains Access Control List information.
#[derive(Debug, Clone)]
pub struct Acl {
    acl_name: AclName,
    include: FqinSet,
    never_include: FqinSet,
}

impl Acl {
    /// Creates an ACL from a list of `include` and `never_include` FQIN statements.
    pub fn new(
        acl_name: &AclName,
        include: &[String],
        never_include: &[String],
    ) -> Result<Acl, ParseAclsError> {
        fn parse_fqin_set(
            acl_name: &AclName,
            statement_list: &[String],
        ) -> Result<FqinSet, ParseAclsError> {
            let parsed_list: Result<Vec<FqinStatement>, ParseAclsError> =
                statement_list.iter().map(|x| parse_fqin_statement(acl_name, x)).collect();

            Ok(FqinSet::from_fqin_statements(parsed_list?))
        }

        fn parse_fqin_statement(
            acl_name: &AclName,
            statement: &str,
        ) -> Result<FqinStatement, ParseAclsError> {
            FqinStatement::from_str(statement).map_err(|err| {
                ParseAclsError::ValidateAclParsingError(
                    statement.to_string(),
                    acl_name.clone(),
                    err,
                )
            })
        }

        let include_set = parse_fqin_set(acl_name, include)?;
        let never_include_set = parse_fqin_set(acl_name, never_include)?;

        // `never_include` should contain only statements which specify VM name only.
        //
        // For example, `vm-name:*` is allowed, but `com.sdv.sample`,
        // `vm-name:com.sdv.sample`, `vm-name:com.sdv.sample.WindowManager` are not allowed.
        for never_include_st in never_include_set.iter() {
            if !has_vm_name_only_specified(never_include_st) {
                return Err(ParseAclsError::NeverIncludeSpecificationError(
                    acl_name.clone(),
                    never_include_st.clone().into(),
                ));
            }
        }

        Ok(Acl {
            acl_name: acl_name.clone(),
            include: include_set,
            never_include: never_include_set,
        })
    }
}

/// Represents the result of an access check.
#[derive(Debug, PartialEq)]
pub enum AccessState {
    /// Explicitly allowed and not forbidden.
    Permitted(AclName, AclStatementStr),
    /// Explicitly forbidden.
    ExplicitlyDenied(AclName, AclStatementStr),
    /// No matching rule (default deny).
    ImplicitlyDenied,
}

/// Contains a list of ACLs that guard an Object and decides whether entities can
/// access the guarded Object.
#[derive(Debug)]
pub struct ObjectAuthzGuard {
    acls: Vec<Acl>,
}

impl ObjectAuthzGuard {
    /// Creates ObjectAuthzGuard for the object guarded by a list of ACLs.
    pub fn new(acls: &[Acl]) -> ObjectAuthzGuard {
        ObjectAuthzGuard { acls: acls.to_vec() }
    }

    /// Checks whether FQIN target has access to the current Object.
    ///
    /// FQIN has access to the Object if both are valid:
    /// 1. There is at least one `include` statement that matches (has intersection)
    ///    the FQIN target.
    /// 2. There is no `never_include` statement that matches (has intersection) the FQIN target.
    pub fn check_fqin_access(&self, target: &ServiceFqin) -> AccessState {
        let target_set = FqinSet::from_fqin(target);

        // `never_include` overrides `include` statements, so check whether
        // access is explicitly denied first.
        for acl in &self.acls {
            if let FqinSetsIntersectionResult::HasIntersection(_, statement) =
                target_set.has_intersection(&acl.never_include)
            {
                return AccessState::ExplicitlyDenied(acl.acl_name.clone(), statement);
            }
        }

        // Check whether access is explicitly granted.
        for acl in &self.acls {
            if let FqinSetsIntersectionResult::HasIntersection(_, statement) =
                target_set.has_intersection(&acl.include)
            {
                return AccessState::Permitted(acl.acl_name.clone(), statement);
            }
        }

        AccessState::ImplicitlyDenied
    }

    /// Checks whether VM has access to the current Object.
    ///
    /// VM has access to the Object if both are valid:
    /// 1. There is at least one FQIN with the target VM name that matches at least one
    ///    `include` statement.
    ///    In other terms, there is a non-empty intersection between FQIN set described by
    ///    `{target_vm_name}:*` statement and FQIN set described by `include` statements.
    /// 2. There is no `never_include` statement that denies access to all FQIN from the target VM.
    ///    In other terms, there is no `{target_vm_name}:*` in `never_include` statements.
    pub fn check_vm_access(&self, target_vm_name: &VmName) -> AccessState {
        let target_statement = FqinStatement::new(
            VmNamePart(SelectorOrConstantValue::ConstantValue(target_vm_name.name().into())),
            PackageNamePart::StarSelector(StarSelector),
            ServiceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
            InstanceNamePart(SelectorOrConstantValue::StarSelector(StarSelector)),
        );

        // `never_include` overrides `include` statements, so check whether
        // access is explicitly denied first.
        //
        // NOTE: This is different from the `never_include` check in the `check_fqin_access()`
        // function since we want to check that the set contains exactly `never_include vm-name:*`,
        // whereas the `check_fqin_access()` function wants to check for the entire FQIN. Due to
        // the semantics of the `has_intersection()` function it is not possible to use it for
        // checking existence of this exact statement.
        for acl in &self.acls {
            for statement in acl.never_include.iter() {
                if *statement == target_statement {
                    return AccessState::ExplicitlyDenied(
                        acl.acl_name.clone(),
                        target_statement.into(),
                    );
                }
            }
        }

        // Check whether access is explicitly granted.
        let target_set = FqinSet::from_fqin_statements(vec![target_statement]);
        for acl in &self.acls {
            if let FqinSetsIntersectionResult::HasIntersection(_, statement) =
                target_set.has_intersection(&acl.include)
            {
                return AccessState::Permitted(acl.acl_name.clone(), statement);
            }
        }

        AccessState::ImplicitlyDenied
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::fqin_statement::FqinStatementParsingError;

    #[test]
    fn test_acl_new_failure_invalid_acl_statement() {
        let err =
            Acl::new(&"TirePressureSubs".into(), &["vm-cdc:*".into()], &["".into()]).unwrap_err();

        assert_eq!(
            err,
            ParseAclsError::ValidateAclParsingError(
                "".into(),
                "TirePressureSubs".into(),
                FqinStatementParsingError::IsEmpty
            )
        );
    }

    #[test]
    fn test_acl_new_failure_never_include_specification() {
        let err = Acl::new(
            &"TirePressureSubs".into(),
            &["vm-cdc:*".into(), "vm-telemetry:*".into()],
            &["com.google.sdv.*".into()],
        )
        .unwrap_err();

        assert_eq!(
            err,
            ParseAclsError::NeverIncludeSpecificationError(
                "TirePressureSubs".into(),
                "com.google.sdv.*".into()
            )
        );
    }

    fn create_object_authz_guard_for_testing() -> ObjectAuthzGuard {
        let acls1 = Acl::new(
            &"TirePressureSubs".into(),
            &["vm-cdc:*".into(), "vm-telemetry:*".into()],
            &["vm-ivi:*".into()],
        )
        .unwrap();

        let acls2 = Acl::new(
            &"OemApiClients".into(),
            &["vm-telemetry:com.google.android.ivi.datatunnel.*".into()],
            &["vm-cdc:*".into()],
        )
        .unwrap();

        ObjectAuthzGuard::new(&[acls1, acls2])
    }

    #[test]
    fn test_check_fqin_access_permitted() {
        let object_authz_guard = create_object_authz_guard_for_testing();

        let target_fqin = ServiceFqin::from_str(
            "vm-telemetry:com.google.android.ivi.datatunnel.TunnelService/main",
        )
        .unwrap();
        let access_state = object_authz_guard.check_fqin_access(&target_fqin);
        assert!(
            matches!(access_state, AccessState::Permitted(_, _)),
            "Unexpected access state: {access_state:?}"
        );
    }

    #[test]
    fn test_check_fqin_access_explicitly_denied() {
        let object_authz_guard = create_object_authz_guard_for_testing();

        // Explicitly denied by `never_include: vm-cdc:*` even though allowed
        // by `include: vm-cdc:*`.
        let target_fqin = ServiceFqin::from_str("vm-cdc:com.oem.Ui/center").unwrap();
        let access_state = object_authz_guard.check_fqin_access(&target_fqin);
        assert!(
            matches!(access_state, AccessState::ExplicitlyDenied(_, _)),
            "Unexpected access state: {access_state:?}"
        );
    }

    #[test]
    fn test_check_fqin_access_implicitly_denied() {
        let object_authz_guard = create_object_authz_guard_for_testing();

        let target_fqin = ServiceFqin::from_str("vm:com.sdv.Telemetry/left-right").unwrap();
        let access_state = object_authz_guard.check_fqin_access(&target_fqin);
        assert_eq!(
            access_state,
            AccessState::ImplicitlyDenied,
            "Unexpected access state: {access_state:?}"
        );
    }

    #[test]
    fn test_check_vm_access_permitted() {
        let object_authz_guard = create_object_authz_guard_for_testing();

        let target_vm = VmName::from_str("vm-telemetry").unwrap();
        let access_state = object_authz_guard.check_vm_access(&target_vm);
        assert!(
            matches!(access_state, AccessState::Permitted(_, _)),
            "Unexpected access state: {access_state:?}"
        );
    }

    #[test]
    fn test_check_vm_access_explicitly_denied() {
        let object_authz_guard = create_object_authz_guard_for_testing();

        // Explicitly denied by `never_include: com.oem.*` even though allowed
        // by `include: vm-cdc:*`.
        let target_vm = VmName::from_str("vm-ivi").unwrap();
        let access_state = object_authz_guard.check_vm_access(&target_vm);
        assert!(
            matches!(access_state, AccessState::ExplicitlyDenied(_, _)),
            "Unexpected access state: {access_state:?}"
        );
    }

    #[test]
    fn test_check_vm_access_implicitly_denied() {
        let object_authz_guard = create_object_authz_guard_for_testing();

        let target_vm = VmName::from_str("vm").unwrap();
        let access_state = object_authz_guard.check_vm_access(&target_vm);
        assert_eq!(
            access_state,
            AccessState::ImplicitlyDenied,
            "Unexpected access state: {access_state:?}"
        );
    }
}
