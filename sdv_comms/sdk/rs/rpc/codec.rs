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

//! Encoder and decoder for Tonic

use bytes::BufMut;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};

#[derive(Default)]
pub(super) struct NoOpCodec {}

impl Codec for NoOpCodec {
    type Decode = bytes::BytesMut;
    type Encode = bytes::BytesMut;
    type Decoder = NoOpDecoder;
    type Encoder = NoOpEncoder;
    fn decoder(&mut self) -> Self::Decoder {
        NoOpDecoder {}
    }
    fn encoder(&mut self) -> Self::Encoder {
        NoOpEncoder {}
    }
}

pub(super) struct NoOpDecoder {}
impl Decoder for NoOpDecoder {
    type Item = bytes::BytesMut;
    type Error = tonic::Status;
    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        let mut item = Self::Item::default();
        item.put(src);
        Ok(Some(item))
    }
}

pub(super) struct NoOpEncoder {}
impl Encoder for NoOpEncoder {
    type Item = bytes::BytesMut;
    type Error = tonic::Status;
    fn encode(&mut self, item: Self::Item, dst: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        dst.put(item);
        Ok(())
    }
}
