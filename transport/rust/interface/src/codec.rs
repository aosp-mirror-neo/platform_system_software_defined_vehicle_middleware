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

//! Codec Utils

use protobuf::Message;
use sdv_status_rs::{SdvResult, SdvStatus, SdvStatusCode};

/// Serializes a proto message, which is passed in from user code.
/// Returns SdvResult that contains a the encoded message with prepended message length.
#[tracing::instrument(name = "MW::Codec<M>::encode", skip(message))]
pub fn encode<M: Message>(message: &M) -> SdvResult<Box<[u8]>> {
    let span = tracing::info_span!("Protobuf Encode").entered();
    let protobuf_encoded = message.write_length_delimited_to_bytes();
    span.exit();
    match protobuf_encoded {
        Err(_) => Err(SdvStatus::new(SdvStatusCode::InvalidArgument)),
        Ok(bytes) => {
            let span = tracing::info_span!("Slice").entered();
            let result = bytes.into_boxed_slice();
            span.exit();
            Ok(result)
        }
    }
}

/// Serializes a proto message and appends it to a vector, prepended with the message length.
///
/// Appends the encoded message to the end of the vector.
/// Will resize the vector if the capacity isn't sufficient for the encoded message.
///
/// # Arguments
/// * `message` - The protobuf message to encode.
/// * `buffer` - The vector to which the encoded message will be appended.
///
/// # Errors
/// Returns `SdvStatusCode::Internal` if proto encoding fails.
#[tracing::instrument(name = "MW::Codec<M>::encode_to_vec", skip(message, buffer))]
pub fn encode_to_vec<M: Message>(message: &M, buffer: &mut Vec<u8>) -> SdvResult<()> {
    message
        .write_length_delimited_to_vec(buffer)
        .map_err(|_| SdvStatus::with_message(SdvStatusCode::Internal, "Proto encoding failed."))
}

