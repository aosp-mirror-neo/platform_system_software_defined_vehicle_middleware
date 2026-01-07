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

//! Service Bundle Authz Tested Service.

use futures::stream::StreamExt;
use lazy_static::lazy_static;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::io::Write;
use std::str::FromStr;
use std::sync::Arc;
use std::thread::JoinHandle;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use sdv::comms::dt::Publisher;
use sdv::comms::rpc::{Server, ServerOptions, UnaryRpcMethod};
use sdv::comms::sd::PublisherUnitMetadata;
use sdv::comms::sd::ServerUnitMetadata;
use sdv::comms::sd::ServiceDiscoveryManager;
use sdv::comms::sd::ServiceUnitDefinition;
use sdv::comms::sd::UnitMetadata;
use sdv::comms::sd::UnitNameDiscoveryArgs;
use sdv::comms::sd::UnitRegistrationArgs;
use sdv::comms::sd::UnitType;
use sdv::status::SdvResult;
use sdv::status::{SdvStatus, SdvStatusCode};

/// Service bundle Authz Tested Service.
pub struct ServiceBundleTestedService {
    // Entry point for comm stack APIs.
    context: sdv::comms::ContextRef,
    // Used to shutdown a runtime after on_stop request
    cancellation_token: Option<CancellationToken>,
    // Execution thread to get results when the bundle is stopped.
    execution_thread: Option<JoinHandle<()>>,
}

// Register the new service bundle.
sdv::lifecycle::register_service_bundle!(ServiceBundleTestedService);

impl ServiceBundle for ServiceBundleTestedService {
    /// Creates a new instance of the ServiceBundleTestedService.
    /// Called when service bundle is created by the system.
    #[instrument(skip(context), fields(fqin = %context.get_self_fqin()))]
    fn new(context: ContextRef) -> ServiceBundleTestedService {
        // Init tracing.
        if let Err(err) = sdv_tracing::try_init_tracing() {
            warn!("Failed to init service bundles tracing: {err}");
        }

        debug!("Creating {}.", context.get_self_fqin());
        ServiceBundleTestedService { context, cancellation_token: None, execution_thread: None }
    }

    /// Called when the service bundle is started by the system.
    #[instrument(skip(self), fields(fqin = %self.context.get_self_fqin()))]
    fn on_start(&mut self) {
        // Dynamic resources(sockets, files, etc) should be allocated during this call.
        debug!("Starting {}.", self.context.get_self_fqin());

        // Spawn new execution thread to avoid blocking the `on_start` method.
        // New thread creates tokio runtime and uses Comm Stack for communication.
        let cancellation_token = CancellationToken::new();
        self.cancellation_token = Some(cancellation_token.clone());

        let new_thread = spawn_execution_thread(cancellation_token, self.context);
        self.execution_thread = Some(new_thread);
    }

    /// Called when the service bundle is stopped by the system in preparation
    /// for shutdown or suspend to RAM/Disc.
    #[instrument(skip(self), fields(fqin = %self.context.get_self_fqin()))]
    fn on_stop(&mut self) {
        // Stop phase requires the service bundle to delete the dynamic resources
        // (sockets, files, etc) that were previously allocated in the [Self::on_start()] method.
        debug!("Stopping {}.", self.context.get_self_fqin());
    }
}

#[instrument(skip_all)]
fn spawn_execution_thread(
    cancellation_token: CancellationToken,
    context: ContextRef,
) -> JoinHandle<()> {
    let Ok(runtime) =
        tokio::runtime::Builder::new_current_thread().enable_time().enable_io().build()
    else {
        panic!("Failed to start the tokio runtime!");
    };

    std::thread::spawn(move || {
        // Due to async nature of Comms APIs we have
        // to execute a service bundle inside a runtime (Tokio).
        runtime.block_on(async move {
            debug!("Starting {} execution loop", context.get_self_fqin());
            // Call `service_main` which contains a business logic.
            // The function is not supposed to return.
            // The execution must be halted only by `on_stop` request coming from the
            // lifecycle manager.
            let res = tokio::select! {
                res = service_main(context) => res,
                () = cancellation_token.cancelled() => Err(SdvStatus::new(SdvStatusCode::Cancelled)),
            };
            debug!("Stopped {} execution loop with result: {res:?}.", context.get_self_fqin());
        });
        // Dropping tokio runtime immediately stops all the execution of a business logic
        // and gracefully free all the allocated resources.
        // Discovering of service units offered by this service bundle are no longer possible.
        //
        // This does not stop a thread created with `tokio::spawn_blocking` or `std::thread::spawn`.
        // The created execution thread will be joined in `onStop` method.
        drop(runtime);
    })
}

