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

//! Functions for handling CBOR material

use ciborium::value::Value;
use coset::CoseError;
use openssl::x509::X509;
use CoseError::DecodeFailed;

pub(crate) fn x509_from_cbor_value(value: Value) -> Result<X509, CoseError> {
    let cert_der = value.into_bytes().map_err(|_| ce("an X.509 cert must be a byte string"))?;
    let cert = X509::from_der(&cert_der)
        .map_err(|e| ce(&format!("Couldn't DER decode X.509 certificate: {e:?}")))?;
    Ok(cert)
}

pub(crate) fn x509_to_cbor_value(cert: X509) -> Result<Value, CoseError> {
    Ok(Value::Bytes(
        cert.to_der().map_err(|e| ce(&format!("Couldn't DER encode X.509 certificate: {e:?}")))?,
    ))
}

/// Helper function for creating a semantic CoseError.
pub(crate) fn ce(s: &str) -> CoseError {
    DecodeFailed(ciborium::de::Error::semantic(None, s))
}
