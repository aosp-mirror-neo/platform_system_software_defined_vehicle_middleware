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

//! Represents FQIN set represented by a list of ACL statements.

use itertools::iproduct;

use sdv_comms_id::ServiceFqin;

use crate::acl_statement_str::AclStatementStr;
use crate::fqin_statement::FqinStatement;

/// FqinSet is used to describe a Set of FQIN represented by a list of ACL statements.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) struct FqinSet {
    statements: Vec<FqinStatement>,
}

/// FqinSetsIntersectionResult contains a result of two FQIN Sets intersection.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) enum FqinSetsIntersectionResult {
    NoIntersection,
    HasIntersection(AclStatementStr, AclStatementStr),
}

impl FqinSet {
    pub fn from_fqin_statements(statements: Vec<FqinStatement>) -> FqinSet {
        FqinSet { statements }
    }

    pub fn from_fqin(fqin: &ServiceFqin) -> FqinSet {
        let statement = FqinStatement::from_fqin(fqin);
        FqinSet { statements: vec![statement] }
    }

    /// Returns whether there is an intersection between `FqinSet`s.
    ///
    /// If so, returns a pair of `FqinStatement`s whose intersection is
    /// non-empty. Note, FQINs that belong to both `FqinStatement`s are not
    /// returned.
    pub fn has_intersection(&self, other: &FqinSet) -> FqinSetsIntersectionResult {
        match iproduct!(&self.statements, &other.statements).find(|(x, y)| x.has_intersection(y)) {
            Some((self_st, other_st)) => FqinSetsIntersectionResult::HasIntersection(
                self_st.clone().into(),
                other_st.clone().into(),
            ),
            None => FqinSetsIntersectionResult::NoIntersection,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &FqinStatement> {
        self.statements.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr;

    #[test]
    fn test_from_fqin_statements() {
        let statement = FqinStatement::from_str("vm:com.google.sdv.WindowManager/main").unwrap();
        let set = FqinSet::from_fqin_statements(vec![statement.clone()]);
        assert_eq!(set, FqinSet { statements: vec![statement] });
    }

    #[test]
    fn test_from_fqin() {
        let set = {
            let fqin = ServiceFqin::from_str("vm:com.google.sdv.WindowManager/main").unwrap();
            FqinSet::from_fqin(&fqin)
        };

        let statement = FqinStatement::from_str("vm:com.google.sdv.WindowManager/main").unwrap();

        assert_eq!(set, FqinSet { statements: vec![statement] });
    }

    #[test]
    fn test_has_intersection_false() {
        let set1 = {
            let statement = FqinStatement::from_str("vm:com.google.ivi.*").unwrap();
            FqinSet::from_fqin_statements(vec![statement.clone()])
        };

        let set2 = {
            let fqin = ServiceFqin::from_str("vm:com.google.sdv.WindowManager/main").unwrap();
            FqinSet::from_fqin(&fqin)
        };

        assert_eq!(set1.has_intersection(&set2), FqinSetsIntersectionResult::NoIntersection);
        assert_eq!(set2.has_intersection(&set1), FqinSetsIntersectionResult::NoIntersection);
    }

    #[test]
    fn test_has_intersection_true() {
        let statement1 = FqinStatement::from_str("vm:com.google.*").unwrap();
        let set1 = FqinSet::from_fqin_statements(vec![statement1.clone()]);

        let statement2 = FqinStatement::from_str("ivi:*").unwrap();
        let statement3 = FqinStatement::from_str("com.google.sdv.WindowManager").unwrap();
        let set2 = FqinSet::from_fqin_statements(vec![statement2.clone(), statement3.clone()]);

        assert_eq!(
            set1.has_intersection(&set2),
            FqinSetsIntersectionResult::HasIntersection(
                "vm:com.google.*".into(),
                "com.google.sdv.WindowManager".into()
            )
        );
        assert_eq!(
            set2.has_intersection(&set1),
            FqinSetsIntersectionResult::HasIntersection(
                "com.google.sdv.WindowManager".into(),
                "vm:com.google.*".into(),
            )
        );
    }
}
