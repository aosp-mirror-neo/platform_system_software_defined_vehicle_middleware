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

    A Service bundle A implements a RPC interface and declares an empty ACL
    definition. A local client and a remote client try to discover the RPC
    interface, and doesn’t get any information back from SD.

-   CUJ-AuthZ-DT-Negative:

    A Service bundle A implements a DT interface and declares an empty ACL
    definition. A local client and a remote client try to discover the DT
    interface, and doesn’t get any information back from SD.

More over, some other important security scenarios are covered:

-   AuthZ-RPC-Connect-Negative

    Make sure that not authorized RPC client cannot connect to the RPC server
    even knowing the RPC server SID and service unit name, which are not
    provided by the Service Discovery agent.

    Note: This is not an official CUJ.

-   AuthZ-RPC-Connect-Negative

    Make sure that not authorized DT subscriber cannot subscribe to the DT
    publisher even knowing the DT publisher SID and service unit name, which are
    not provided by the Service Discovery agent.

    Note: This is not an official CUJ.

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
adb -s $SERIAL1 shell sdv_service_bundle create instance1:com.sdv.google.test.authz.foo.AuthzTestedService/instance

adb -s $SERIAL1 shell sdv_service_bundle start instance1:com.sdv.google.test.authz.foo.AuthzTestedService/instance
```

## 4. Start Bar

Run the following command and expect logs "Received response: hello_world":

```shell
adb -s $SERIAL2 shell sdv_service_bundle create instance1:com.android.sdv.test.authz.bar.AuthzTestDriver/instance

adb -s $SERIAL2 shell sdv_service_bundle start instance1:com.android.sdv.test.authz.bar.AuthzTestDriver/instance
```

## 5. Check logs

```shell
adb -s $SERIAL1 shell logcat -d *:F com_sdv_google_test_authz_foo_AuthzTestedService_instance | grep authz_test

adb -s $SERIAL2 shell logcat -d *:F com_android_sdv_test_authz_bar_AuthzTestDriver_instance | grep authz_test
```

Expected logs on VM 1 (SID will be different on every run):

```logs
com_sdv_google_test_authz_foo_AuthzTestedService_instance: sdv_comms::rpc::certificate_provider: New root certs received
com_sdv_google_test_authz_foo_AuthzTestedService_instance: authz_tested_service: Non-connectable RPC server identifiers: SID 9264256277222008612
com_sdv_google_test_authz_foo_AuthzTestedService_instance: sdv_comms::rpc::certificate_provider: TLS is enabled
com_sdv_google_test_authz_foo_AuthzTestedService_instance: sdv_comms::rpc::certificate_provider: New root certs received
com_sdv_google_test_authz_foo_AuthzTestedService_instance: authz_tested_service: Non-connectable DT publisher identifiers: SID 9264256277222008612
com_sdv_google_test_authz_foo_AuthzTestedService_instance: authz_tested_service: RPC Servers are running
com_sdv_google_test_authz_foo_AuthzTestedService_instance: authz_tested_service: Processing request
```

Expected logs on VM 2 (SID will be different on every run):

```logs
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Non-connectable RPC server was not discovered due to ACLs configuration as it was expected.
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Connectable RPC server identifiers: SID 9264256277222008612, UnitName echo-rpc-server
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: sdv_comms::rpc::certificate_provider: TLS is enabled
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: sdv_comms::rpc::certificate_provider: New root certs received
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Received response from connectable RPC server: 9264256277222008612$instance1:com.sdv.google.test.authz.foo.AuthzTestedService/instance
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: sdv_comms::rpc::certificate_provider: TLS is enabled
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: sdv_comms::rpc::certificate_provider: New root certs received
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: sdv_comms::rpc::client: Could not obtain unary response: ACL check didn't allow this operation
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Non-connectable RPC server denied connection due to ACLs configuration as it was expected.
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Non-connectable DT publisher was not discovered due to ACLs configuration as it was expected.
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Connectable DT publisher identifiers: SID 9264256277222008612, UnitName ping-dt-publisher
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Received non-connectable DT publisher SID: 9264256277222008612
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Non-connectable DT publisher was not discovered due to ACLs configuration as it was expected.
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Connectable DT publisher identifiers: SID 9264256277222008612, UnitName ping-dt-publisher
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Non-connectable DT publisher identifiers: SID 9264256277222008612, UnitName ping-dt-publisher-non-connectable
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Non-connectable DT publisher denied connection due to ACLs configuration as it was expected.
```
