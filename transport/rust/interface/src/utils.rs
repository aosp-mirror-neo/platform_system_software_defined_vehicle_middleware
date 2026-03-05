/*
 * Copyright (C) 2026 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Middleware Interface (Crate) Utilities

/// Converts a package path and type name into a kebab-case string.
///
/// This function is used to generate topic names for VSIDL services.
/// It converts the package path and type name into a kebab-case string
/// by replacing dots and colons with dashes and converting uppercase letters to lowercase.
///
///
/// # Arguments
///
/// * `pkg_path` - The package path of the type.
/// * `type_name` - The type name.
/// * `suffix` - An optional suffix to append to the string.
///
/// # Returns
///
/// A kebab-case string representing the package path and type name.
///
/// # Examples
///
/// ```
/// use crate::format_to_kebab_case;
/// let topic_name = format_to_kebab_case("com.android.sdv.comp.test.stable", "TirePressure", "front-left");
/// assert_eq!(topic_name, "com-android-sdv-comp-test-stable-tire-pressure-front-left");
/// ```
pub(crate) fn format_to_kebab_case(pkg_path: &str, type_name: &str, suffix: &str) -> String {
    let estimated_len = pkg_path.len() + type_name.len() + suffix.len() + 5;
    let mut result = String::with_capacity(estimated_len);

    // Prepare our suffix separator if needed
    let separator = if suffix.is_empty() { None } else { Some('-') };

    // Chain everything into a single, lazy iterator that we can "peek" into
    let mut chars =
        pkg_path.chars().chain(type_name.chars()).chain(separator).chain(suffix.chars()).peekable();

    // State tracking
    let mut last_was_dash = true; // Set to true initially to prevent leading dashes
    let mut last_was_lower = false;
    let mut last_was_upper = false;
    let mut last_was_digit = false;

    while let Some(c) = chars.next() {
        if c.is_alphanumeric() {
            if c.is_uppercase() {
                // Look ahead at the next character
                let next_is_lower = chars.peek().is_some_and(|&next_c| next_c.is_lowercase());

                // Determine if we are at a word boundary
                let is_word_boundary =
                    last_was_lower || last_was_digit || (last_was_upper && next_is_lower);
                // Insert a dash if we're at a boundary AND didn't just insert one
                if !last_was_dash && is_word_boundary {
                    result.push('-');
                }

                result.extend(c.to_lowercase());
                last_was_upper = true;
                last_was_lower = false;
                last_was_digit = false;
            } else {
                result.push(c);
                // Numbers shouldn't trigger the "last was lower" acronym split
                last_was_lower = c.is_lowercase();
                last_was_digit = c.is_ascii_digit();
                last_was_upper = false;
            }
            last_was_dash = false;
        } else {
            // Any non-alphanumeric character becomes a dash
            if !last_was_dash {
                result.push('-');
                last_was_dash = true;
            }
            last_was_upper = false;
            last_was_lower = false;
            last_was_digit = false;
        }
    }

    // Clean up a trailing dash if the string happened to end with a separator
    if result.ends_with('-') {
        result.pop();
    }

    result
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_basic_camel_case() {
        assert_eq!(
            format_to_kebab_case("some.pkg", "MyType", "front-left"),
            "some-pkg-my-type-front-left"
        );
    }

    #[test]
    fn test_acronym_handling() {
        // Acronym at the start
        assert_eq!(format_to_kebab_case("", "XMLParser", ""), "xml-parser");
        // Acronym in the middle
        assert_eq!(format_to_kebab_case("", "parseXMLString", ""), "parse-xml-string");
        // Acronym at the end
        assert_eq!(format_to_kebab_case("", "parseXML", ""), "parse-xml");
    }

    #[test]
    fn test_empty_suffix() {
        // Ensures no trailing dash is left behind when the suffix is empty
        assert_eq!(format_to_kebab_case("core.net", "HTTPRequest", ""), "core-net-http-request");
    }

    #[test]
    fn test_multiple_separators() {
        // Checks that consecutive non-alphanumeric chars collapse into a single dash
        assert_eq!(
            format_to_kebab_case("some::weird..pkg", "Type Name", "suf_fix"),
            "some-weird-pkg-type-name-suf-fix"
        );
    }

    #[test]
    fn test_numbers() {
        // Numbers shouldn't cause weird splits or be dropped
        assert_eq!(
            format_to_kebab_case("api.v1beta1", "MyType2", "gen3"),
            "api-v1beta1-my-type2-gen3"
        );
    }

    #[test]
    fn test_leading_and_trailing_separators() {
        // Ensures leading and trailing dashes from separators are cleaned up
        assert_eq!(format_to_kebab_case(".leading", "Type", "trailing."), "leading-type-trailing");
    }

    #[test]
    fn test_all_empty() {
        assert_eq!(format_to_kebab_case("", "", ""), "");
    }
}
