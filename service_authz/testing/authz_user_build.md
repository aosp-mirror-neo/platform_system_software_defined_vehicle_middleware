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

# Authorization enforcement test on a `user` build

This test verifies that authorization enforcement cannot be disabled on a `user`
build.

This test uses 2 testing service bundles that verify both positive and negative
paths for the authorization. If authorization enforcement will be disabled, then
this test should fail.

## Test configuration

The test configuration consists of 2 testing service bundles: TestedService and
TestDriver. These testing service bundles could be used for one VM or multiple
VMs scenario.

TestedService exposes:

-   2 RPC servers that implement the same unit type
    -   1 RPC server (connectable) does authorize TestDriver RPC client
    -   1 RPC server (non-connectable) does NOT authorize TestDriver RPC client
-   2 DT publishers that implement the same unit type
    -   1 DT publisher (connectable) does authorize TestDriver DT subscriber
    -   1 DT publisher (non-connectable) does NOT authorize TestDriver DT
        subscriber

TestDriver performs the following sub tests:

-   CUJ-AuthZ-RPC-Negative

    Discover RPC clients by unit type name. Expect to discover only connectable
    RPC server.

-   AuthZ-RPC-Connect-Positive

    Connect to the connectable RPC server and perform an RPC request to fetch
    SID and FQIN of the non-connectable RPC server.

-   AuthZ-RPC-Connect-Negative

    Connect to the non-connectable RPC server using fetched SID and FQIN. Expect
    to get an authorization error.

-   CUJ-AuthZ-DT-Negative

    Discover DT publishers by unit type name. Expect to discover only
    connectable DT publisher.

-   AuthZ-DT-Connect-Positive

    Subscribe to the connectable DT publisher and receive SID of the
    non-connectable DT publisher.

-   AuthZ-DT-Connect-Negative

    Subscribe to the non-connectable DT publisher using received SID. Expect to
    get an authorization error.

## 1. Create user build

Build `user` image, not `userdebug`:

```shell
source build/envsetup.sh
lunch sdv_core_cf-trunk_staging-user
m
```

## 2. Cleanup

Reset previously created VMs:

```shell
cvd reset -y ; (yes | cvd rm) ; cvd clear
```

## 3. Start VM

When starting a VM, we need to provide an authorization enforcement flag to
disable the authorization. This flag should be ignored in the `user` build and
this is exactly what we want to verify.

Also on a `user` build, adb is disabled by default and we need to
[enable adb](go/sdv-user-build-testing).

Start VM:

```shell
cvd create \
  --extra_kernel_cmdline="androidboot.sdv.authz.enable=false" \
  --vendor_boot_image=$ANDROID_PRODUCT_OUT/vendor_boot-debug.img \
  --extra_bootconfig_args="androidboot.sdv.boot_mode=unlocked androidboot.sdv.instance_name=instance1"
```

<!-- TODO: b/438901145 - do not require root in this test -->

Note: the VM is started in SDV boot mode `unlocked`, because this test requires
`adb root` which, in `user` builds, is enabled by using the
`vendor_boot-debug.img` image. However, using `vendor_boot-debug.img` does not
allow locking the bootloader (i.e. enabling Android verified boot) which, in
`user` builds, is required for establishing the mesh.

## 4. Enable root

Enable root:

```shell
export SERIAL1=0.0.0.0:6520

adb -s $SERIAL1 wait-for-device
adb -s $SERIAL1 root
```

## 5. Verify system properties

Verify that you actually have a `user` build. Expect the following command to
return `user`:

```shell
adb -s $SERIAL1 shell getprop ro.build.type
```

Verify that authorization enforcement is disabled. Expect the following command
to return `false`:

```shell
adb -s $SERIAL1 shell getprop sdv.authz.enable
```

## 6. Start Foo

Run the following command to start tested service:

```shell
adb -s $SERIAL1 shell sdv_service_bundle create instance1:com.sdv.google.test.authz.foo.AuthzTestedService/instance

adb -s $SERIAL1 shell sdv_service_bundle start instance1:com.sdv.google.test.authz.foo.AuthzTestedService/instance
```

## 7. Start Bar

Run the following command to start test driver:

```shell
adb -s $SERIAL1 shell sdv_service_bundle create instance1:com.android.sdv.test.authz.bar.AuthzTestDriver/instance

adb -s $SERIAL1 shell sdv_service_bundle start instance1:com.android.sdv.test.authz.bar.AuthzTestDriver/instance
```

## 8. Verify logs

Run the following command to retrieve logs:

```shell
adb -s $SERIAL1 shell logcat -d *:F com_android_sdv_test_authz_bar_AuthzTestDriver_instance | grep "Non-connectable.*as it was expected."
```

Expected logs:

```shell
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Non-connectable RPC server was not discovered due to ACLs configuration as it was expected.
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Non-connectable RPC server denied connection due to ACLs configuration as it was expected.
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Non-connectable DT publisher was not discovered due to ACLs configuration as it was expected.
com_android_sdv_test_authz_bar_AuthzTestDriver_instance: authz_test_driver: Non-connectable DT publisher denied connection due to ACLs configuration as it was expected.
```
