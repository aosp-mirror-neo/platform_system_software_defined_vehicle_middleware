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

use google_sdv_service_discovery_common::aidl::google::sdv::service_discovery::common::AccessControl::AccessControl as AccessControlAidl;
use sdv_status_rs::SdvStatus;

#[repr(transparent)]
#[derive(Clone, Debug)]
pub struct AccessControl(AccessControlAidl);

impl TryFrom<AccessControlAidl> for AccessControl {
    type Error = SdvStatus;

    fn try_from(aidl: AccessControlAidl) -> Result<Self, Self::Error> {
        Ok(Self(aidl))
    }
}

impl From<AccessControl> for AccessControlAidl {
    fn from(access_control: AccessControl) -> Self {
        access_control.0
    }
}
