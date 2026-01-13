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

/// A type that represents values that may be used only once. It will be destroyed on use.
#[repr(transparent)]
pub(in crate::callback) struct MaybeUsed<T>(Option<T>);

impl<T> MaybeUsed<T> {
    pub(crate) fn new_unused(value: T) -> MaybeUsed<T> {
        MaybeUsed(Some(value))
    }

    /// Extracts the stored value and returns it.
    /// WARNING: This function will panic if called more than once on the same instance.
    pub(crate) fn take(&mut self) -> T {
        self.0.take().unwrap()
    }
}
