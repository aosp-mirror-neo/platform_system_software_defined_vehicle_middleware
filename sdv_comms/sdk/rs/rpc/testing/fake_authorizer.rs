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

#![cfg(test)]

use crate::id::ServiceFqin;
use crate::rpc::server::Authorizer;

use sdv_status_rs::SdvResult;

use async_trait::async_trait;

// Fake Authorizer
#[derive(Clone)]
pub(crate) enum Access {
    Allow,
    Deny,
}

#[derive(Clone)]
pub(crate) struct FakeAuthorizer {
    access: Access,
}

impl FakeAuthorizer {
    pub(crate) fn new(access: Access) -> FakeAuthorizer {
        FakeAuthorizer { access }
    }
}

#[async_trait]
impl Authorizer for FakeAuthorizer {
    async fn is_authorized(&self, _subject_fqin: &ServiceFqin) -> SdvResult<bool> {
        match self.access {
            Access::Allow => Ok(true),
            Access::Deny => Ok(false),
        }
    }
}
