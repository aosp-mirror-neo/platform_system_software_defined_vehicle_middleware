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

use anyhow::Context;
use openssl::hash::{hash, MessageDigest};

pub(crate) fn sha256(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let digest_bytes =
        hash(MessageDigest::sha256(), data).context("Failed to compute hash of uds_pubs")?;

    Ok((*digest_bytes).to_owned())
}

#[cfg(test)]
pub(crate) mod test_support {
    /// sha256("foobar")
    pub(crate) fn sha256_foobar() -> Vec<u8> {
        vec![
            195, 171, 143, 241, 55, 32, 232, 173, 144, 71, 221, 57, 70, 107, 60, 137, 116, 229,
            146, 194, 250, 56, 61, 74, 57, 96, 113, 76, 174, 240, 196, 242,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::sha256;
    use super::test_support::sha256_foobar;

    #[test]
    fn sha256_sanity_check() -> anyhow::Result<()> {
        assert_eq!(sha256("foobar".as_bytes())?, sha256_foobar());
        Ok(())
    }
}
