<!--Copyright (C) 2026 The Android Open Source Project

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License. -->

# RPC Crypto Library

This directory contains library that enables TLS for RPC.

## Purpose

Service Discovery and SDV RPC library depend on each other in terms of X509 certificate signing and usage. E.g. the certificate issued by SD should be acceptable in RPC library that RPC is using. To verify this compatibility we have to implement a lot of unit tests, that would emulate interaction between RPC and SD. It is easier to do if for the tests we use real code that is located at one place in a library.

## Limitations

This library can't depend on the types that are defined in Comm Stack because RPC part of Comms Stack will depend on this library. Due to this limitations some functions are not as type safe as they could be.
