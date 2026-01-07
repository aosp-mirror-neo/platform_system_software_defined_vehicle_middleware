// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
#![allow(unused)]

use rustutils::android::system_properties::{self, Result};

// TODO: Replace this with a simpler mock implementation

#[cfg(not(test))]
pub fn read(name: &str) -> Result<Option<String>> {
    system_properties::read(name)
}

#[cfg(test)]
pub fn read(name: &str) -> Result<Option<String>> {
    testing::get_property(name)
}

#[cfg(not(test))]
pub fn read_bool(name: &str, default_value: bool) -> Result<bool> {
    system_properties::read_bool(name, default_value)
}

#[cfg(test)]
pub fn read_bool(name: &str, default_value: bool) -> Result<bool> {
    // Pretty much duplicating what rustutils::system_properties::read_bool does
    let value = testing::get_property(name)?;
    Ok(testing::parse_bool(value).unwrap_or(default_value))
}

#[cfg(test)]
pub(crate) mod testing {
    use std::collections::HashMap;
    use std::fmt::Debug;
    use std::sync::Mutex;

    use lazy_static::lazy_static;
    use rustutils::android::system_properties::Result;

    pub(crate) fn parse_bool(maybe_value: Option<String>) -> Option<bool> {
        match maybe_value.as_deref() {
            Some(value) => match value {
                "1" | "y" | "yes" | "on" | "true" => Some(true),
                "0" | "n" | "no" | "off" | "false" => Some(false),
                _ => None,
            },
            None => None,
        }
    }

    lazy_static! {
        static ref PROPERTIES_TEST_VALUE: Mutex<HashMap<String, Result<Option<String>>>> =
            Default::default();
    }

    pub(crate) fn set_property<S: Into<String>>(name: S, value: Result<Option<String>>) {
        let mut hashmap = PROPERTIES_TEST_VALUE.lock().unwrap();
        hashmap.insert(name.into(), value);
    }

    pub(crate) fn set_property_ok_value<S: Into<String>, V: Into<String>>(name: S, value: V) {
        let mut hashmap = PROPERTIES_TEST_VALUE.lock().unwrap();
        hashmap.insert(name.into(), Ok(Some(value.into())));
    }

    pub(crate) fn get_property<S: AsRef<str> + Debug>(name: S) -> Result<Option<String>> {
        let mut hashmap = PROPERTIES_TEST_VALUE.lock().unwrap();
        let name = name.as_ref();
        hashmap.remove(name).unwrap_or_else(|| panic!("Property was not set: '{name}'"))
    }
}

#[cfg(test)]
mod tests {
    use super::testing;

    #[test]
    fn set_get_success() {
        testing::set_property("test", Ok(None));

        let prop = testing::get_property("test");

        assert!(prop.is_ok());
        assert!(prop.unwrap().is_none());
    }

    #[test]
    fn set_ok_some_get_success() {
        testing::set_property("test", Ok(None));

        let prop = testing::get_property("test");

        assert!(prop.is_ok());
        assert!(prop.unwrap().is_none());
    }
}
