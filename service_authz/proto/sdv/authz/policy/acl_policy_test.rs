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

    assert_eq!(policy.publisher.len(), 1);
    assert_eq!(policy.publisher[0].message, "test.package.MessageType");
    assert_eq!(policy.publisher[0].topic[0], "test_topic");

    assert_eq!(policy.subscriber.len(), 1);
    assert_eq!(policy.subscriber[0].message, "test.package.MessageType");
    assert_eq!(policy.subscriber[0].topic[0], "test_another_topic");

    assert_eq!(policy.server.len(), 1);
    assert_eq!(policy.server[0].service, "test.package.Server");
    assert_eq!(policy.server[0].channel[0], "test-channel");

    assert_eq!(policy.client.len(), 1);
    assert_eq!(policy.client[0].service, "test.package.Server");
    assert_eq!(policy.client[0].channel[0], "test-another-channel");

    assert!(policy.allow_read_all);
}