// 'service_main' is supposed to serve as an entry point for all the business logic of a service.
#[instrument(skip_all)]
async fn service_main(context: ContextRef) -> SdvResult<()> {
    // RPC servers.

    let _non_connectable_server = create_non_connectable_server(context).await?;

    let non_connectable_server_sid = discover_non_connectable_server(context).await?;
    info!("Non-connectable RPC server identifiers: SID {non_connectable_server_sid}");

    let _server = create_server(context).await?;

    // DT publishers.

    let non_connectable_publisher = create_non_connectable_publisher(context).await?;

    let non_connectable_publisher_sid = discover_non_connectable_publisher(context).await?;
    info!("Non-connectable DT publisher identifiers: SID {non_connectable_publisher_sid}");

    let publisher = create_publisher(context).await?;
    publish_message(&publisher, &non_connectable_publisher_sid.to_string())?;
    publish_message(&non_connectable_publisher, "This message should not be received")?;

    // Publish messages in the loop and log status update that
    // the service bundle is running.

    for _ in 1..10 {
        publish_message(&publisher, &non_connectable_publisher_sid.to_string())?;
        publish_message(&non_connectable_publisher, "This message should not be received")?;

        info!("RPC Servers are running");
        std::io::stdout().flush().unwrap();
        sleep(Duration::from_secs(10)).await;
    }

    info!("Stopping RPC Servers after 100 seconds");

    Ok(())
}

static SERVER_UNIT_TYPE_STR: &str =
    "com.sdv.google.test.authz.foo.AuthzTestedService#EchoRpcServerUnitType";
static SERVER_SERVICE_UNIT_NAME: &str = "echo-rpc-server";

lazy_static! {
    static ref SERVER_UNIT_TYPE: UnitType =
        UnitType::from_str(SERVER_UNIT_TYPE_STR).expect("Expect Valid UnitType");
}

async fn create_server(context: ContextRef) -> SdvResult<Server> {
    let mut unit_registration_args = UnitRegistrationArgs::new();
    {
        let mut server_unit_metadata = ServerUnitMetadata::new();
        server_unit_metadata.set_version(1);

        unit_registration_args.set_unit_name(SERVER_SERVICE_UNIT_NAME);
        unit_registration_args.set_unit_type(&SERVER_UNIT_TYPE);
        unit_registration_args.set_unit_metadata(&UnitMetadata::Server(server_unit_metadata));
    }
    let method: UnaryRpcMethod = Arc::new(move |_, _x: &[u8]| {
        info!("Processing request");
        let response = context.get_self_fqin().to_string().into_bytes();
        Box::pin(async move { Ok(response) })
    });

    let methods_map: HashMap<String, UnaryRpcMethod> =
        HashMap::from([("echo".to_string(), method)]);
    let server =
        Server::create(context, unit_registration_args, methods_map, &ServerOptions::default())
            .await?;
    Ok(server)
}

static NON_CONNECTABLE_SERVER_PACKAGE_NAME: &str = "com.sdv.google.test.authz.foo";
static NON_CONNECTABLE_SERVER_SERVICE_BUNDLE_NAME: &str = "AuthzTestedService";
static NON_CONNECTABLE_SERVER_SERVICE_UNIT_NAME: &str = "echo-rpc-server-non-connectable";

// Creates an RPC server that does not authorize access to any RPC clients
// except the current service bundle.
//
// This RPC server is used to test that clients cannot discover or connect to it
// (knowing IP address and port).
async fn create_non_connectable_server(context: ContextRef) -> SdvResult<Server> {
    let mut unit_registration_args = UnitRegistrationArgs::new();
    {
        let mut server_unit_metadata = ServerUnitMetadata::new();
        server_unit_metadata.set_version(1);

        unit_registration_args.set_unit_name(NON_CONNECTABLE_SERVER_SERVICE_UNIT_NAME);
        unit_registration_args.set_unit_type(&SERVER_UNIT_TYPE);
        unit_registration_args.set_unit_metadata(&UnitMetadata::Server(server_unit_metadata));
    }
    let method: UnaryRpcMethod = Arc::new(|_, _x: &[u8]| {
        panic!("Non connectable server should not receive any requests");
    });

    let methods_map: HashMap<String, UnaryRpcMethod> =
        HashMap::from([("echo".to_string(), method)]);
    let server =
        Server::create(context, unit_registration_args, methods_map, &ServerOptions::default())
            .await?;
    Ok(server)
}

// Discover an RPC server created by `create_non_connectable_server()`.
//
// Returns SID of that RPC server.
async fn discover_non_connectable_server(context: ContextRef) -> SdvResult<u64> {
    let name_args = UnitNameDiscoveryArgs::new_builder()
        .set_sdv_package_name(NON_CONNECTABLE_SERVER_PACKAGE_NAME)
        .set_service_bundle_name(NON_CONNECTABLE_SERVER_SERVICE_BUNDLE_NAME)
        .set_service_unit_name(NON_CONNECTABLE_SERVER_SERVICE_UNIT_NAME.to_string())
        .build()
        .expect("UnitNameDiscoveryArgs build() failed");

    let discovery_result: Vec<ServiceUnitDefinition> =
        ServiceDiscoveryManager::new(context).list_units_by_name(&name_args).collect().await;

    assert_eq!(
        discovery_result.len(),
        1,
        "Unexpected service discovery result. Server must be started before client"
    );
    let server_identifier = discovery_result[0].get_unit_identifier();
    assert_eq!(server_identifier.get_unit_name(), NON_CONNECTABLE_SERVER_SERVICE_UNIT_NAME);

    Ok(server_identifier.get_sid())
}

