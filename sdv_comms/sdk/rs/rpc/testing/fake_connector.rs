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

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::id::ServiceFqin;
use crate::rpc::client::Connector;
use crate::rpc::server::ServiceRegistrar;

use sdv_status_rs::SdvResult;
use sdv_status_rs::SdvStatus;
use sdv_status_rs::SdvStatusCode;

use async_trait::async_trait;
use tokio::net::TcpListener as TokioTcpListener;
use tokio::net::TcpStream as TokioTcpStream;

#[derive(Clone)]
pub(crate) struct RegistrationRecord {
    pub(crate) address: SocketAddr,
}

// Fake Connector
#[derive(Clone)]
pub(crate) struct FakeConnector {
    service_registry: Arc<std::sync::Mutex<HashMap<String, RegistrationRecord>>>,
}

impl FakeConnector {
    pub(crate) fn new(
        service_registry: Arc<std::sync::Mutex<HashMap<String, RegistrationRecord>>>,
    ) -> FakeConnector {
        FakeConnector { service_registry }
    }
}

#[async_trait]
impl Connector for FakeConnector {
    async fn connect(&self, _fqin: &ServiceFqin, unit_name: &str) -> SdvResult<TokioTcpStream> {
        let discovery_res: Option<RegistrationRecord> = {
            let registry_guard = self.service_registry.lock().unwrap();
            registry_guard.get(unit_name).cloned()
        };

        match discovery_res {
            Some(record) => Ok(TokioTcpStream::connect(record.address).await.unwrap()),
            None => SdvResult::Err(SdvStatus::with_message(
                SdvStatusCode::NotFound,
                "Connecting to server: could not find unit name",
            )),
        }
    }
}

// Fake Registrar
pub(crate) struct FakeServiceRegistrar {
    service_registry: Arc<std::sync::Mutex<HashMap<String, RegistrationRecord>>>,
    unit_name: String,
}

impl FakeServiceRegistrar {
    pub(crate) fn new(
        service_registry: Arc<std::sync::Mutex<HashMap<String, RegistrationRecord>>>,
        unit_name: String,
    ) -> FakeServiceRegistrar {
        FakeServiceRegistrar { service_registry, unit_name }
    }
}

#[async_trait]
impl ServiceRegistrar for FakeServiceRegistrar {
    async fn register(&mut self) -> SdvResult<TokioTcpListener> {
        let port: u32 = 0;
        let addr: String = format!("127.0.0.1:{port}").parse().expect("Could not parse IP:Port");

        let listener = TokioTcpListener::bind(addr).await.expect("Could not create a TCP listener");
        let local_addr = listener.local_addr().expect("Could not get address of the TCP listener");

        let registration_record = RegistrationRecord { address: local_addr };

        self.service_registry.lock().unwrap().insert(self.unit_name.clone(), registration_record);
        Ok(listener)
    }
}
