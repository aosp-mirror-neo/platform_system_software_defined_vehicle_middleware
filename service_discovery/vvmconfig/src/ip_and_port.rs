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
use ciborium::Value;
use coset::{AsCborValue, CoseError};
use serde::Deserialize;
use std::net::{IpAddr, SocketAddr};

const DEFAULT_MESH_AGENT_PORT: u16 = 2000;

/// IP address and an optional port number.
#[derive(Clone, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct IpAndPort {
    /// An IP address.
    pub ip: IpAddr,
    /// An optional port number. If not specified, the default port number for
    /// Service Discovery agents should be used instead.
    pub port: Option<u16>,
}

// Must be kept in sync with vvmconfig.cddl
impl AsCborValue for IpAndPort {
    fn from_cbor_value(value: Value) -> Result<Self, CoseError> {
        use std::convert::TryInto;

        let value_array =
            value.into_array().map_err(|_| semantic_error("IpAndPort must be an array"))?;

        let mut iterator = value_array.into_iter();

        let ip_bytes = iterator
            .next()
            .ok_or(semantic_error("IpAndPort array cannot be empty"))?
            .into_bytes()
            .map_err(|_| semantic_error("IpAndPort::ip must be a bstr"))?;

        let ip: IpAddr = match TryInto::<[u8; 4]>::try_into(ip_bytes.clone()) {
            Ok(four_bytes) => IpAddr::from(four_bytes),
            Err(_) => match TryInto::<[u8; 16]>::try_into(ip_bytes) {
                Ok(sixteen_bytes) => IpAddr::from(sixteen_bytes),
                Err(_) => {
                    return Err(semantic_error(
                        "IpAndPort::ip bstr must be either 4 or 16 bytes long",
                    ))
                }
            },
        };

        let port: Option<u16> = match iterator.next() {
            Some(value) => Some(
                value
                    .into_integer()
                    .map_err(|_| semantic_error("InAndPort::port must be an uint"))?
                    .try_into()
                    .map_err(|_| semantic_error("InAndPort::port must be an u16"))?,
            ),
            None => None,
        };

        match iterator.next() {
            Some(_) => Err(semantic_error("IpAndPort array has more than 2 items")),
            None => Ok(IpAndPort { ip, port }),
        }
    }

    fn to_cbor_value(self) -> Result<Value, CoseError> {
        let mut vec = Vec::new();

        vec.push(value_for_addr(&self.ip)?);

        if let Some(port) = self.port {
            vec.push(Value::Integer(port.into()));
        }

        Ok(Value::Array(vec))
    }
}

fn value_for_addr(ip: &IpAddr) -> Result<Value, CoseError> {
    match ip {
        IpAddr::V4(ipv4) => Ok(Value::Bytes(ipv4.octets().to_vec())),
        IpAddr::V6(ipv6) => Ok(Value::Bytes(ipv6.octets().to_vec())),
    }
}

impl From<IpAndPort> for SocketAddr {
    fn from(value: IpAndPort) -> Self {
        let port = match value.port {
            Some(port) => port,
            None => DEFAULT_MESH_AGENT_PORT,
        };
        SocketAddr::new(value.ip, port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    #[test]
    fn from_cbor_value_valid_ipv6() {
        let ipv6_bytes = [
            0x20, 0x01, 0x0d, 0xb8, 0x85, 0xa3, 0x00, 0x00, 0x00, 0x00, 0x8a, 0x2e, 0x03, 0x70,
            0x73, 0x34,
        ];
        let port_val: u16 = 8080;
        let cbor_values_ipv6 =
            vec![Value::Bytes(ipv6_bytes.to_vec()), Value::Integer(port_val.into())];
        let cbor_array_ipv6 = Value::Array(cbor_values_ipv6);
        let result_ipv6 = IpAndPort::from_cbor_value(cbor_array_ipv6);

        let ip_and_port_ipv6 = result_ipv6.unwrap();
        assert_eq!(ip_and_port_ipv6.ip, IpAddr::V6(Ipv6Addr::from(ipv6_bytes)));
        assert_eq!(ip_and_port_ipv6.port, Some(port_val));
    }

    #[test]
    fn from_cbor_value_invalid_ip_length_5_bytes() {
        let invalid_ip_bytes = vec![1, 2, 3, 4, 5];
        let port_val: u16 = 8080;
        let cbor_values_invalid_ip =
            vec![Value::Bytes(invalid_ip_bytes), Value::Integer(port_val.into())];
        let cbor_array_invalid_ip = Value::Array(cbor_values_invalid_ip);
        let result_invalid_ip = IpAndPort::from_cbor_value(cbor_array_invalid_ip);

        let err = result_invalid_ip.unwrap_err();
        assert!(err.to_string().contains("IpAndPort::ip bstr must be either 4 or 16 bytes long"));
    }
}
