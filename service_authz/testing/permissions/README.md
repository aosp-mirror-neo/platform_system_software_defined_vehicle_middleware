<!--
  Copyright (C) 2026 The Android Open Source Project
  Licensed under the Apache License, Version 2.0 (the "License");
  you may not use this file except in compliance with the License.
  You may obtain a copy of the License at

      http://www.apache.org/licenses/LICENSE-2.0

  Unless required by applicable law or agreed to in writing, software
  distributed under the License is distributed on an "AS IS" BASIS,
  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
  See the License for the specific language governing permissions and
  limitations under the License.
-->

# Testing Authz

This folder contains APEXes that are used for authorization e2e test coverage.

To be more specific, the following CUJs are covered:

-   CUJ-AuthZ-RPC-Negative:

    A Service bundle A implements a RPC interface. Service A declares the
    permissions to serve ('server') and call('client') the defined RPC interface.
    Service Bundle B doesn't declare 'client' permissions to connect to the
    defined RPC interface. A client in Service Bundle B tries to discover the RPC
    interface, and doesn’t get any information back from SD.

-   CUJ-AuthZ-DT-Negative:

    A Service bundle A implements a DT publisher. Service A declares the
    permissions to publish and subscribe to the topic of the DT publisher.
    Service Bundle B doesn't declare 'subscriber' permissions to connect to the
    defined DT topic. A subscriber in Service Bundle B tries to discover the DT
    publisher, and doesn’t get any information back from SD.

More over, some other important security scenarios are covered:

-   AuthZ-RPC-Connect-Negative

    Make sure that not authorized RPC client cannot connect to the RPC server
    even knowing the RPC server SID and service unit name, which are not
    provided by the Service Discovery agent.

    Note: This is not an official CUJ.

-   AuthZ-DT-Connect-Negative

    Make sure that not authorized DT subscriber cannot subscribe to the DT
    publisher even knowing the DT publisher SID and service unit name, which are
    not provided by the Service Discovery agent.

    Note: This is not an official CUJ.

-   VM-Level AuthZ-RPC-Negative

    A Service bundle implements a RPC interface. The VM-level authorization
    policy explicitly denies exporting this RPC server. A local client in the
    same VM can successfully connect to it, but a remote client in another
    VM cannot discover the RPC interface.

-   VM-Level AuthZ-DT-Negative

    A Service bundle implements a DT publisher. The VM-level authorization
    policy explicitly denies exporting this DT publisher. A local subscriber in
    the same VM can successfully subscribe to it, but a remote subscriber in
    another VM cannot discover the DT publisher.

Follow instructions below to manually start 2 Service Bundles that communicate
with each other and perform authorization tests.

## 1. Cleanup

```shell
cvd reset -y ; (yes | cvd rm) ; cvd clear

sdv-cf create --instance_name=instance1

sdv-cf create --instance_name=instance2

export SERIAL1=0.0.0.0:6520
export SERIAL2=0.0.0.0:6521
```

## 2. Prepare device

```shell
until adb -s $SERIAL1 root; do :; done

until adb -s $SERIAL2 root; do :; done
```

## 3. Start Foo

Run the following command and expect logs "RPC Server is running":

```shell
adb -s $SERIAL1 shell sdv_service_bundle create instance1:com.android.sdv.test.permissions.foo.AuthzTestedService/instance

adb -s $SERIAL1 shell sdv_service_bundle start instance1:com.android.sdv.test.permissions.foo.AuthzTestedService/instance
```

## 4. Start Bar

Run the following command and expect logs "Received response: hello_world":

```shell
adb -s $SERIAL2 shell sdv_service_bundle create instance1:com.android.sdv.test.permissions.bar.AuthzTestDriver/instance

adb -s $SERIAL2 shell sdv_service_bundle start instance1:com.android.sdv.test.permissions.bar.AuthzTestDriver/instance
```

## 5. Check logs

