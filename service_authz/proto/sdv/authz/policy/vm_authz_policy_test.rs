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
use sdv_authz_policy_proto_rust::vm_authz_policy::VmAuthzPolicy;
use std::fs;

const TEST_TEXTPROTO_PATH: &str = "sdv/authz/policy/testdata/vm_authz_policy_permissions.textproto";

#[test]
fn test_parse_textproto_with_permissions() {
    let textproto_content = fs::read_to_string(TEST_TEXTPROTO_PATH).unwrap();
    let policy: VmAuthzPolicy = parse_from_str(&textproto_content).unwrap();

    assert_eq!(policy.publisher.len(), 3);
    assert_eq!(policy.publisher[0].message, Some("test.package.MessageType".to_string()));
    assert_eq!(policy.publisher[0].topic[0], "test_topic");
    assert_eq!(policy.publisher[1].message, Some("test.package.MessageType2".to_string()));
    assert_eq!(policy.publisher[1].allow_all_topics, Some(true));
    assert_eq!(policy.publisher[2].package, Some("test.blanket_package".to_string()));
    assert_eq!(policy.publisher[2].allow_all_topics, None);

    assert_eq!(policy.subscriber.len(), 3);
    assert_eq!(policy.subscriber[0].message, Some("test.package.MessageType".to_string()));
    assert_eq!(policy.subscriber[0].topic[0], "test_another_topic");
    assert_eq!(policy.subscriber[1].message, Some("test.package.MessageType2".to_string()));
    assert_eq!(policy.subscriber[1].allow_all_topics, Some(true));
    assert_eq!(policy.subscriber[2].package, Some("test.blanket_package".to_string()));
    assert_eq!(policy.subscriber[2].allow_all_topics, None);

    assert_eq!(policy.server.len(), 3);
    assert_eq!(policy.server[0].service, Some("test.package.Server".to_string()));
    assert_eq!(policy.server[0].channel[0], "test-channel");
    assert_eq!(policy.server[1].service, Some("test.package.Server2".to_string()));
    assert_eq!(policy.server[1].allow_all_channels, Some(true));
    assert_eq!(policy.server[2].package, Some("test.blanket_package".to_string()));
    assert_eq!(policy.server[2].allow_all_channels, None);

    assert_eq!(policy.client.len(), 3);
    assert_eq!(policy.client[0].service, Some("test.package.Server".to_string()));
    assert_eq!(policy.client[0].channel[0], "test-another-channel");
    assert_eq!(policy.client[1].service, Some("test.package.Server2".to_string()));
    assert_eq!(policy.client[1].allow_all_channels, Some(true));
    assert_eq!(policy.client[2].package, Some("test.blanket_package".to_string()));
    assert_eq!(policy.client[2].allow_all_channels, None);
}
