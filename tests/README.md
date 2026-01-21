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

# Middleware End-To-End Tests

## VSIDL End-to-End Tests

Functional e2e tests of middleware through `vsidlc`-generated code using SDV Comms Stack:

- `vsidl/rust/latest`: Testing the latest work-in-progress Rust middleware APIs.
- `vsidl/rust/stable`: Testing the last major release of Rust middleware APIs.

1. Regenerate:

    ```bash
    m regenerator
    regenerator --root $ANDROID_BUILD_TOP/system/software_defined_vehicle/middleware/tests/vsidl
    ```

2. Run all end-to-end tests in this directory:

    ```bash
    cvd create
    export NOTIFY_AS_NATIVE=0.0.0.0:6520
    atest sdv_core_middleware_unit_vsidl*e2e_test
    ```
