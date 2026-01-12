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

//! This file creates and represents the VM Name.

use std::convert::TryFrom;
use std::fmt;
use std::str::FromStr;

use sdv_comms_id_validator::vm_name_validation::{validate_vm_name, VmNameValidationError};

/// Virtual Machine Name, e.g. `vm`, `front-left-vm-10`.
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Default, Hash, Eq, PartialOrd, Ord)]
pub struct VmName(String);

impl VmName {
    /// Returns wrapped name as &str.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl FromStr for VmName {
    type Err = VmNameValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_vm_name(s)?;
        Ok(VmName(s.to_string()))
    }
}

impl fmt::Display for VmName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for VmName {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<VmName> for String {
    fn from(val: VmName) -> Self {
        val.0
    }
}

impl TryFrom<String> for VmName {
    type Error = VmNameValidationError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        validate_vm_name(&name)?;
        Ok(VmName(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use sdv_comms_id_validator::alphanumhyphen_validation::AlphanumHyphenValidationError;

    #[test]
    fn test_vm_name_from_valid_str() {
        assert_eq!(VmName::from_str("vm"), Ok(VmName("vm".to_string())));
        assert_eq!(VmName::from_str("vm-oem-7777"), Ok(VmName("vm-oem-7777".to_string())));
    }

    #[test]
    fn test_vm_name_from_invalid_str() {
        let alphanum_err = AlphanumHyphenValidationError::IsEmpty;
        assert_eq!(VmName::from_str(""), Err(VmNameValidationError(alphanum_err)));

        let alphanum_err = AlphanumHyphenValidationError::EndsWithInvalidCharacter('*');
        assert_eq!(VmName::from_str("vm-*"), Err(VmNameValidationError(alphanum_err)));
    }
}
