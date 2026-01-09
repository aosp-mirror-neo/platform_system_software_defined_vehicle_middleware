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

//! Classes for handling the files present in the vvmtruststore partition.

mod uds_public_keys;
pub use uds_public_keys::UdsPublicKeys;
pub use uds_public_keys::UDS_PUBS_PATH;

mod certified_uds_pubs;
pub use certified_uds_pubs::CertifiedUdsPubs;

mod claims;
pub use claims::{Claims, ISS, KEY_USAGE, SUB};

mod key_usage;
pub use key_usage::KeyUsage;

mod util;
pub use util::ec_or_okp_cose_key_to_pkey;

mod signer_certificate;
pub use signer_certificate::SignerCertificateClaims;

pub use hwtrust_private_key::private_key::pkey_kind;

mod crl;
pub use crl::CertificateRevocationList;
