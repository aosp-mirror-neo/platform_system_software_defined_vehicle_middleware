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

//! Functions to create/parse GetPeerLocalAclsResponse proto message.

pub(crate) mod from_peer_local_acls_proto;
pub(crate) mod to_peer_local_acls_proto;
pub(crate) mod unit_tests;

pub(crate) use from_peer_local_acls_proto::from_peer_local_acls_proto;
pub(crate) use to_peer_local_acls_proto::to_peer_local_acls_proto;