/// Decodes a proto message from a byte buffer with encoded message length, which is read from the pub/sub comms implementation.
#[tracing::instrument(name = "MW::Codec<M>::decode", skip(bytes))]
pub fn decode<M: Message>(bytes: &[u8]) -> SdvResult<M> {
    let span = tracing::info_span!("Prepare Input Stream").entered();
    // Determine the actual message size encoded at the start of the byte array
    let mut input_stream = protobuf::CodedInputStream::from_bytes(bytes);
    let size = input_stream.read_raw_varint32().map_err(|_| {
        SdvStatus::with_message(SdvStatusCode::Internal, "Failed to get the size of buffer")
    })?;
    // And parse only the part of the byte array that contains the message
    input_stream.push_limit(size.into()).map_err(|_| {
        SdvStatus::with_message(SdvStatusCode::Internal, "Failed to read the message from buffer")
    })?;

    span.exit();
    let _span = tracing::info_span!("Protobuf Decode").entered();
    match M::parse_from(&mut input_stream) {
        Err(_ /* TODO(b/351162991): Log error. */) => {
            Err(SdvStatus::with_message(SdvStatusCode::Internal, "Failed to parse the message"))
        }
        Ok(message) => Ok(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protobuf::well_known_types::wrappers::UInt32Value;
    use sdv_status_rs::SdvStatusCode;

    // A simple protobuf message for testing.
    fn create_test_message(value: u32) -> UInt32Value {
        let mut msg = UInt32Value::new();
        msg.value = value;
        msg
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        // GIVEN a valid message
        let original_message = create_test_message(12345);

        // WHEN encode and decode
        let encoded_bytes = encode(&original_message).expect("Encoding failed");
        assert!(!encoded_bytes.is_empty());
        let decoded_message: UInt32Value = decode(&encoded_bytes).expect("Decoding failed");

        // THEN the message would be identical to the original message
        assert_eq!(original_message, decoded_message);
        assert_eq!(original_message.value, decoded_message.value);
    }

    #[test]
    fn test_decode_empty_bytes() {
        // GIVEN empty bytes
        let empty_bytes: &[u8] = &[];

        // WHEN decode
        let result: SdvResult<UInt32Value> = decode(empty_bytes);

        // THEN result is error
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().status_code(), SdvStatusCode::Internal);
    }

    #[test]
    fn test_decode_insufficient_bytes_for_size() {
        // GIVEN a single byte that isn't a valid varint end.
        let bytes: &[u8] = &[0x80];

        // WHEN decode
        let result: SdvResult<UInt32Value> = decode(bytes);

        // THEN result is error
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.status_code(), SdvStatusCode::Internal);
        assert_eq!(
            error.to_string(),
            "SdvStatusCode::Internal(13): Failed to get the size of buffer"
        );
    }

    #[test]
    fn test_decode_size_larger_than_buffer() {
        // GIVEN encodes size 10, but only provides 1 byte of data (0x01)
        let bytes: &[u8] = &[0x0A, 0x01];

        // WHEN decode
        let result: SdvResult<UInt32Value> = decode(bytes);

        // THEN result is error
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert_eq!(error.status_code(), SdvStatusCode::Internal);
        assert_eq!(error.to_string(), "SdvStatusCode::Internal(13): Failed to parse the message");
    }

    #[test]
    fn test_decode_invalid_protobuf_data() {
        // GIVEN valid size bytes, but data is not a valid UInt32Value
        let bytes: &[u8] = &[0x02, 0xFF, 0xFF];

        // WHEN decode
        let result: SdvResult<UInt32Value> = decode(bytes);

        // THEN result is error
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().status_code(), SdvStatusCode::Internal);
    }

    #[test]
    fn test_decode_extra_bytes_after_message() {
        // GIVEN valid message
        let original_message = create_test_message(9876);

        // WHEN encode, add extra garbage bytes, and decode
        let mut encoded_data = encode(&original_message).unwrap().into_vec();

        encoded_data.push(0xDE);
        encoded_data.push(0xAD);
        encoded_data.push(0xBE);
        encoded_data.push(0xEF);

        let decoded_message: UInt32Value = decode(&encoded_data).unwrap();

        // THEN message is identical to original message
        assert_eq!(original_message, decoded_message);
    }

    #[test]
    fn test_encode_produces_length_delimited() {
        // GIVEN valid message
        let msg_value: u32 = 42;
        let original_message = create_test_message(msg_value);

        // GIVEN expected bytes and length
        let expected_protobuf_bytes = vec![0x08, 0x2A];
        let expected_length_delimiter = expected_protobuf_bytes.len() as u8;

        // WHEN encode
        let encoded_bytes = encode(&original_message).expect("Encoding failed");

        // THEN encode bytes are identical to expected bytes
        assert_eq!(encoded_bytes[0], expected_length_delimiter);
        assert_eq!(&encoded_bytes[1..], expected_protobuf_bytes.as_slice());
    }

    #[test]
    fn test_encode_to_vec_behavior() {
        // GIVEN a non-empty vector with some initial data
        let mut buffer = vec![0xAA, 0xBB];
        let initial_len = buffer.len();

        // AND a message to encode
        let message = create_test_message(100);
        let encoded_msg = encode(&message).expect("encode failed");

        // WHEN encode_to_vec is called
        encode_to_vec(&message, &mut buffer).expect("encode_to_vec failed");

        // THEN the message is appended to the vector
        assert_eq!(buffer.len(), initial_len + encoded_msg.len());
        assert_eq!(&buffer[..initial_len], &[0xAA, 0xBB]);
        assert_eq!(&buffer[initial_len..], encoded_msg.as_ref());

        // AND WHEN we encode another message
        let message2 = create_test_message(200);
        let encoded_msg2 = encode(&message2).expect("encode failed");
        encode_to_vec(&message2, &mut buffer).expect("encode_to_vec failed");

        // THEN it is also appended
        assert_eq!(buffer.len(), initial_len + encoded_msg.len() + encoded_msg2.len());
    }
}
