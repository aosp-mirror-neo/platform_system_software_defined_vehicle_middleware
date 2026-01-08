/*
 * Copyright (C) 2026 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#[cfg(test)]
mod test {
    // rust
    use log::warn;
    // protobuf
    use protobuf::{
        reflect::MessageDescriptor, reflect::ReflectValueRef, MessageDyn, MessageField,
    };
    // sample
    use protobuf::CodedInputStream;
    use protobuf::Message;
    use sdv_proto_test_messages::nested_test_message::NestedTestMessage;
    use sdv_proto_test_messages::repeated_nested_test_message::RepeatedNestedTestMessage;
    use sdv_proto_test_messages::test_message::test_message::{InnerTestMessage, TestOneof};
    use sdv_proto_test_messages::test_message::TestMessage;
    use sdv_proto_test_messages::test_message::{Enum0123, Enum4567};

    #[tokio::test]
    async fn test_repeated_nested_test_message() {
        if let Err(err) = sdv::tracing::try_init_tracing() {
            warn!("Failed to init service bundles tracing: {err}");
        }

        // GIVEN a RepeatedNestedTestMessage
        let message_descriptor = get_message_descriptor();
        let message = create_message();

        // Serialize the message to bytes
        let message_bytes = message.write_to_bytes().expect("Failed to serialize message");

        // Convert the bytes to CodedInputStream
        let mut input_stream = CodedInputStream::from_bytes(&message_bytes);

        // WHEN / THEN parsing the message using the message descriptor
        let span = tracing::info_span!("Test::Protobuf Parse").entered();
        let message =
            message_descriptor.parse_from(&mut input_stream).expect("parsing the message failed.");
        span.exit();

        // WHEN / THEN reading the message using the reflection
        let span = tracing::info_span!("Test::Reflect").entered();
        assert_eq!(
            read_string_value(&*message, "desc"),
            "RepeatedNestedTestMessage with array size 10000".to_string()
        );
        assert_eq!(read_repeated_u32_value(&*message, "nested_test_messages"), 10000u32);
        for i in 0..10000 {
            validate_repeated_test_message(&*message, i);
        }
        span.exit();
    }

    // -------------------------------------------------------------
    // TEST SETUP
    // -------------------------------------------------------------

    #[tracing::instrument(name = "Test::create_message")]
    fn create_message() -> RepeatedNestedTestMessage {
        let test_message_proto = TestMessage {
            d: f64::MIN,
            f: f32::MAX,
            i32: i32::MIN,
            i64: i64::MAX,
            u32: u32::MAX, // u32::MAX because u32::MIN is treated as unset value
            u64: u64::MAX,
            s32: i32::MIN,
            s64: i64::MAX,
            f32: u32::MAX, // u32::MAX because u32::MIN is treated as unset value
            f64: u64::MAX,
            sf32: i32::MIN,
            sf64: i64::MAX,
            b: true,
            str: "Some String Field".to_string(),
            bb: vec![0xff, 0x00, 0x01, 0xff],
            e0: Enum0123::three.into(),
            e4: Enum4567::seven.into(),
            inner: MessageField::some(InnerTestMessage {
                inner_d: f64::MAX,
                inner_f: f32::MIN,
                inner_i32: i32::MAX,
                ..Default::default()
            }),
            TestOneof: Some(TestOneof::OneofD(f64::MAX)),
            ..Default::default()
        };
        let nested_test_message_proto = NestedTestMessage {
            desc: "A description of a nested test message".to_string(),
            test_message: MessageField::some(test_message_proto),
            ..Default::default()
        };

        RepeatedNestedTestMessage {
            desc: format!("RepeatedNestedTestMessage with array size {}", 10000),
            nested_test_messages: vec![nested_test_message_proto; 10000],
            ..Default::default()
        }
    }

    fn get_message_descriptor() -> MessageDescriptor {
        // message-specific file descriptor.
        let file_descriptors = [
            sdv_proto_test_messages::repeated_nested_test_message::file_descriptor(),
            sdv_proto_test_messages::nested_test_message::file_descriptor(),
            sdv_proto_test_messages::test_message::file_descriptor(),
            vsidl_v1_stdlib::annotations::file_descriptor(),
        ];
        let message_descriptor = file_descriptors
            .iter()
            .find_map(|fd| {
                fd.message_by_full_name(".com.sdv.google.test_messages.RepeatedNestedTestMessage")
            })
            .expect("message descriptor was not found");
        message_descriptor
    }

    // -------------------------------------------------------------
    // VALIDATION
    // -------------------------------------------------------------

    fn read_string_value(message: &dyn MessageDyn, field_name: &str) -> String {
        let field_descriptor = message
            .descriptor_dyn()
            .field_by_name(field_name)
            .unwrap_or_else(|| panic!("failed reading field_by_name: '{field_name}'"));
        match field_descriptor
            .get_singular(message)
            .unwrap_or_else(|| panic!("failed getting singular field: '{field_name}'"))
        {
            ReflectValueRef::String(string) => string.to_string(),
            _ => panic!("failed parsing field '{field_name}' as nested String"),
        }
    }

    fn read_repeated_u32_value(message: &dyn MessageDyn, field_name: &str) -> u32 {
        let field_descriptor = message
            .descriptor_dyn()
            .field_by_name(field_name)
            .unwrap_or_else(|| panic!("failed reading field_by_name: '{field_name}'"));
        field_descriptor.get_repeated(message).len() as u32
    }

    macro_rules! validate {
        ($test_message:ident, $field_name:expr, $reflect_value_ref:path, $expected_value:expr) => {
            let field =
                $test_message.descriptor_dyn().field_by_name($field_name).unwrap_or_else(|| {
                    panic!("failed reading field_by_name: '{}'", stringify!($field_name))
                });
            if let Some($reflect_value_ref(value)) = field.get_singular(&*$test_message) {
                assert_eq!(value, $expected_value);
            } else {
                panic!(
                    "failed parsing field {} as nested {}, found {:?}",
                    stringify!($field_name),
                    stringify!($reflect_value_ref),
                    field.get_singular(&*$test_message).unwrap().get_type()
                );
            }
        };
    }

    fn validate_repeated_test_message(message: &dyn MessageDyn, index: usize) {
        let field_descriptor = message
            .descriptor_dyn()
            .field_by_name("nested_test_messages")
            .unwrap_or_else(|| panic!("failed reading field_by_name: nested_test_messages"));

        if let ReflectValueRef::Message(nested_message) =
            field_descriptor.get_repeated(message).get(index)
        {
            let test_message_field = nested_message
                .descriptor_dyn()
                .field_by_name("test_message")
                .unwrap_or_else(|| panic!("failed reading field_by_name: test_message"));
            if let Some(ReflectValueRef::Message(test_message)) =
                test_message_field.get_singular(&*nested_message)
            {
                validate!(test_message, "d", ReflectValueRef::F64, f64::MIN);
                validate!(test_message, "f", ReflectValueRef::F32, f32::MAX);
                validate!(test_message, "i32", ReflectValueRef::I32, i32::MIN);
                validate!(test_message, "i64", ReflectValueRef::I64, i64::MAX);
                validate!(test_message, "u32", ReflectValueRef::U32, u32::MAX);
                validate!(test_message, "u64", ReflectValueRef::U64, u64::MAX);
                validate!(test_message, "f32", ReflectValueRef::U32, u32::MAX);
                validate!(test_message, "f64", ReflectValueRef::U64, u64::MAX);
                validate!(test_message, "sf32", ReflectValueRef::I32, i32::MIN);
                validate!(test_message, "sf64", ReflectValueRef::I64, i64::MAX);
                validate!(test_message, "b", ReflectValueRef::Bool, true);
                validate!(test_message, "str", ReflectValueRef::String, "Some String Field");
                validate!(test_message, "bb", ReflectValueRef::Bytes, vec![0xff, 0x00, 0x01, 0xff]);

                let e0_field = test_message
                    .descriptor_dyn()
                    .field_by_name("e0")
                    .unwrap_or_else(|| panic!("failed reading field_by_name: e0"));
                if let Some(ReflectValueRef::Enum(_, index)) = e0_field.get_singular(&*test_message)
                {
                    assert_eq!(index, 3);
                }

                let e4_field = test_message
                    .descriptor_dyn()
                    .field_by_name("e4")
                    .unwrap_or_else(|| panic!("failed reading field_by_name: e4"));
                if let Some(ReflectValueRef::Enum(_, index)) = e4_field.get_singular(&*test_message)
                {
                    assert_eq!(index, 3);
                }

                let inner_field = test_message
                    .descriptor_dyn()
                    .field_by_name("inner")
                    .unwrap_or_else(|| panic!("failed reading field_by_name: inner"));
                if let Some(ReflectValueRef::Message(inner_message)) =
                    inner_field.get_singular(&*test_message)
                {
                    validate!(inner_message, "inner_d", ReflectValueRef::F64, f64::MAX);
                    validate!(inner_message, "inner_f", ReflectValueRef::F32, f32::MIN);
                    validate!(inner_message, "inner_i32", ReflectValueRef::I32, i32::MAX);
                }

                validate!(test_message, "oneof_d", ReflectValueRef::F64, f64::MAX);
            }
        } else {
            panic!("failed parsing nested test message at index '{index}'");
        }
    }
}
