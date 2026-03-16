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

//! TODO(b/493831901): Replace this module by the tested vsidlc message_size implementation.
//! Utility methods for topic attributes.

use protobuf::descriptor::FieldOptions;
use protobuf::reflect::{FieldDescriptor, MessageDescriptor, RuntimeFieldType, RuntimeType};
use protobuf::{MessageFull, UnknownValueRef};
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};
use std::collections::HashMap;

const REPEATED_FIELD_MAX_COUNT_FIELD_NUMBER: u32 = 50050;
const VARIABLE_TYPE_MAX_SIZE_FIELD_NUMBER: u32 = 50051;

fn get_option_value(options: &FieldOptions, field_number: u32) -> Option<u32> {
    options.special_fields.unknown_fields().get(field_number).and_then(
        |value_ref| match value_ref {
            UnknownValueRef::Varint(v) => Some(v as u32),
            _ => None,
        },
    )
}

fn get_repeated_field_max_count(field: &FieldDescriptor) -> Option<u32> {
    field
        .proto()
        .options
        .as_ref()
        .and_then(|opts| get_option_value(opts, REPEATED_FIELD_MAX_COUNT_FIELD_NUMBER))
}

fn get_variable_type_max_size(field: &FieldDescriptor) -> Option<u32> {
    field
        .proto()
        .options
        .as_ref()
        .and_then(|opts| get_option_value(opts, VARIABLE_TYPE_MAX_SIZE_FIELD_NUMBER))
}

// Conservative maximum size for a 32-bit varint
fn max_varint32_size() -> u32 {
    6
}

// Conservative maximum size for a 64-bit varint
fn max_varint64_size() -> u32 {
    11
}

// Size of the length delimiter for a field of a given size
fn length_delimiter_size(len: u32) -> u32 {
    if len == 0 {
        1
    } else {
        ((31 - len.leading_zeros()) / 7) + 1
    }
}

fn compute_tag_size(field_number: u32) -> u32 {
    // The tag is encoded as a varint: (field_number << 3) | wire_type
    // We can use the field number to estimate the size of the varint.
    length_delimiter_size(field_number << 3)
}

fn get_field_size(field: &FieldDescriptor, cache: &mut HashMap<String, u32>) -> SdvResult<u32> {
    let tag_size = compute_tag_size(field.number() as u32);

    let value_size = match field.runtime_field_type() {
        RuntimeFieldType::Singular(ty) => match ty {
            RuntimeType::F32 => 4,
            RuntimeType::F64 => 8,
            RuntimeType::I32 => max_varint32_size(),
            RuntimeType::I64 => max_varint64_size(),
            RuntimeType::U32 => max_varint32_size(),
            RuntimeType::U64 => max_varint64_size(),
            RuntimeType::Bool => 1,
            RuntimeType::String => {
                let max_size = get_variable_type_max_size(field).ok_or_else(|| {
                    SdvStatus::with_message(
                        SdvStatusCode::InvalidArgument,
                        format!(
                            "Missing (sdv.vsidl.v1.variable_type_max_size) option for field {}",
                            field.name()
                        ),
                    )
                })?;
                max_size + length_delimiter_size(max_size)
            }
            RuntimeType::VecU8 => {
                let max_size = get_variable_type_max_size(field).ok_or_else(|| {
                    SdvStatus::with_message(
                        SdvStatusCode::InvalidArgument,
                        format!(
                            "Missing (sdv.vsidl.v1.variable_type_max_size) option for field {}",
                            field.name()
                        ),
                    )
                })?;
                max_size + length_delimiter_size(max_size)
            }
            RuntimeType::Enum(_) => max_varint32_size(),
            RuntimeType::Message(desc) => {
                if let Some(max_size) = get_variable_type_max_size(field) {
                    max_size + length_delimiter_size(max_size)
                } else {
                    let nested_size = calculate_message_size_recursive(&desc, cache)?;
                    nested_size + length_delimiter_size(nested_size)
                }
            }
        },
        RuntimeFieldType::Repeated(_) => unreachable!("Handled in outer scope"), // Should not happen here
        RuntimeFieldType::Map(_, _) => {
            return Err(SdvStatus::with_message(
                SdvStatusCode::Unimplemented,
                "Map fields are not supported",
            ))
        }
    };

    let single_element_size = tag_size + value_size;

    if field.is_repeated() {
        let max_count = get_repeated_field_max_count(field).ok_or_else(|| {
            SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                format!(
                    "Missing (sdv.vsidl.v1.repeated_field_max_count) option for repeated field {}",
                    field.name()
                ),
            )
        })?;
        Ok(single_element_size * max_count)
    } else {
        match field.proto().type_.unwrap_or_default().enum_value_or_default() {
            protobuf::descriptor::field_descriptor_proto::Type::TYPE_FIXED32
            | protobuf::descriptor::field_descriptor_proto::Type::TYPE_SFIXED32
            | protobuf::descriptor::field_descriptor_proto::Type::TYPE_FLOAT => Ok(tag_size + 4),
            protobuf::descriptor::field_descriptor_proto::Type::TYPE_FIXED64
            | protobuf::descriptor::field_descriptor_proto::Type::TYPE_SFIXED64
            | protobuf::descriptor::field_descriptor_proto::Type::TYPE_DOUBLE => Ok(tag_size + 8),
            protobuf::descriptor::field_descriptor_proto::Type::TYPE_SINT32 => {
                Ok(tag_size + max_varint32_size())
            }
            protobuf::descriptor::field_descriptor_proto::Type::TYPE_SINT64 => {
                Ok(tag_size + max_varint64_size())
            }
            _ => Ok(single_element_size),
        }
    }
}

fn calculate_message_size_recursive(
    descriptor: &MessageDescriptor,
    cache: &mut HashMap<String, u32>,
) -> SdvResult<u32> {
    let full_name = descriptor.full_name();
    if let Some(&size) = cache.get(full_name) {
        if size == u32::MAX {
            return Err(SdvStatus::with_message(
                SdvStatusCode::InvalidArgument,
                format!("Cyclic dependency detected for message {}", full_name),
            ));
        }
        return Ok(size);
    }

    // Insert a temporary value to detect cycles
    cache.insert(full_name.to_string(), u32::MAX);

    let mut total_size: u32 = 0;
    for field in descriptor.fields() {
        total_size += get_field_size(&field, cache)?;
    }

    cache.insert(full_name.to_string(), total_size);
    Ok(total_size)
}

/// Calculates the maximum runtime size of a protobuf message.
pub fn get_message_size<M: MessageFull>() -> SdvResult<u32> {
    let descriptor = M::descriptor();
    let mut cache = HashMap::new();
    calculate_message_size_recursive(&descriptor, &mut cache)
}
