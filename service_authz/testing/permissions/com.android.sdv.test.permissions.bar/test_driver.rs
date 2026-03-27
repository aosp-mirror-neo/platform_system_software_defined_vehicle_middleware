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

//! Service Bundle Authz Test Driver.

use futures::stream::StreamExt;
use lazy_static::lazy_static;
use log::{debug, error, info, warn};
use std::io::Write;
use std::str::FromStr;
use std::thread::JoinHandle;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use sdv::comms::dt::SubscriberOptions;
use sdv::comms::dt::SubscriberStream;
use sdv::comms::id::ServiceFqin;
use sdv::comms::id::ServiceIdentity;
use sdv::comms::id::UnitIdentifier;
use sdv::comms::rpc::{Client, ClientOptions};
use sdv::comms::sd::ServiceDiscoveryManager;
use sdv::comms::sd::ServiceUnitDefinition;
use sdv::comms::sd::UnitType;
use sdv::status::SdvResult;
use sdv::status::{SdvStatus, SdvStatusCode};

/// Service bundle Test Driver.
pub struct ServiceBundleTestDriver {
    // Entry point for comm stack APIs.
    context: sdv::comms::ContextRef,
    // Used to shutdown a runtime after on_stop request
    cancellation_token: Option<CancellationToken>,
    // Execution thread to get results when the bundle is stopped.
    execution_thread: Option<JoinHandle<()>>,
}

// Register the new service bundle.
sdv::lifecycle::register_service_bundle!(ServiceBundleTestDriver);

