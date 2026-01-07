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

use super::async_client::{
    MeshManagerAsyncClient, PeerUnitStream, PingRequest, SubscribePeerLocalServiceUnitsRequest,
};
use crate::kv::{ServiceRegistry, ServiceUnitIdentifier};
use crate::mesh::proto_to_aidl::TryToAidl;
use anyhow::{bail, Context};
use futures::StreamExt;
use log::{debug, info, warn};
use sdv_sd_mesh_api::sdv_sd_mesh::{
    peer_local_service_units_event::Event, AddServiceUnitEvent, RemoveByIdentityEvent,
    RemoveServiceUnitEvent,
};
use std::borrow::Borrow;
use std::sync::Arc;

/// Represents the client side of a connection to an sdv_sd_agent in a peer VM.
///
/// Tracks registration of service units in that peer, updating the local ServiceRegistry
/// accordingly.
///
/// check_liveness() must be called regularly to ensure the connection is still alive.
pub(crate) struct PeerConnection {
    vm_name: String,
    rpc_client: Box<dyn MeshManagerAsyncClient>,
    service_units_stream_reader: Option<tokio::task::JoinHandle<()>>,
}

impl PeerConnection {
    pub fn try_new(
        vm_name: &str,
        rpc_client: Box<dyn MeshManagerAsyncClient>,
        service_registry: Arc<ServiceRegistry>,
        tokio_runtime: Arc<tokio::runtime::Runtime>,
    ) -> anyhow::Result<Self> {
        let service_units_stream_reader = subscribe_peer_service_unit_updates(
            vm_name,
            service_registry,
            rpc_client.borrow(),
            tokio_runtime.borrow(),
        )?;
        debug!("Subscribed for a stream of service unit events from remote VM: {vm_name}");

        Ok(Self {
            vm_name: vm_name.to_owned(),
            rpc_client,
            service_units_stream_reader: Some(service_units_stream_reader),
        })
    }

    pub async fn check_liveness(&mut self) -> anyhow::Result<()> {
        if !self.is_stream_reader_alive() {
            bail!(format_dead_stream_reader_error(&self.vm_name));
        }

        let ping_result = self.ping().await.with_context(|| format_ping_error(&self.vm_name));

        if ping_result.is_err() {
            self.abort_service_units_stream_reader().await;
        }

        ping_result
    }

    async fn ping(&self) -> anyhow::Result<()> {
        self.rpc_client.ping(&PingRequest::default()).await?;
        Ok(())
    }

    pub async fn close(&mut self) {
        self.abort_service_units_stream_reader().await;
    }

    async fn abort_service_units_stream_reader(&mut self) {
        let maybe_join_handle = self.service_units_stream_reader.take();
        if let Some(join_handle) = maybe_join_handle {
            join_handle.abort();
            let _ = join_handle.await;
            debug!("Aborted task for reading service units stream from VM {}", self.vm_name);
        }
    }

    fn is_stream_reader_alive(&self) -> bool {
        match self.service_units_stream_reader.as_ref() {
            Some(join_handle) => !join_handle.is_finished(),
            None => false,
        }
    }
}

fn format_dead_stream_reader_error(vm_name: &str) -> String {
    format!("Task for reading service units stream from VM {vm_name} is dead.")
}

fn format_ping_error(vm_name: &str) -> String {
    format!("Ping to VM {vm_name} failed")
}

fn subscribe_peer_service_unit_updates(
    vm_name: &str,
    service_registry: Arc<ServiceRegistry>,
    rpc_client: &dyn MeshManagerAsyncClient,
    tokio_runtime: &tokio::runtime::Runtime,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let service_units_stream = rpc_client
        .subscribe_peer_local_service_units(&SubscribePeerLocalServiceUnitsRequest {
            ..Default::default()
        })
        .inspect_err(|err| {
            warn!("Unable to get list of units local to a peer {vm_name}, {err:?}");
        })?;

    let join_handle = tokio_runtime.spawn(read_service_units_stream(
        service_units_stream,
        service_registry,
        vm_name.to_owned(),
    ));

    Ok(join_handle)
}

async fn read_service_units_stream(
    mut peer_unit_stream: PeerUnitStream,
    service_registry: Arc<ServiceRegistry>,
    vm_name: String,
) {
    debug!("Started task for reading service units stream from VM {vm_name}");
    while let Some(result) = peer_unit_stream.next().await {
        match result {
            Ok(event_proto) => match event_proto.event {
                Some(event) => {
                    process_service_unit_event(event, &service_registry, vm_name.as_ref());
                }
                None => {
                    warn!(
                        "Received PeerLocalServiceUnitsEvent from {vm_name} with event field not \
                        set, terminating connection."
                    );
                    break;
                }
            },
            Err(error) => {
                warn!("gRPC event stream to {vm_name} terminated with error: {error}");
                break;
            }
        }
    }
    info!("Service units stream from VM {vm_name} has ended.");
}