```shell
adb -s $SERIAL1 shell logcat -d *:F com_android_sdv_test_permissions_foo_AuthzTestedService_instance

adb -s $SERIAL2 shell logcat -d *:F com_android_sdv_test_permissions_bar_AuthzTestDriver_instance
```

Expected logs on VM 1 (SID will be different on every run):

```
sdv_comms::rpc::certificate_provider: TLS is enabled
sdv_comms::rpc::certificate_provider: New root certs received
sdv_comms::rpc::server: Setting up the RPC server with a path /com.android.sdv.test.permissions.foo.EchoRpcServerUnitType/
authz_tested_service_permissions: Non-connectable RPC server identifiers: SID 4755063177337670
sdv_comms::rpc::certificate_provider: TLS is enabled
sdv_comms::rpc::certificate_provider: New root certs received
sdv_comms::rpc::server: Setting up the RPC server with a path /com.android.sdv.test.permissions.foo.EchoRpcServerUnitType/
sdv_comms::rpc::certificate_provider: TLS is enabled
sdv_comms::rpc::certificate_provider: New root certs received
sdv_comms::rpc::server: Setting up the RPC server with a path /com.android.sdv.test.permissions.foo.EchoRpcServerUnitType/
authz_tested_service_permissions: Non-connectable DT publisher identifiers: SID 4755063177337670
sdv_comms::rpc::certificate_provider: TLS is enabled
sdv_comms::rpc::certificate_provider: New root certs received
authz_tested_service_permissions: Processing request
authz_tested_service_permissions: Successfully performed local request to VM denied server, response: hello_world
authz_tested_service_permissions: Successfully received message locally from VM denied publisher
authz_tested_service_permissions: RPC Servers are running
authz_tested_service_permissions: Processing request
```

Expected logs on VM 2 (SID will be different on every run):

```logs
authz_test_driver_permissions: Non-connectable RPC server was not discovered due to permissions configuration as it was expected.
authz_test_driver_permissions: Connectable RPC server identifiers: SID 4755063177337670, UnitName echo-rpc-server
sdv_comms::rpc::certificate_provider: TLS is enabled
sdv_comms::rpc::certificate_provider: New root certs received
authz_test_driver_permissions: Received response from connectable RPC server: instance1:com.android.sdv.test.permissions.foo.AuthzTestedService/instance
authz_test_driver_permissions: VM-denied RPC server was not discovered due to VM permissions configuration as it was expected.
sdv_comms::rpc::certificate_provider: TLS is enabled
sdv_comms::rpc::certificate_provider: New root certs received
sdv_comms::rpc::client: Could not obtain unary response: ACL check didn't allow this operation
authz_test_driver_permissions: Non-connectable RPC server denied connection due to permissions configuration as it was expected.
authz_test_driver_permissions: Non-connectable DT publisher was not discovered due to permissions configuration as it was expected.
authz_test_driver_permissions: Connectable DT publisher identifiers: SID 4755063177337670, UnitName ping-dt-publisher
authz_test_driver_permissions: Received non-connectable DT publisher SID: 4755063177337670
authz_test_driver_permissions: VM-denied DT publisher was not discovered due to VM permissions configuration as it was expected.
authz_test_driver_permissions: Non-connectable DT publisher was not discovered due to permissions configuration as it was expected.
authz_test_driver_permissions: Connectable DT publisher identifiers: SID 4755063177337670, UnitName ping-dt-publisher
authz_test_driver_permissions: Non-connectable DT publisher identifiers: SID 4755063177337670, UnitName ping-dt-publisher-non-connectable
sdv_comms_common::callback::ffi: Callback invocation failed with: SdvStatusCode::PermissionDenied(7) (caused by Status(-8, EX_SERVICE_SPECIFIC): '2: You aren't authorized to access the specifiedpublication!')
authz_test_driver_permissions: Non-connectable DT publisher denied connection due to permissions configuration as it was expected.
```