impl ServiceBundle for ServiceBundleTestDriver {
    /// Creates a new instance of the ServiceBundleTestDriver.
    /// Called when service bundle is created by the system.
    #[instrument(skip(context), fields(fqin = %context.get_self_fqin()))]
    fn new(context: ContextRef) -> ServiceBundleTestDriver {
        // Init tracing.
        if let Err(err) = sdv_tracing::try_init_tracing() {
            warn!("Failed to init service bundles tracing: {err}");
        }

        debug!("Creating {}.", context.get_self_fqin());
        ServiceBundleTestDriver { context, cancellation_token: None, execution_thread: None }
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
    let non_connectable_server_fqin = fetch_non_connectable_server_fqin(context).await?;

    check_vm_denied_server_not_discoverable(context).await?;

    test_request_to_non_connectable_server(context, non_connectable_server_fqin).await?;

    let non_connectable_publisher_sid = fetch_non_connectable_publisher_sid(context).await?;

    check_vm_denied_publisher_not_discoverable(context).await?;

    test_subscribe_to_non_connectable_publisher(context, non_connectable_publisher_sid).await?;

    Ok(())
}

static TESTED_SERVICE_PACKAGE_NAME: &str = "com.android.sdv.test.permissions.foo";
static TESTED_SERVICE_BUNDLE_NAME: &str = "AuthzTestedService";

static TESTED_SERVER_UNIT_TYPE_STR: &str =
    "com.android.sdv.test.permissions.foo.AuthzTestedService#EchoRpcServerUnitType";

lazy_static! {
    static ref TESTED_SERVER_UNIT_TYPE: UnitType =
        UnitType::from_str(TESTED_SERVER_UNIT_TYPE_STR).expect("Expect Valid UnitType");
}

static TESTED_SERVER_SERVICE_UNIT: &str = "echo-rpc-server";

async fn wait_for_server_to_start(context: ContextRef) -> SdvResult<UnitIdentifier> {
    for i in 1..100 {
        let discovery_result: Vec<ServiceUnitDefinition> = ServiceDiscoveryManager::new(context)
            .list_units_by_type(&TESTED_SERVER_UNIT_TYPE)
            .collect()
            .await;

        if discovery_result.is_empty() {
            info!("Attempt {i} to discover connectable RPC server failed.");
            std::io::stdout().flush().unwrap();
            sleep(Duration::from_secs(1)).await;
            continue;
        }

        assert_eq!(
            discovery_result.len(),
            1,
            "Unexpected service discovery result: {discovery_result:?}"
        );

        let server_identifier = discovery_result[0].get_unit_identifier();
        assert_eq!(server_identifier.get_unit_name(), TESTED_SERVER_SERVICE_UNIT);

        info!("Non-connectable RPC server was not discovered due to permissions configuration as it was expected.");

        info!(
            "Connectable RPC server identifiers: SID {}, UnitName {}",
            server_identifier.get_sid(),
            server_identifier.get_unit_name()
        );
        return Ok(server_identifier);
    }

    let err_str = "Failed to wait for server to start";
    error!("{err_str}");
    Err(SdvStatus::with_message(SdvStatusCode::Internal, err_str))
}

async fn create_client(context: ContextRef) -> SdvResult<Client> {
    let server_identifier = wait_for_server_to_start(context).await?;

    Client::create(context, server_identifier, &ClientOptions::default()).await
}

// Performs an RPC request to the connectable RPC server to fetch the FQIN of
// the non-connectable RPC server.
async fn fetch_non_connectable_server_fqin(context: ContextRef) -> SdvResult<ServiceFqin> {
    let client = create_client(context).await?;

    let response = client.unary_call("echo", b"hello_world").await.map_err(|err| {
        let err_str = format!("Failed to perform RPC request: {err}");
        error!("{err_str}");
        SdvStatus::with_message(SdvStatusCode::Internal, err_str)
    })?;

    let response_str = std::str::from_utf8(&response).map_err(|err| {
        let err_str = format!("Failed to convert RPC response to UTF-8: {err}");
        error!("{err_str}");
        SdvStatus::with_message(SdvStatusCode::Internal, err_str)
    })?;

    info!("Received response from connectable RPC server: {response_str}");

    let non_connectable_server_fqin = ServiceFqin::from_str(response_str).map_err(|err| {
        let err_str = format!("Failed to parse non_connectable_server_fqin: {err}");
        error!("{err_str}");
        SdvStatus::with_message(SdvStatusCode::Internal, err_str)
    })?;

    Ok(non_connectable_server_fqin)
}

// This function creates a client to the non-connectable server and performs
// an RPC request to the non-connectable RPC server. It is expected that
// RPC call fails due to ACL configuration which denies connection.
async fn test_request_to_non_connectable_server(
    context: ContextRef,
    non_connectable_server_fqin: ServiceFqin,
) -> SdvResult<()> {
    let non_connectable_client =
        create_non_connectable_client(context, non_connectable_server_fqin).await?;

    match non_connectable_client.unary_call("echo", b"hello_world").await {
        Ok(res) => {
            let err_str = format!("Received response from non-connectable RPC server meaning that permissions are not enforced: {res:?}");
            error!("{err_str}");
            Err(SdvStatus::with_message(SdvStatusCode::Internal, err_str))
        }
        Err(err) => {
            if err.status_code() != SdvStatusCode::PermissionDenied
                || !err.to_string().contains("ACL check didn't allow this operation")
            {
                let err_str = format!("Unexpected error occurred while performing RPC request to non-connectable RPC server: {err}");
                error!("{err_str}");
                Err(SdvStatus::with_message(SdvStatusCode::Internal, err_str))
            } else {
                info!("Non-connectable RPC server denied connection due to permissions configuration as it was expected.");
                Ok(())
            }
        }
    }
}

static VM_DENIED_SERVER_SERVICE_UNIT_NAME: &str = "echo-rpc-server-vm-denied";

async fn check_vm_denied_server_not_discoverable(context: ContextRef) -> SdvResult<()> {
    use sdv::comms::sd::UnitNameDiscoveryArgs;

    let name_args = UnitNameDiscoveryArgs::new_builder()
        .set_sdv_package_name(TESTED_SERVICE_PACKAGE_NAME)
        .set_service_bundle_name(TESTED_SERVICE_BUNDLE_NAME)
        .set_service_unit_name(VM_DENIED_SERVER_SERVICE_UNIT_NAME.to_string())
        .build()
        .expect("UnitNameDiscoveryArgs build() failed");

    let discovery_result: Vec<ServiceUnitDefinition> =
        ServiceDiscoveryManager::new(context).list_units_by_name(&name_args).collect().await;

    if !discovery_result.is_empty() {
        let err_str =
            format!("VM denied RPC server was unexpectedly discovered: {:?}", discovery_result);
        error!("{err_str}");
        return Err(SdvStatus::with_message(SdvStatusCode::Internal, err_str));
    }

    info!("VM-denied RPC server was not discovered due to VM permissions configuration as it was expected.");
    Ok(())
}

static TESTED_SERVER_NON_CONNECTABLE_SERVICE_UNIT: &str = "echo-rpc-server-non-connectable";

async fn create_non_connectable_client(
    context: ContextRef,
    target_fqin: ServiceFqin,
) -> SdvResult<Client> {
    sdv::comms::rpc::client::internal::create(
        context,
        sdv::comms::rpc::client::internal::TargetServiceInfo {
            service_unit_name: TESTED_SERVER_NON_CONNECTABLE_SERVICE_UNIT.to_string(),
            unit_type: TESTED_SERVER_UNIT_TYPE.clone(),
            fqin: target_fqin,
        },
        &ClientOptions::default(),
    )
    .await
}

static TESTED_PUBLISHER_UNIT_TYPE_STR: &str =
    "com.android.sdv.test.permissions.foo.AuthzTestedService#PingDtPublisherUnitType";

lazy_static! {
    static ref TESTED_PUBLISHER_UNIT_TYPE: UnitType =
        UnitType::from_str(TESTED_PUBLISHER_UNIT_TYPE_STR).expect("Expect Valid UnitType");
}

static TESTED_PUBLISHER_SERVICE_UNIT: &str = "ping-dt-publisher";

async fn wait_for_publisher_to_start(
    context: ContextRef,
) -> SdvResult<(ServiceIdentity, UnitIdentifier)> {
    for i in 1..100 {
        let discovery_result: Vec<ServiceUnitDefinition> = ServiceDiscoveryManager::new(context)
            .list_units_by_type(&TESTED_PUBLISHER_UNIT_TYPE)
            .collect()
            .await;

        if discovery_result.is_empty() {
            info!("Attempt {i} to discover connectable DT publisher failed.");
            std::io::stdout().flush().unwrap();
            sleep(Duration::from_secs(1)).await;
            continue;
        }

        assert_eq!(
            discovery_result.len(),
            1,
            "Unexpected service discovery result: {discovery_result:?}"
        );

        let publisher_identifier = discovery_result[0].get_unit_identifier();
        assert_eq!(publisher_identifier.get_unit_name(), TESTED_PUBLISHER_SERVICE_UNIT);

        info!("Non-connectable DT publisher was not discovered due to permissions configuration as it was expected.");

        info!(
            "Connectable DT publisher identifiers: SID {}, UnitName {}",
            publisher_identifier.get_sid(),
            publisher_identifier.get_unit_name()
        );
        return Ok((discovery_result[0].get_service_bundle_identity(), publisher_identifier));
    }

    let err_str = "Failed to wait for server to start";
    error!("{err_str}");
    Err(SdvStatus::with_message(SdvStatusCode::Internal, err_str))
}

async fn create_subscription(context: ContextRef) -> SdvResult<SubscriberStream> {
    let (_, publisher_identifier) = wait_for_publisher_to_start(context).await?;

    let subscriber = SubscriberStream::create(
        context,
        &publisher_identifier,
        Duration::ZERO,
        SubscriberOptions { fetch_last_message: true },
    )
    .await?;

    Ok(subscriber)
}

const PUBLISHER_MESSAGE_SIZE: usize = 128;

async fn fetch_non_connectable_publisher_sid(context: ContextRef) -> SdvResult<u64> {
    let mut subscriber = create_subscription(context).await?;

    let msg = subscriber.next().await.ok_or_else(|| {
        SdvStatus::with_message(SdvStatusCode::Internal, "Failed to receive DT message".to_string())
    })?;

    let (messages_read, data) = msg?;

    assert_eq!(messages_read, 1);
    assert_eq!(data.len(), PUBLISHER_MESSAGE_SIZE);

    let sid = parse_u64_from_utf8_with_trailing_zeros(&data).map_err(|err| {
        SdvStatus::with_message(
            SdvStatusCode::Internal,
            format!("Failed to parse SID from message ({data:?}): {err}"),
        )
    })?;

    info!("Received non-connectable DT publisher SID: {sid}");

    Ok(sid)
}

fn parse_u64_from_utf8_with_trailing_zeros(s: &[u8]) -> SdvResult<u64> {
    // Find the first null byte (0).
    let end_index = s.iter().position(|&byte| byte == 0).unwrap_or(s.len());

    // Slice the string up to the null byte or the end.
    let number_str = std::str::from_utf8(&s[..end_index])
        .map_err(|err| SdvStatus::with_cause(SdvStatusCode::Internal, err))?;

    // Parse the u64.
    number_str.parse::<u64>().map_err(|err| SdvStatus::with_cause(SdvStatusCode::Internal, err))
}

static VM_DENIED_PUBLISHER_SERVICE_UNIT_NAME: &str = "ping-dt-publisher-vm-denied";

async fn check_vm_denied_publisher_not_discoverable(context: ContextRef) -> SdvResult<()> {
    use sdv::comms::sd::UnitNameDiscoveryArgs;

    let name_args = UnitNameDiscoveryArgs::new_builder()
        .set_sdv_package_name(TESTED_SERVICE_PACKAGE_NAME)
        .set_service_bundle_name(TESTED_SERVICE_BUNDLE_NAME)
        .set_service_unit_name(VM_DENIED_PUBLISHER_SERVICE_UNIT_NAME.to_string())
        .build()
        .expect("UnitNameDiscoveryArgs build() failed");

    let discovery_result: Vec<ServiceUnitDefinition> =
        ServiceDiscoveryManager::new(context).list_units_by_name(&name_args).collect().await;

    if !discovery_result.is_empty() {
        let err_str =
            format!("VM denied DT publisher was unexpectedly discovered: {:?}", discovery_result);
        error!("{err_str}");
        return Err(SdvStatus::with_message(SdvStatusCode::Internal, err_str));
    }

    info!("VM-denied DT publisher was not discovered due to VM permissions configuration as it was expected.");
    Ok(())
}

static NON_CONNECTABLE_PUBLISHER_SERVICE_UNIT_NAME: &str = "ping-dt-publisher-non-connectable";

// This function creates a subscriber to the non-connectable DT publisher.
//
// It is expected that fails due to ACL configuration which denies
// connection.
async fn test_subscribe_to_non_connectable_publisher(
    context: ContextRef,
    non_connectable_publisher_sid: u64,
) -> SdvResult<()> {
    let (publisher_service_identity, _) = wait_for_publisher_to_start(context).await?;

    // Create identifier of the non-connectable publisher by keeping service
    // identity of the connectable publisher and using service unit name of
    // the non-connectable publisher.
    let publisher_identifier = UnitIdentifier::new(
        publisher_service_identity,
        NON_CONNECTABLE_PUBLISHER_SERVICE_UNIT_NAME,
    );

    info!(
        "Non-connectable DT publisher identifiers: SID {}, UnitName {}",
        publisher_identifier.get_sid(),
        publisher_identifier.get_unit_name()
    );
    assert_eq!(publisher_identifier.get_sid(), non_connectable_publisher_sid);

    match SubscriberStream::create(
        context,
        &publisher_identifier,
        Duration::ZERO,
        SubscriberOptions { fetch_last_message: true },
    )
    .await
    {
        Ok(_) => {
            let err_str = "Received response from non-connectable DT publisher meaning that permissions are not enforced";
            error!("{err_str}");
            Err(SdvStatus::with_message(SdvStatusCode::Internal, err_str))
        }
        Err(err) => {
            // Error message is not propagated through C ABI layer, thus only
            // the status code can be verified.
            if err.status_code() != SdvStatusCode::PermissionDenied {
                let err_str = format!("Unexpected error occurred while subscribing to non-connectable DT publisher: {err}");
                error!("{err_str}");
                Err(SdvStatus::with_message(SdvStatusCode::Internal, err_str))
            } else {
                info!("Non-connectable DT publisher denied connection due to permissions configuration as it was expected.");
                Ok(())
            }
        }
    }
}
