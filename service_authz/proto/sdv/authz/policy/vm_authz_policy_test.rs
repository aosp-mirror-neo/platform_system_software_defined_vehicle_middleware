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

    assert_eq!(policy.allow_publisher.len(), 3);
    assert_eq!(policy.allow_publisher[0].message, "test.package.MessageType");
    assert_eq!(policy.allow_publisher[0].topic[0], "test_topic");
    assert_eq!(policy.allow_publisher[1].message, "test.package.MessageType2");
    assert_eq!(policy.allow_publisher[1].topic[0], "*");
    assert_eq!(policy.allow_publisher[2].message, "test.package.Allowed");
    assert_eq!(policy.allow_publisher[2].topic[0], "*");

    assert_eq!(policy.allow_subscriber.len(), 3);
    assert_eq!(policy.allow_subscriber[0].message, "test.package.MessageType");
    assert_eq!(policy.allow_subscriber[0].topic[0], "test_another_topic");
    assert_eq!(policy.allow_subscriber[1].message, "test.package.MessageType2");
    assert_eq!(policy.allow_subscriber[1].topic[0], "*");
    assert_eq!(policy.allow_subscriber[2].message, "test.package.Allowed");
    assert_eq!(policy.allow_subscriber[2].topic[0], "*");

    assert_eq!(policy.allow_server.len(), 3);
    assert_eq!(policy.allow_server[0].service, "test.package.Server");
    assert_eq!(policy.allow_server[0].channel[0], "test-channel");
    assert_eq!(policy.allow_server[1].service, "test.package.Server2");
    assert_eq!(policy.allow_server[1].channel[0], "*");
    assert_eq!(policy.allow_server[2].service, "test.package.Allowed");
    assert_eq!(policy.allow_server[2].channel[0], "*");

    assert_eq!(policy.allow_client.len(), 3);
    assert_eq!(policy.allow_client[0].service, "test.package.Server");
    assert_eq!(policy.allow_client[0].channel[0], "test-another-channel");
    assert_eq!(policy.allow_client[1].service, "test.package.Server2");
    assert_eq!(policy.allow_client[1].channel[0], "*");
    assert_eq!(policy.allow_client[2].service, "test.package.Allowed");
    assert_eq!(policy.allow_client[2].channel[0], "*");

    assert_eq!(policy.deny_publisher.len(), 3);
    assert_eq!(policy.deny_publisher[0].message, "test.package.DeniedMessageType");
    assert_eq!(policy.deny_publisher[0].topic[0], "denied_topic");
    assert_eq!(policy.deny_publisher[1].message, "test.package.DeniedMessageType2");
    assert_eq!(policy.deny_publisher[1].topic[0], "*");
    assert_eq!(policy.deny_publisher[2].message, "test.package.Denied");
    assert_eq!(policy.deny_publisher[2].topic[0], "*");

    assert_eq!(policy.deny_subscriber.len(), 3);
    assert_eq!(policy.deny_subscriber[0].message, "test.package.DeniedMessageType");
    assert_eq!(policy.deny_subscriber[0].topic[0], "denied_topic");
    assert_eq!(policy.deny_subscriber[1].message, "test.package.DeniedMessageType2");
    assert_eq!(policy.deny_subscriber[1].topic[0], "*");
    assert_eq!(policy.deny_subscriber[2].message, "test.package.Denied");
    assert_eq!(policy.deny_subscriber[2].topic[0], "*");

    assert_eq!(policy.deny_server.len(), 3);
    assert_eq!(policy.deny_server[0].service, "test.package.DeniedServer");
    assert_eq!(policy.deny_server[0].channel[0], "denied-channel");
    assert_eq!(policy.deny_server[1].service, "test.package.DeniedServer2");
    assert_eq!(policy.deny_server[1].channel[0], "*");
    assert_eq!(policy.deny_server[2].service, "test.package.Denied");
    assert_eq!(policy.deny_server[2].channel[0], "*");

    assert_eq!(policy.deny_client.len(), 3);
    assert_eq!(policy.deny_client[0].service, "test.package.DeniedServer");
    assert_eq!(policy.deny_client[0].channel[0], "denied-channel");
    assert_eq!(policy.deny_client[1].service, "test.package.DeniedServer2");
    assert_eq!(policy.deny_client[1].channel[0], "*");
    assert_eq!(policy.deny_client[2].service, "test.package.Denied");
    assert_eq!(policy.deny_client[2].channel[0], "*");
}