const PUBLISHER_MESSAGE_SIZE: u32 = 128;
const PUBLISHER_MESSAGE_COUNT: u32 = 16;

static PUBLISHER_UNIT_TYPE_STR: &str =
    "com.sdv.google.test.authz.foo.AuthzTestedService#PingDtPublisherUnitType";
static PUBLISHER_SERVICE_UNIT_NAME: &str = "ping-dt-publisher";

lazy_static! {
    static ref PUBLISHER_UNIT_TYPE: UnitType =
        UnitType::from_str(PUBLISHER_UNIT_TYPE_STR).expect("Expect Valid UnitType");
}

async fn create_publisher(context: ContextRef) -> SdvResult<Publisher> {
    let mut unit_registration_args = UnitRegistrationArgs::new();
    let mut publisher_metadata = PublisherUnitMetadata::new();
    publisher_metadata.set_message_size(PUBLISHER_MESSAGE_SIZE);
    publisher_metadata.set_message_count(PUBLISHER_MESSAGE_COUNT);

    unit_registration_args.set_unit_type(&PUBLISHER_UNIT_TYPE);
    unit_registration_args.set_unit_metadata(&UnitMetadata::Publisher(publisher_metadata));
    unit_registration_args.set_unit_name(PUBLISHER_SERVICE_UNIT_NAME);

    Publisher::create(context, unit_registration_args).await
}

fn publish_message(publisher: &Publisher, msg: &str) -> SdvResult<()> {
    let publisher_msg = resize_to_size(msg.as_bytes(), PUBLISHER_MESSAGE_SIZE);

    let written = publisher.write_messages(&publisher_msg)?;
    if written != 1 {
        return Err(SdvStatus::with_message(
            SdvStatusCode::Internal,
            format!("publisher.write_messages() returns {written}, expected 1."),
        ));
    }
    Ok(())
}

fn resize_to_size(data: &[u8], size: u32) -> Vec<u8> {
    let mut resized_data = vec![0u8; size as usize];

    let copy_length = std::cmp::min(data.len(), size as usize);

    resized_data[..copy_length].copy_from_slice(&data[..copy_length]);

    resized_data
}

static NON_CONNECTABLE_PUBLISHER_PACKAGE_NAME: &str = "com.sdv.google.test.authz.foo";
static NON_CONNECTABLE_PUBLISHER_SERVICE_BUNDLE_NAME: &str = "AuthzTestedService";
static NON_CONNECTABLE_PUBLISHER_SERVICE_UNIT_NAME: &str = "ping-dt-publisher-non-connectable";

// Creates a DT publisher that does not authorize access to any DT subscriber
// except the current service bundle.
//
// This DT publisher is used to test that clients cannot discover or connect
// to it (knowing SID and service unit name).
async fn create_non_connectable_publisher(context: ContextRef) -> SdvResult<Publisher> {
    let mut unit_registration_args = UnitRegistrationArgs::new();
    let mut publisher_metadata = PublisherUnitMetadata::new();
    publisher_metadata.set_message_size(PUBLISHER_MESSAGE_SIZE);
    publisher_metadata.set_message_count(PUBLISHER_MESSAGE_COUNT);

    unit_registration_args.set_unit_type(&PUBLISHER_UNIT_TYPE);
    unit_registration_args.set_unit_metadata(&UnitMetadata::Publisher(publisher_metadata));
    unit_registration_args.set_unit_name(NON_CONNECTABLE_PUBLISHER_SERVICE_UNIT_NAME);

    Publisher::create(context, unit_registration_args).await
}

// Discover an DT publisher created by `create_non_connectable_publisher()`.
//
// Returns SID of that DT publisher.
async fn discover_non_connectable_publisher(context: ContextRef) -> SdvResult<u64> {
    let name_args = UnitNameDiscoveryArgs::new_builder()
        .set_sdv_package_name(NON_CONNECTABLE_PUBLISHER_PACKAGE_NAME)
        .set_service_bundle_name(NON_CONNECTABLE_PUBLISHER_SERVICE_BUNDLE_NAME)
        .set_service_unit_name(NON_CONNECTABLE_PUBLISHER_SERVICE_UNIT_NAME.to_string())
        .build()
        .expect("UnitNameDiscoveryArgs build() failed");

    let discovery_result: Vec<ServiceUnitDefinition> =
        ServiceDiscoveryManager::new(context).list_units_by_name(&name_args).collect().await;

    assert_eq!(
        discovery_result.len(),
        1,
        "Unexpected service discovery result. Publisher must be started before client"
    );
    let server_identifier = discovery_result[0].get_unit_identifier();
    assert_eq!(server_identifier.get_unit_name(), NON_CONNECTABLE_PUBLISHER_SERVICE_UNIT_NAME);

    Ok(server_identifier.get_sid())
}
