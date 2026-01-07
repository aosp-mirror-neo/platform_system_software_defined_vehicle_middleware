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

//! Represents Selector type used to define ACL statements.

use std::fmt;

/// StarSelector is used to describe a Set of any objects.
#[derive(Clone, Eq, PartialEq, Debug)]
pub(crate) struct StarSelector;

impl StarSelector {
    pub(crate) fn try_from(s: &str) -> Option<StarSelector> {
        match s {
            "*" => Some(StarSelector),
            _ => None,
        }
    }
}

impl fmt::Display for StarSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "*")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_star_selector_try_from_success() {
        assert_eq!(StarSelector::try_from("*"), Some(StarSelector));
    }

    #[test]
    fn test_star_selector_try_from_failed() {
        assert_eq!(StarSelector::try_from(""), None);

        assert_eq!(StarSelector::try_from("?"), None);
        assert_eq!(StarSelector::try_from("!"), None);

        assert_eq!(StarSelector::try_from("**"), None);
        assert_eq!(StarSelector::try_from("??"), None);
        assert_eq!(StarSelector::try_from("!!"), None);

        assert_eq!(StarSelector::try_from("*?"), None);
        assert_eq!(StarSelector::try_from("?!"), None);
        assert_eq!(StarSelector::try_from("!*"), None);

        assert_eq!(StarSelector::try_from("value*"), None);
        assert_eq!(StarSelector::try_from("value?"), None);
        assert_eq!(StarSelector::try_from("value!"), None);

        assert_eq!(StarSelector::try_from("*value"), None);
        assert_eq!(StarSelector::try_from("?value"), None);
        assert_eq!(StarSelector::try_from("!value"), None);

        assert_eq!(StarSelector::try_from("val*ue"), None);
        assert_eq!(StarSelector::try_from("val?ue"), None);
        assert_eq!(StarSelector::try_from("val!ue"), None);
    }
}
