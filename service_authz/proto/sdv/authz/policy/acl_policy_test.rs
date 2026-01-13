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

use protobuf::text_format::parse_from_str;
use sdv_authz_policy_proto_rust::acl_policy::AclPolicy;
use std::fs;

const TEST_TEXTPROTO_PATH: &str = "sdv/authz/policy/testdata/authz_policy_permissions.textproto";

#[test]
fn test_parse_textproto_with_permissions() {
    let textproto_content = fs::read_to_string(TEST_TEXTPROTO_PATH).unwrap();
    let policy: AclPolicy = parse_from_str(&textproto_content).unwrap();

    assert_eq!(policy.publish.len(), 1);
    assert_eq!(policy.publish[0].publication_type, "test.MessageType");
    assert_eq!(policy.publish[0].channel_name, "test-channel-name");

    assert_eq!(policy.subscribe.len(), 1);
    assert_eq!(policy.subscribe[0].publication_type, "test.MessageType");
    assert_eq!(policy.subscribe[0].channel_name, "test-another-channel-name");

    assert_eq!(policy.serve.len(), 1);
    assert_eq!(policy.serve[0].server_type, "test.Server");
    assert_eq!(policy.serve[0].service_unit_name, "test-service-unit-name");

    assert_eq!(policy.call.len(), 1);
    assert_eq!(policy.call[0].server_type, "test.Server");
    assert_eq!(policy.call[0].service_unit_name, "test-another-service-unit-name");
}
