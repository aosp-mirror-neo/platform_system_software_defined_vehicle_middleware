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

use crate::error::semantic_error;
use crate::IpAndPort;
use ciborium::Value;
use coset::{AsCborValue, CoseError};
use serde::Deserialize;

/// Virtual Machine Configuration
#[derive(Clone, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VmConfig {
    /// List of IP addresses where the Service Discovery agent of this VM can be found.
    pub ips: Vec<IpAndPort>,

    /// Index of the DICE policy for the Android SDV VM DICE chain.
    pub android: u32,

    /// Index of the DICE policy for the Secure World DICE chain used by the Android SDV VM.
    pub secure_world: u32,
}

// Must be kept in sync with vvmconfig.cddl
impl AsCborValue for VmConfig {
    fn from_cbor_value(value: Value) -> Result<Self, CoseError> {
        let [ips, android, secure_world] = value
            .into_array()
            .map_err(|_| semantic_error("VmConfig must be an array"))?
            .try_into()
            .map_err(|_| semantic_error("VmConfig array must have 3 items"))?;

        let ips = ips
            .into_array()
            .map_err(|_| semantic_error("VmConfig::ips is not an array"))?
            .into_iter()
            .map(IpAndPort::from_cbor_value)
            .collect::<Result<Vec<IpAndPort>, CoseError>>()?;

        let android: u32 = android
            .into_integer()
            .map_err(|_| semantic_error("VmConfig::android is not an integer"))?
            .try_into()
            .map_err(|_| semantic_error("VmConfig::android doesn't fit into an u32"))?;

        let secure_world: u32 = secure_world
            .into_integer()
            .map_err(|_| semantic_error("VmConfig::secureWorld is not an integer"))?
            .try_into()
            .map_err(|_| semantic_error("VmConfig::secureWorld doesn't fit into an u32"))?;

        Ok(VmConfig { ips, android, secure_world })
    }

    fn to_cbor_value(self) -> Result<Value, CoseError> {
        Ok(Value::Array(vec![
            Value::Array(
                self.ips
                    .into_iter()
                    .map(IpAndPort::to_cbor_value)
                    .collect::<Result<Vec<Value>, CoseError>>()?,
            ),
            Value::Integer(self.android.into()),
            Value::Integer(self.secure_world.into()),
        ]))
    }
}