fn process_service_unit_event(
    peer_local_service_units_event: Event,
    service_registry: &ServiceRegistry,
    vm_name: &str,
) {
    match peer_local_service_units_event {
        Event::AddServiceUnit(AddServiceUnitEvent {
            unit_definition, transport_metadata, ..
        }) => {
            let (Ok(definition), Ok(metadata)) =
                (unit_definition.try_to_aidl(), transport_metadata.try_to_aidl())
            else {
                warn!("Failed to convert remote definition or metadata from proto to aidl");
                return;
            };
            if let Err(err) = service_registry.add_remote_service_unit(definition, metadata) {
                warn!("Unable to populate units from VM {vm_name}: {err}");
            }
        }

        Event::RemoveServiceUnit(RemoveServiceUnitEvent { service_unit_identifier, .. }) => {
            let suid = ServiceUnitIdentifier {
                sid: service_unit_identifier.sid.into(),
                name: service_unit_identifier.service_unit_name.clone(),
            };
            service_registry.remove_remote_service_unit(&suid);
        }

        Event::RemoveByIdentity(RemoveByIdentityEvent { service_fqin, .. }) => {
            let Ok(fqin) = service_fqin.try_to_aidl() else {
                warn!("Failed to convert remote fqin from proto to aidl");
                return;
            };

            let Some(identity) = service_registry.find_identity_by_fqin_v2(&fqin) else {
                info!("Failed to remove find remote identity: not found");
                return;
            };

            service_registry.remove_by_identity_v2(&identity);
        }
        _ => warn!("Received unsupported event"),
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    use anyhow::anyhow;
    use futures::{task::Poll, Stream};
    use sdv_sd_mesh_api::sdv_sd_mesh::PeerLocalServiceUnitsEvent;
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::time::Duration;
    use tokio::time::timeout;

    /// A peer unit stream "never" ends in order to send updates whenever they happen.
    pub(crate) struct FakePeerUnitStream {
        events: VecDeque<grpcio::Result<PeerLocalServiceUnitsEvent>>,
    }

    impl FakePeerUnitStream {
        /// Creates a new FakePeerUnitStream
        ///
        /// It will return the `events` in the given order when polled. Once there are no more
        /// events to be returned, it will return Poll::Pending instead. This way the stream
        /// is kept alive.
        pub(crate) fn new(events: Vec<grpcio::Result<PeerLocalServiceUnitsEvent>>) -> Self {
            Self { events: events.into() }
        }
    }

    impl Stream for FakePeerUnitStream {
        type Item = grpcio::Result<PeerLocalServiceUnitsEvent>;

        fn poll_next(
            self: Pin<&mut Self>,
            _cx: &mut futures::task::Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            if self.events.is_empty() {
                Poll::Pending
            } else {
                Poll::Ready(self.get_mut().events.pop_front())
            }
        }
    }

    pub(crate) const VM_INSTANCE_1_NAME: &str = "instance1";
    pub(crate) const VM_INSTANCE_2_NAME: &str = "instance2";
    pub(crate) const VM_INSTANCE_3_NAME: &str = "instance3";

    /// Creates a vector of events with test definitions and metadata for 2 unit types of different
    /// kinds.
    pub(crate) fn create_service_unit_events(
        vm_name: &str,
    ) -> Vec<crate::Result<PeerLocalServiceUnitsEvent, grpcio::Error>> {
        use protobuf::MessageField;
        use sdv_sd_mesh_api::discovery::ServiceUnitDefinition as ServiceUnitDefinitionProto;
        use sdv_sd_mesh_api::identity::{
            service_identity::EcPublicKey as EcPublicKeyProto, ServiceFqin as ServiceFqinProto,
            ServiceIdentity as ServiceIdentityProto,
        };
        use sdv_sd_mesh_api::metadata::{
            application_metadata::typed_metadata::Metadata as MetadataProto,
            application_metadata::TypedMetadata as TypedMetadataProto,
            ApplicationMetadata as ApplicationMetadataProto,
            PublisherUnitMetadata as PublisherUnitMetadataProto,
            ServerUnitMetadata as ServerUnitMetadataProto,
            TransportMetadata as TransportMetadataProto,
        };
        use sdv_sd_mesh_api::sdv_sd_mesh::{
            peer_local_service_units_event::Event::AddServiceUnit, AddServiceUnitEvent,
        };
        use std::iter::zip;

        let mut unit_definition = Vec::new();
        let mut transport_metadata = Vec::new();

        // Ensure our fake SIDs are unique
        // We assume this function will only be called for instance2 and instance3 (the remote VMs
        // in our tests).
        // We could have used id_gen::SidGenerator for that, but that would require an extra
        // function parameter, which is less convenient and leads to less readable test code.
        let sid = if vm_name == VM_INSTANCE_2_NAME {
            123
        } else {
            assert_eq!(vm_name, VM_INSTANCE_3_NAME);
            124
        };

        let mut identity = ServiceIdentityProto {
            name: Some(ServiceFqinProto { sdv_vm_name: vm_name.to_string(), ..Default::default() })
                .into(),
            sid,
            public_key: Some(EcPublicKeyProto { key_value: vec![0; 32], ..Default::default() })
                .into(),
            ..Default::default()
        };

        let service_sud = ServiceUnitDefinitionProto {
            identity: Some(identity.clone()).into(),
            app_metadata: Some(ApplicationMetadataProto {
                typed_metadata: Some(TypedMetadataProto {
                    metadata: Some(MetadataProto::ServerUnit(ServerUnitMetadataProto::default())),
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        };
        unit_definition.push(service_sud);
        transport_metadata.push(TransportMetadataProto::default());

        // Ensure our fake SIDs are unique
        // See previous comment on SIDs.
        if vm_name == VM_INSTANCE_2_NAME {
            identity.sid = 321;
        } else {
            assert_eq!(vm_name, VM_INSTANCE_3_NAME);
            identity.sid = 322;
        };
        let topic_sud = ServiceUnitDefinitionProto {
            identity: Some(identity).into(),
            app_metadata: Some(ApplicationMetadataProto {
                typed_metadata: Some(TypedMetadataProto {
                    metadata: Some(MetadataProto::PublisherUnit(
                        PublisherUnitMetadataProto::default(),
                    )),
                    ..Default::default()
                })
                .into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        };
        unit_definition.push(topic_sud);
        transport_metadata.push(TransportMetadataProto::default());

        zip(unit_definition, transport_metadata)
            .map(|(service_unit, transport_metadata)| {
                Ok(PeerLocalServiceUnitsEvent {
                    event: Some(AddServiceUnit(AddServiceUnitEvent {
                        unit_definition: MessageField(Some(Box::new(service_unit))),
                        transport_metadata: MessageField(Some(Box::new(transport_metadata))),
                        ..Default::default()
                    })),
                    ..Default::default()
                })
            })
            .collect()
    }

    /// Creates a vector of events with test definitions and metadata for 2 unit types of different
    /// kinds.
    pub(crate) fn create_instance2_service_unit_events(
    ) -> Vec<crate::Result<PeerLocalServiceUnitsEvent, grpcio::Error>> {
        create_service_unit_events(VM_INSTANCE_2_NAME)
    }

    /// Polls a condition until it returns true, with a timeout.
    pub(crate) async fn wait_for<F>(mut condition: F) -> anyhow::Result<()>
    where
        F: FnMut() -> bool + Send + 'static,
    {
        timeout(Duration::from_secs(5), async {
            loop {
                if condition() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .map_err(|_| anyhow!("wait_for() timed out"))?;
        Ok(())
    }

    /// Waits until the service units from the remote VM are present in the service
    /// registry.
    pub(crate) async fn wait_for_remote_service_units_registration(
        service_registry: Arc<ServiceRegistry>,
        vm_name: &str,
    ) -> anyhow::Result<()> {
        let vm_name = vm_name.to_string();
        wait_for(move || {
            let all_service_units = service_registry.get_all_service_units_by_vm_name(&vm_name);
            // Our mock async_client returns two service units. Thus MeshManager should have
            // correctly processed this result and added the corresponding service units to the
            // service registry.
            all_service_units.len() == 2
        })
        .await
    }

    /// Waits until the service units from the remote VM (instance2) are present in the service
    /// registry.
    pub(crate) async fn wait_for_instance2_service_units_registration(
        service_registry: Arc<ServiceRegistry>,
    ) -> anyhow::Result<()> {
        wait_for_remote_service_units_registration(service_registry, VM_INSTANCE_2_NAME).await
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{
        create_instance2_service_unit_events, wait_for_instance2_service_units_registration,
        FakePeerUnitStream, VM_INSTANCE_2_NAME,
    };
    use super::*;
    use crate::mesh::async_client::{MockMeshManagerAsyncClient, PingResponse};
    use grpcio::{Error as GrpcError, RpcStatus, RpcStatusCode};

    /// Tests that PeerConnection automatically subscribes to the stream of service unit updates
    /// and correctly process it, updating the given ServiceRegistry accordingly.
    #[test]
    fn peer_service_unit_stream_is_processed() -> anyhow::Result<()> {
        let mut mock_client = Box::new(MockMeshManagerAsyncClient::new());

        mock_client.expect_subscribe_peer_local_service_units().return_once(move |_| {
            Ok(Box::pin(FakePeerUnitStream::new(create_instance2_service_unit_events())))
        });

        let service_registry = Arc::new(ServiceRegistry::default());

        let tokio_runtime =
            Arc::new(tokio::runtime::Builder::new_current_thread().enable_all().build()?);

        let _peer_connection = PeerConnection::try_new(
            VM_INSTANCE_2_NAME,
            mock_client,
            service_registry.clone(),
            tokio_runtime.clone(),
        )?;

        tokio_runtime.block_on(async {
            wait_for_instance2_service_units_registration(service_registry).await.unwrap();
        });

        Ok(())
    }

    /// check_liveness() should fail if the service units stream has finished.
    #[test]
    fn check_liveness_dead_stream_err() -> anyhow::Result<()> {
        let mut mock_client = Box::new(MockMeshManagerAsyncClient::new());

        // Returns a stream that will finish once all its events have been consumed.
        mock_client.expect_subscribe_peer_local_service_units().return_once(move |_| {
            Ok(Box::pin(futures::stream::iter(create_instance2_service_unit_events())))
        });

        mock_client.expect_ping().returning(|_request| Ok(PingResponse::default()));

        let service_registry = Arc::new(ServiceRegistry::default());

        let tokio_runtime =
            Arc::new(tokio::runtime::Builder::new_current_thread().enable_all().build()?);

        let mut peer_connection = PeerConnection::try_new(
            VM_INSTANCE_2_NAME,
            mock_client,
            service_registry.clone(),
            tokio_runtime.clone(),
        )?;

        tokio_runtime.block_on(async move {
            // This will consume all events of the service units stream. The stream will then
            // finish.
            wait_for_instance2_service_units_registration(service_registry).await.unwrap();

            let err = peer_connection.check_liveness().await.expect_err(
                "A dead service unit stream reader should have caused check_liveness() failure",
            );
            assert_eq!(err.to_string(), format_dead_stream_reader_error(VM_INSTANCE_2_NAME));
        });

        Ok(())
    }

    #[test]
    fn check_liveness_ok() -> anyhow::Result<()> {
        let mut mock_client = Box::new(MockMeshManagerAsyncClient::new());

        mock_client.expect_subscribe_peer_local_service_units().return_once(move |_| {
            Ok(Box::pin(FakePeerUnitStream::new(create_instance2_service_unit_events())))
        });

        mock_client.expect_ping().returning(|_request| Ok(PingResponse::default()));

        let service_registry = Arc::new(ServiceRegistry::default());

        let tokio_runtime =
            Arc::new(tokio::runtime::Builder::new_current_thread().enable_all().build()?);

        let mut peer_connection = PeerConnection::try_new(
            VM_INSTANCE_2_NAME,
            mock_client,
            service_registry.clone(),
            tokio_runtime.clone(),
        )?;

        tokio_runtime.block_on(async move {
            wait_for_instance2_service_units_registration(service_registry).await.unwrap();

            peer_connection.check_liveness().await.expect("connection should be considered alive");
        });

        Ok(())
    }

    /// check_liveness() fails if the Ping it send to the peer fails.
    #[test]
    fn check_liveness_ping_timeout_err() -> anyhow::Result<()> {
        let mut mock_client = Box::new(MockMeshManagerAsyncClient::new());

        mock_client.expect_subscribe_peer_local_service_units().return_once(move |_| {
            Ok(Box::pin(FakePeerUnitStream::new(create_instance2_service_unit_events())))
        });

        // Simulate ping failure due to an offline (or otherwise unresponsive) peer VM.
        mock_client.expect_ping().return_once(|_request| {
            Err(GrpcError::RpcFailure(RpcStatus::new(RpcStatusCode::DEADLINE_EXCEEDED)))
        });

        let service_registry = Arc::new(ServiceRegistry::default());

        let tokio_runtime =
            Arc::new(tokio::runtime::Builder::new_current_thread().enable_all().build()?);

        let mut peer_connection = PeerConnection::try_new(
            VM_INSTANCE_2_NAME,
            mock_client,
            service_registry.clone(),
            tokio_runtime.clone(),
        )?;

        tokio_runtime.block_on(async move {
            wait_for_instance2_service_units_registration(service_registry).await.unwrap();

            let err = peer_connection
                .check_liveness()
                .await
                .expect_err("Ping failure should have caused check_liveness() to fail");

            assert_eq!(err.to_string(), format_ping_error(VM_INSTANCE_2_NAME));
        });

        Ok(())
    }
}
