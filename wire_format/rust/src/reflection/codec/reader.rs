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

use std::ops::{Deref, DerefMut};

use protobuf::CodedInputStream;

/// Protobuf reader
pub struct ProtobufReader<'lt> {
    coded_input_stream: CodedInputStream<'lt>,
}

impl<'lt> Deref for ProtobufReader<'lt> {
    type Target = CodedInputStream<'lt>;

    fn deref(&self) -> &Self::Target {
        &self.coded_input_stream
    }
}

impl DerefMut for ProtobufReader<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.coded_input_stream
    }
}

impl<'lt> ProtobufReader<'lt> {
    pub(crate) fn new(bytes: &'lt [u8]) -> Self {
        let coded_input_stream = CodedInputStream::from_bytes(bytes);
        Self { coded_input_stream }
    }
}
