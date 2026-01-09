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

/// A simplified description of the power state.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum SimplePowerState {
    /// System is about to be suspended or turned off.
    #[allow(dead_code)] // Work around sdv_sd_agent_vendor build warning.
    Off,

    /// System is running normally.
    On,
}

#[cfg(feature = "vpm")]
mod vpm;

#[cfg(feature = "ivi")]
mod ivi_networking;

#[cfg(feature = "ivi")]
mod vpm {
    pub(crate) use super::ivi_networking::run_power_state_reporter;
}

#[cfg(not(any(feature = "vpm", feature = "ivi")))]
mod stub;

#[cfg(not(any(feature = "vpm", feature = "ivi")))]
mod vpm {
    pub(crate) use super::stub::run_power_state_reporter;
}

pub(crate) use vpm::run_power_state_reporter;
