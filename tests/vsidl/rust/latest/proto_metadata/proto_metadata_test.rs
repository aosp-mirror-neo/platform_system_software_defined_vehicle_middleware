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
    use futures::StreamExt;
    use std::str::FromStr;
    use std::sync::Arc;
    // protobuf
    use protobuf::well_known_types::wrappers::BoolValue;
    use protobuf::{
        reflect::FileDescriptor, reflect::MessageDescriptor, reflect::ReflectValueRef,
        well_known_types, CodedInputStream, MessageDyn,
    };
    // sdv
    use sdv::comms::dt::{Subscriber as DtSubscriber, SubscriberOptions as DtSubscriberOptions};
    use sdv::comms::id::ServiceFqin;
    use sdv::comms::sd::{ServiceDiscoveryManager, ServiceUnitDefinition, UnitNameDiscoveryArgs};
    use sdv::comms::ContextRef;
    use sdv::mw::{Publisher, SdvComms};
    use sdv_proto_reflection_rs::message_metadata::MessageMetadata;
    // sample
    use sdv_mw_rs_sdv_vsidl_tests_metadata_sample_bundle::{
        publisher as publisher_foo, SampleBundle,
    };
    use sdv_mw_rs_sdv_vsidl_tests_metadata_sample_bundle_without::{
        publisher as publisher_bar, SampleBundleWithout,
    };
    use vsidl_tests_proto_metadata::childdir::Child;
    use vsidl_tests_proto_metadata::reference::Neighbor;
    use vsidl_tests_proto_metadata::sample_message::sample_message::Nested;
    use vsidl_tests_proto_metadata::sample_message::SampleMessage;
    use vsidl_tests_proto_metadata::sample_message::Sibling;
    use vsidl_tests_proto_metadata_another::another::Another;

    const PACKAGE: &str = "sdv.vsidl.tests.metadata";

    #[tokio::test]
    async fn test_metadata_not_registered() {
        let bundle_name: &str = "SampleBundleWithout";
        let service_unit_name: &str = "bar";
        let unit_name = sdv::mw::UnitName::builder()
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .build()
            .unwrap();
        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("vm:{PACKAGE}.{bundle_name}/instance"))
                .expect("ServiceFqin should not be malformed"),
        );
        let sdv_comms = Arc::new(SdvComms { context });

        // GIVEN a publisher from a service bundle that does not register message metadata.
        let mut sample_sb = SampleBundleWithout::builder()
            .set_comms(sdv_comms.clone())
            .build()
            .await
            .expect("Sample service bundle creation failed.");
        let _publisher = sample_sb
            .take_publisher(publisher_bar::Channel::<SampleMessage>::BAR)
            .expect("taking sample_sb.publisher::BAR failed.");

        // WHEN discovering the service-unit.
        let service_unit = find_service_unit(&unit_name, context).await;

        // THEN application metadata shall be empty.
        assert!(service_unit.get_application_metadata().is_empty());
    }

    #[tokio::test]
    async fn test_proto_metadata() {
        let bundle_name: &str = "SampleBundle";
        let service_unit_name: &str = "foo";
        let unit_name = sdv::mw::UnitName::builder()
            .package_name(PACKAGE)
            .bundle_name(bundle_name)
            .service_unit_name(service_unit_name)
            .build()
            .unwrap();
        let context = ContextRef::create(
            ServiceFqin::from_str(&format!("vm:{PACKAGE}.{bundle_name}/instance"))
                .expect("ServiceFqin should not be malformed"),
        );
        let sdv_comms = Arc::new(SdvComms { context });

        // GIVEN a publisher from a service bundle that registers message metadata.
        let mut sample_sb = SampleBundle::builder()
            .set_comms(sdv_comms.clone())
            .build()
            .await
            .expect("Sample service bundle creation failed.");
        let publisher = sample_sb
            .take_publisher(publisher_foo::Channel::<SampleMessage>::FOO)
            .expect("taking sample_sb.publisher::FOO failed.");
        publish_message(&publisher);

        // WHEN discovering the service-unit.
        let service_unit = find_service_unit(&unit_name, context).await;

        // THEN we can obtain the message descriptor metadata.
        let message_descriptor = get_message_descriptor(&service_unit);

        // AND WHEN reading the last message.
        let byte_buffer = read_last_message(context, &service_unit).await;
        let mut input_stream = CodedInputStream::from_bytes(&byte_buffer);
        let size = input_stream.read_raw_varint32().unwrap().into();
        let _ = input_stream.push_limit(size);

        // THEN we parse the message using the message descriptor.
        let message =
            message_descriptor.parse_from(&mut input_stream).expect("parsing the message failed.");
        assert_eq!(read_nested_u32_value(&*message, "direct"), 1u32);
        assert_eq!(read_nested_u32_value(&*message, "nested"), 2u32);
        assert_eq!(read_nested_u32_value(&*message, "ref_to_same_file"), 3u32);
        assert_eq!(read_nested_u32_value(&*message, "ref_to_diff_file"), 5u32);
        assert_eq!(read_nested_u32_value(&*message, "ref_to_diff_dir"), 6u32);
        assert!(read_nested_bool_value(&*message, "ref_to_well_known"));
        assert_eq!(read_nested_u32_value(&*message, "ref_to_diff_catalog"), 7u32);
    }

    fn publish_message(publisher: &Publisher<SampleMessage>) {
        let mut message = SampleMessage::new();
        message.direct = 1;
        message.nested = Some(Nested { value: 2, ..Default::default() }).into();
        message.ref_to_same_file = Some(Sibling { value: 3, ..Default::default() }).into();
        message.ref_to_well_known = Some(BoolValue { value: true, ..Default::default() }).into();
        message.ref_to_diff_file = Some(Neighbor { value: 5, ..Default::default() }).into();
        message.ref_to_diff_dir = Some(Child { value: 6, ..Default::default() }).into();
        message.ref_to_diff_catalog = Some(Another { value: 7, ..Default::default() }).into();

        publisher.publish(&message).expect("pulishing failed.");
    }

    async fn find_service_unit(
        unit_name: &sdv::mw::UnitName,
        context: ContextRef,
    ) -> ServiceUnitDefinition {
        let sd = ServiceDiscoveryManager::new(context);

        let discovery_args = UnitNameDiscoveryArgs::new_builder()
            .set_sdv_package_name(unit_name.get_package_name())
            .set_service_bundle_name(unit_name.get_bundle_name())
            .set_service_unit_name(unit_name.get_service_unit_name())
            .build()
            .expect("Discovery args builder failed.");
        sd.list_units_by_name(&discovery_args).next().await.expect("Did not discover publisher.")
    }

    fn get_message_descriptor(service_unit: &ServiceUnitDefinition) -> MessageDescriptor {
        // file descriptors for proto well_known_types are not passed over the wire.
        let wellknown_descriptors: Vec<FileDescriptor> = vec![
            well_known_types::any::file_descriptor().clone(),
            well_known_types::api::file_descriptor().clone(),
            well_known_types::duration::file_descriptor().clone(),
            well_known_types::empty::file_descriptor().clone(),
            well_known_types::field_mask::file_descriptor().clone(),
            well_known_types::source_context::file_descriptor().clone(),
            well_known_types::struct_::file_descriptor().clone(),
            well_known_types::timestamp::file_descriptor().clone(),
            well_known_types::type_::file_descriptor().clone(),
            well_known_types::wrappers::file_descriptor().clone(),
            protobuf::descriptor::file_descriptor().clone(),
        ];

        // deserialize message metadata.
        let message_metadata: MessageMetadata =
            protobuf::Message::parse_from_bytes(&service_unit.get_application_metadata())
                .expect("parsing message descriptors failed");

        // message-specific file descriptor.
        let file_descriptors = match FileDescriptor::new_dynamic_fds(
            message_metadata.file_descriptors,
            &wellknown_descriptors,
        ) {
            Err(e) => {
                // this reveals exactly which dependency is missing.
                eprintln!("{e}");
                panic!("Creating file descriptors failed!")
            }
            Ok(fd) => fd,
        };
        let message_descriptor = file_descriptors
            .iter()
            .find_map(|fd| fd.message_by_full_name(&message_metadata.message_full_name))
            .expect("message descriptor was not found");
        message_descriptor
    }

    async fn read_last_message(
        context: ContextRef,
        service_unit: &ServiceUnitDefinition,
    ) -> Box<[u8]> {
        let dt_subscriber = DtSubscriber::create(
            context,
            &service_unit.get_unit_identifier(),
            DtSubscriberOptions { fetch_last_message: true },
        )
        .await
        .expect("Subscriber creation failed.");

        let size_for_one_msg =
            dt_subscriber.get_publisher_unit_metadata().get_message_size() as usize;
        let mut bytes = vec![0u8; size_for_one_msg];
        let _ = dt_subscriber.read_messages(&mut bytes).expect("Reading message failed.");
        bytes.into_boxed_slice()
    }

    fn read_nested_u32_value(message: &dyn MessageDyn, field_name: &str) -> u32 {
        let field_descriptor = message
            .descriptor_dyn()
            .field_by_name(field_name)
            .unwrap_or_else(|| panic!("failed reading field_by_name: {field_name}"));

        match field_descriptor
            .get_singular(message)
            .unwrap_or_else(|| panic!("failed getting singular field: {field_name}"))
        {
            ReflectValueRef::U32(number) => number,
            // All nested values are named 'value' in this test setup, mirroring proto well known types.
            ReflectValueRef::Message(nested_message) => {
                read_nested_u32_value(&*nested_message, "value")
            }
            _ => panic!("failed parsing field as nested u32: {field_name}"),
        }
    }

    fn read_nested_bool_value(message: &dyn MessageDyn, field_name: &str) -> bool {
        let field_descriptor = message
            .descriptor_dyn()
            .field_by_name(field_name)
            .unwrap_or_else(|| panic!("failed reading field_by_name: {field_name}"));

        match field_descriptor
            .get_singular(message)
            .unwrap_or_else(|| panic!("failed getting singular field: {field_name}"))
        {
            ReflectValueRef::Bool(value) => value,
            ReflectValueRef::Message(nested_message) => {
                read_nested_bool_value(&*nested_message, "value")
            }
            _ => panic!("failed parsing field as nested Bool: {field_name}"),
        }
    }
}
