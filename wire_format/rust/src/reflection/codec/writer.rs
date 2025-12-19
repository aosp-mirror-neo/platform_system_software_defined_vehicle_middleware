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
// limitations under the License

use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use anyhow::bail;
use protobuf::CodedOutputStream;
use sdv_vsidl_common::proto_size_utils::compute_length_size;

const MAX_ONE_BYTE_VARINT: u8 = 127;

// Represents a varint length delimiter. The parent field represents the index of the node
// that has this node as a nested field
struct Node {
    pub(crate) data: NodeData,
    pub(crate) parent: Option<usize>,
}

// Information needed to construct the well-formatted protobuf serialized message
struct NodeData {
    // Index of the length delimiter in the buffer
    pub(crate) length_delimiter_index: usize,
    // Amount of bytes that were actually serialized for this length delimited field.
    pub(crate) serialized_bytes_first_pass: usize,
    // Amount of bytes that were added by nested length delimited fields - this occurs when their
    // `serialized_bytes_first_pass` cannot be stored in 1 byte. This means that we must allocate
    // more bytes for the varint length delimiter, and the added bytes must be accounted for by
    // their parent nodes
    pub(crate) delimiter_bytes_to_add: usize,
}

/// Interface that represents Protobuf format writers
pub trait IProtobufWriter<'output_lt>:
    Deref<Target = CodedOutputStream<'output_lt>> + DerefMut<Target = CodedOutputStream<'output_lt>>
{
    /// Starts a length delimited value. This method handles the length delimiter that precedes
    /// the serialized value. After the value is serialized, `end_length_delimited_message` must be called
    /// to denote the end of the length delimited value.
    fn start_length_delimited_message(&mut self) -> anyhow::Result<()>;

    /// Ends a length delimited value. This method must be used to denote the end of a length delimited
    /// value that was handled with `start_length_delimited_message`. Calling this method without a
    /// corresponding `start_length_delimited_message` will return an error.
    fn end_length_delimited_message(&mut self) -> anyhow::Result<()>;

    /// Returns the total number of bytes written by the writer. This method intentionally shadows the method
    /// `total_bytes_written` defined in CodedOutputStream, which can return erroneous results if the
    /// output stream is recreated
    fn total_bytes_written(&self) -> u64;
}

// This trait is meant to be used only in conjunction with `ProtobufWriter`. The
// `ProtobufWriter` struct writes its output to a buffer, to which it maintains a mutable
// reference to a buffer through the `CodedOutputStream` struct. Implementors of this trait
// are meant to hold another reference to this buffer, and provide access to it through unsafe
// structures and operations.
// This is needed by the ProtobufWriter, which will drop its reference and recreate the output
// stream. All the methods of this trait require that the ProtobufWriter's CodedOutputStream is
// dropped before calling them.
trait IBufferHolder<'lt> {
    // SAFETY: Caller must ensure that all other references to the buffer are dropped before
    // calling this method.
    // The returned `CodedOutputStream` contains a mutable reference to the buffer.
    unsafe fn recreate_output_stream(
        &mut self,
        stream_index: usize,
        written_bytes: usize,
    ) -> CodedOutputStream<'lt>;

    // SAFETY: Caller must ensure that all other references to the buffer are dropped before
    // calling this method.
    unsafe fn get_writable_buffer(&mut self) -> &'lt mut [u8];

    // SAFETY: Caller must ensure that all other references to the buffer are dropped before
    // calling this method.
    unsafe fn resize_buffer(&mut self, _new_size: usize) {}
}

struct PublicationBufferHolder<'lt> {
    reserved_bytes_prefix: usize,
    // Raw pointer to the start of the buffer used in `coded_output_stream`. Note that `coded_output_stream`
    // has a reference to a suffix of the buffer, not the full buffer
    buffer_ptr: *mut u8,
    // Length of the buffer used in `coded_output_stream`. This is the full length of the buffer, not the length
    // of the suffix referenced by `coded_output_stream`
    buffer_len: usize,
    _pd: PhantomData<&'lt [u8]>,
}

impl<'lt> PublicationBufferHolder<'lt> {
    fn new(reserved_bytes_prefix: usize, buffer_ptr: *mut u8, buffer_len: usize) -> Self {
        Self { reserved_bytes_prefix, buffer_ptr, buffer_len, _pd: PhantomData }
    }

    // SAFETY: Caller must ensure that all other references to the buffer are dropped before
    // calling this method.
    unsafe fn get_buffer(&mut self) -> &'lt mut [u8] {
        // SAFETY: Function's safety contract ensures no other references to the buffer exist.
        // Function takes mut self so it can't be accessed from multiple threads without synchronization.
        unsafe { std::slice::from_raw_parts_mut(self.buffer_ptr, self.buffer_len) }
    }
}

impl<'lt> IBufferHolder<'lt> for PublicationBufferHolder<'lt> {
    unsafe fn recreate_output_stream(
        &mut self,
        stream_index: usize,
        written_bytes: usize,
    ) -> CodedOutputStream<'lt> {
        // subtract 1 to account for the single byte we wrote for reserving the space
        let serialized_bytes_first_pass = written_bytes - stream_index - 1;
        // SAFETY: Function's safety contract ensures no other reference to the buffer exists.
        // Function takes &mut self so it can't be accessed from multiple threads without synchronization.
        let buffer = unsafe { std::slice::from_raw_parts_mut(self.buffer_ptr, self.buffer_len) };
        // Write the length value
        buffer[stream_index + self.reserved_bytes_prefix] = serialized_bytes_first_pass as u8;

        CodedOutputStream::bytes(&mut buffer[self.reserved_bytes_prefix + written_bytes..])
    }

    unsafe fn get_writable_buffer(&mut self) -> &'lt mut [u8] {
        // SAFETY: Function's safety contract ensures `get_buffer`'s safety condition holds.
        // Function takes mut self so it can't be accessed from multiple threads without synchronization
        let buffer = unsafe { self.get_buffer() };
        &mut buffer[self.reserved_bytes_prefix..]
    }
}

struct RpcBufferHolder<'lt> {
    vec_ptr: *mut Vec<u8>,
    _pd: PhantomData<&'lt Vec<u8>>,
}

impl<'lt> RpcBufferHolder<'lt> {
    fn new(vec_ptr: *mut Vec<u8>) -> Self {
        Self { vec_ptr, _pd: PhantomData }
    }
}

impl<'lt> IBufferHolder<'lt> for RpcBufferHolder<'lt> {
    unsafe fn recreate_output_stream(
        &mut self,
        stream_index: usize,
        written_bytes: usize,
    ) -> CodedOutputStream<'lt> {
        // subtract 1 to account for the single byte we wrote for reserving the space
        let serialized_bytes_first_pass = written_bytes - stream_index - 1;

        // SAFETY: Function's safety contract ensures no other reference to the buffer exists.
        // Function takes &mut self so it can't be accessed from multiple threads without synchronization.
        let buffer = unsafe { &mut *self.vec_ptr };
        buffer[stream_index] = serialized_bytes_first_pass as u8;
        CodedOutputStream::vec(buffer)
    }

    unsafe fn get_writable_buffer(&mut self) -> &'lt mut [u8] {
        // SAFETY: Function's safety contract ensures no other references to the buffer exist.
        // Function takes &mut self so it can't be accessed from multiple threads without synchronization.
        let buffer = unsafe { &mut *self.vec_ptr };
        &mut buffer[..]
    }

    unsafe fn resize_buffer(&mut self, new_size: usize) {
        // SAFETY: Function's safety contract ensures no other references to the buffer exist.
        // Function takes &mut self so it can't be accessed from multiple threads without synchronization.
        let buffer = unsafe { &mut *self.vec_ptr };
        buffer.resize(new_size, 0);
    }
}

struct ProtobufWriter<'lt, B: IBufferHolder<'lt>> {
    coded_output_stream: CodedOutputStream<'lt>,
    // Total number of bytes written by previous instances of coded_output_stream.
    // Does not include count of current stream.
    bytes_written_by_previous_streams: usize,
    // This represents a tree of length delimiter nodes. A node corresponds
    // to a length delimited field (a message or an array) in protobuf, with children representing
    // nested structs/arrays with length delimiter fields.
    // A node's children will be stored in order from left to right, in the order in which they
    // appear in the serialized protobuf message.
    nodes: Vec<Node>,
    curr_node_index: usize,
    buffer_holder: B,
}

impl<'lt, B: IBufferHolder<'lt>> ProtobufWriter<'lt, B> {
    fn new(coded_output_stream: CodedOutputStream<'lt>, buffer_holder: B) -> Self {
        Self {
            coded_output_stream,
            bytes_written_by_previous_streams: 0,
            nodes: vec![Node {
                data: NodeData {
                    length_delimiter_index: 0,
                    serialized_bytes_first_pass: 0,
                    delimiter_bytes_to_add: 0,
                },
                parent: None,
            }],
            curr_node_index: 0,
            buffer_holder,
        }
    }

    fn start_length_delimited_message(&mut self) -> anyhow::Result<()> {
        // Get the current index, where the length delimiter field should appear in the serialized output
        let length_delimiter_index = self.total_bytes_written() as usize;
        self.nodes.push(Node {
            data: NodeData {
                length_delimiter_index,
                serialized_bytes_first_pass: 0,
                delimiter_bytes_to_add: 0,
            },
            // set the node's parent to the current node in scope
            parent: Some(self.curr_node_index),
        });
        // set the newly created node to be the one in scope
        self.curr_node_index = self.nodes.len() - 1;

        // Guess that the message size will fit in a single byte, store 0 at the current index
        Ok(self.coded_output_stream.write_raw_byte(0)?)
    }

    fn end_length_delimited_message(&mut self) -> anyhow::Result<()> {
        let current_index = self.total_bytes_written() as usize;

        // get the current node in scope
        let curr_node = &mut self.nodes[self.curr_node_index];

        // subtract 1 to account for the single byte we wrote for reserving the space
        let serialized_bytes_first_pass = current_index - curr_node.data.length_delimiter_index - 1;
        // a parent will always exist, since the initial node we added never goes out of scope
        let Some(parent_index) = curr_node.parent else { bail!("Internal error") };

        if serialized_bytes_first_pass <= MAX_ONE_BYTE_VARINT as usize {
            // Assigning self.coded_output_stream to a new instance to drop the existing mutable
            // reference to the buffer
            self.coded_output_stream = CodedOutputStream::bytes(&mut []);
            // SAFETY: Previous assignment dropped the existing reference to the buffer.
            self.coded_output_stream = unsafe {
                self.buffer_holder
                    .recreate_output_stream(curr_node.data.length_delimiter_index, current_index)
            };
            self.bytes_written_by_previous_streams = current_index;

            // And discard this node. Make sure that we are on the top of the stack (i.e. all our children must have been popped as well)
            if self.curr_node_index != self.nodes.len() - 1 {
                bail!("Internal error")
            }
            let _ = self.nodes.pop();
        } else {
            // set the amount of bytes serialized in this first pass
            curr_node.data.serialized_bytes_first_pass = serialized_bytes_first_pass;
        }
        // reset the current scope to the parent node
        self.curr_node_index = parent_index;
        Ok(())
    }

    fn total_bytes_written(&self) -> u64 {
        let underlying_stream_bytes = self.coded_output_stream.total_bytes_written();
        underlying_stream_bytes + self.bytes_written_by_previous_streams as u64
    }

    // Returns the buffer holder and the total number of bytes written by the writer, dropping
    // all other references to the buffer.
    fn finalize(mut self) -> anyhow::Result<(B, usize)> {
        self.coded_output_stream.flush()?;
        self.bytes_written_by_previous_streams = self.total_bytes_written() as usize;
        // Assigning self.coded_output_stream to a new instance to drop the existing mutable
        // reference to the buffer
        self.coded_output_stream = CodedOutputStream::bytes(&mut []);
        {
            // SAFETY: Previous assignment dropped the sole mut ref to the buffer. Function takes mut self
            // so it can't be accessed from multiple threads without synchronization
            let buffer = unsafe { self.buffer_holder.get_writable_buffer() };
            self.compute_needed_buffer_adjustments(buffer)?;
            // `buffer` goes out of scope
        }
        let final_byte_count =
            self.bytes_written_by_previous_streams + self.nodes[0].data.delimiter_bytes_to_add;

        // SAFETY: No other references to the buffer exist
        unsafe {
            self.buffer_holder.resize_buffer(final_byte_count);
        }
        if self.nodes[0].data.delimiter_bytes_to_add != 0 {
            // We need to shift data in the buffer

            // SAFETY: No other references to the buffer exist
            let buffer = unsafe { self.buffer_holder.get_writable_buffer() };
            self.shift_buffer(self.bytes_written_by_previous_streams, buffer)?;
        }
        Ok((self.buffer_holder, final_byte_count))
    }

    fn compute_needed_buffer_adjustments(&mut self, buffer: &mut [u8]) -> anyhow::Result<()> {
        // Traverse the nodes from right to left - this corresponds to the reverse of the order
        // in which the nodes appear in the serialized buffer, and also to a post order traversal
        // of the constructed tree
        let mut i = self.nodes.len() - 1;
        while i > 0 {
            let current_node = &self.nodes[i];

            let delimiter_bytes_to_add = current_node.data.delimiter_bytes_to_add;

            // compute the total bytes to serialize from the bytes serialized in the first pass and the
            // bytes that were added by children nodes for their updated length delimiter
            let total_bytes_to_serialize =
                current_node.data.serialized_bytes_first_pass + delimiter_bytes_to_add;

            let Some(parent_index) = current_node.parent else { bail!("Internal error") };

            let length_delimiter_byte_size = compute_length_size(total_bytes_to_serialize);
            if length_delimiter_byte_size > 1 {
                // The total number of bytes we need to serialize doesn't fit within one byte,
                // we need to shift the buffer by (length_delimiter_byte_size - 1 bytes)
                let shift = length_delimiter_byte_size - 1;
                // update the parent's delimiter_bytes_to_add by this shift
                self.nodes[parent_index].data.delimiter_bytes_to_add += shift;
            } else {
                // we can directly serialize the length in the reserved byte
                let delimiter_index = current_node.data.length_delimiter_index;
                buffer[delimiter_index] = total_bytes_to_serialize as u8;
            }
            // In any case, we must update the parent's delimiter_bytes_to_add with the info collected by
            // our child nodes.
            self.nodes[parent_index].data.delimiter_bytes_to_add += delimiter_bytes_to_add;
            i -= 1;
        }
        // self.nodes[0].data.delimiter_bytes_to_add now stores the total number of bytes added for length delimiters
        Ok(())
    }

    fn shift_buffer(
        &self,
        bytes_written_in_first_pass: usize,
        buffer: &mut [u8],
    ) -> anyhow::Result<()> {
        let mut node_index = self.nodes.len() - 1;
        // The first node we encounter (starting from the right) needs to shift by all the bytes
        // we need to add. The nodes we encounter after it will shift less and less until all
        // needed shifts are done.
        let mut shift_amount = self.nodes[0].data.delimiter_bytes_to_add;
        // The first node we encounter needs to shift all bytes from its starting index till the
        // last index that was written. Nodes encountered after it will shift from their starting
        // index till the previous node's original start index
        let mut shift_end_index = bytes_written_in_first_pass;
        // Iterate over the nodes from end to start, ignore the first placeholder node
        while node_index > 0 {
            let node = &self.nodes[node_index];
            // This is the varint that should be written for the current length delimiter field
            let varint_to_write =
                node.data.serialized_bytes_first_pass + node.data.delimiter_bytes_to_add;
            let needed_bytes = compute_length_size(varint_to_write);

            // If needed_bytes are more than 1, we need to shift the data we have currently,
            // serialize the needed bytes and update the shift indices. If needed_bytes == 1,
            // then this size was already serialized in the earlier traversal and the data is still
            // in proper protobuf format
            if needed_bytes > 1 {
                // shift the buffer from the current index to `shift_end_index` by `shift_amount`
                buffer.copy_within(
                    node.data.length_delimiter_index..shift_end_index,
                    node.data.length_delimiter_index + shift_amount,
                );

                let index_shift = needed_bytes - 1;
                let new_length_delimiter_index = (node.data.length_delimiter_index + shift_amount)
                    .checked_sub(index_shift)
                    .ok_or_else(|| anyhow::Error::msg("Attempted to subtract with underflow"))?;

                // serialize the needed varint
                encode_varint32(varint_to_write as u32, &mut buffer[new_length_delimiter_index..]);
                // The next node needs to shift bytes up to the current node's original index
                shift_end_index = node.data.length_delimiter_index;
                // The next node needs to shift bytes by the current shift_amount minus the amount
                // of bytes shifted by the current node
                shift_amount = shift_amount
                    .checked_sub(index_shift)
                    .ok_or_else(|| anyhow::Error::msg("Attempted to subtract with underflow"))?;
            }
            node_index -= 1;
        }
        Ok(())
    }
}

fn encode_varint32(mut value: u32, buffer: &mut [u8]) {
    let mut i = 0;
    while value & !0x7F > 0 {
        buffer[i] = ((value & 0x7F) | 0x80) as u8;
        value >>= 7;
        i += 1;
    }
    buffer[i] = value as u8;
}

/// Protobuf writer used to write to SDV publications. Most of the actual serialization methods are provided by
/// the `CodedOutputStream` struct, which is accessible via the `Deref` and `DereMut` implementations.
pub struct PublicationWriter<'lt> {
    writer: ProtobufWriter<'lt, PublicationBufferHolder<'lt>>,
}

impl<'lt> Deref for PublicationWriter<'lt> {
    type Target = CodedOutputStream<'lt>;

    fn deref(&self) -> &Self::Target {
        &self.writer.coded_output_stream
    }
}

impl DerefMut for PublicationWriter<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.writer.coded_output_stream
    }
}

impl<'lt> PublicationWriter<'lt> {
    fn new(
        coded_output_stream: CodedOutputStream<'lt>,
        reserved_bytes_prefix: usize,
        buffer_ptr: *mut u8,
        buffer_len: usize,
    ) -> Self {
        Self {
            writer: ProtobufWriter::new(
                coded_output_stream,
                PublicationBufferHolder::new(reserved_bytes_prefix, buffer_ptr, buffer_len),
            ),
        }
    }

    /// Create a new publication writer that writes into the provided buffer. The publication's
    /// maximum header size is the maximum amount of bytes needed to store the serialized message
    /// length (value between 0 and 5).
    /// The provided buffer must satisfy: buffer.len() >= sdv publication size + 'header_max_size'.
    /// This is because the publication writer may serialize data starting at an index between 0 and
    /// 'header_max_size'.
    pub fn create_publication_writer(
        header_max_size: usize,
        buffer: &'lt mut [u8],
    ) -> anyhow::Result<Self> {
        // validate the given sizes
        if buffer.len() <= header_max_size {
            bail!(
                "Invalid header max size ({}) for buffer of length {}",
                header_max_size,
                buffer.len()
            )
        }
        let publication_original_size = buffer.len() - header_max_size;
        if compute_length_size(publication_original_size) > header_max_size {
            bail!("Given header max ({header_max_size}) is insufficient to hold length value for publication of size {}", buffer.len())
        }
        let buffer_ptr = buffer.as_mut_ptr();
        let buffer_len = buffer.len();
        let coded_output_stream = CodedOutputStream::bytes(&mut buffer[header_max_size..]);
        Ok(Self::new(coded_output_stream, header_max_size, buffer_ptr, buffer_len))
    }

    /// This method must be called in order to finalize the serialization process. After calling this method,
    /// the publication will be serialized in the passed in buffer.
    /// This method returns the index in the buffer where the serialized data starts.
    pub fn finalize(self) -> anyhow::Result<usize> {
        let (mut buffer_holder, final_byte_count) = self.writer.finalize()?;
        let reserved_bytes_prefix = buffer_holder.reserved_bytes_prefix;
        // SAFETY: `self.writer.finalize()` drops the writer and ensures that `buffer_holder``
        // holds the only references to the buffer
        let buffer = unsafe { buffer_holder.get_buffer() };

        let needed_len_field_size = compute_length_size(final_byte_count);
        let start_index = reserved_bytes_prefix - needed_len_field_size;
        encode_varint32(final_byte_count as u32, &mut buffer[start_index..]);
        Ok(start_index)
    }
}

impl<'lt> IProtobufWriter<'lt> for PublicationWriter<'lt> {
    fn start_length_delimited_message(&mut self) -> anyhow::Result<()> {
        self.writer.start_length_delimited_message()
    }

    fn end_length_delimited_message(&mut self) -> anyhow::Result<()> {
        self.writer.end_length_delimited_message()
    }

    fn total_bytes_written(&self) -> u64 {
        self.writer.total_bytes_written()
    }
}

/// Protobuf writer used to write to SDV RPC method arguments/return values. Most of the actual serialization
/// methods are provided by the `CodedOutputStream` struct, which is accessible via the `Deref` and `DereMut` implementations.
pub struct RpcWriter<'lt> {
    writer: ProtobufWriter<'lt, RpcBufferHolder<'lt>>,
}

impl<'lt> Deref for RpcWriter<'lt> {
    type Target = CodedOutputStream<'lt>;

    fn deref(&self) -> &Self::Target {
        &self.writer.coded_output_stream
    }
}

impl DerefMut for RpcWriter<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.writer.coded_output_stream
    }
}

impl<'lt> RpcWriter<'lt> {
    fn new(coded_output_stream: CodedOutputStream<'lt>, vec_ptr: *mut Vec<u8>) -> Self {
        Self { writer: ProtobufWriter::new(coded_output_stream, RpcBufferHolder::new(vec_ptr)) }
    }

    /// Create a new rpc writer that writes into the provided buffer. The writer may modify
    /// the buffer's size and capacity during serialization
    pub fn create_rpc_writer(buffer: &'lt mut Vec<u8>) -> Self {
        let vec_ptr = buffer as *mut Vec<u8>;

        let coded_output_stream = CodedOutputStream::vec(buffer);

        Self::new(coded_output_stream, vec_ptr)
    }

    /// This method must be called in order to finalize the serialization process. After calling this method,
    /// the passed in buffer will contain the serialized data
    pub fn finalize(self) -> anyhow::Result<()> {
        let _ = self.writer.finalize()?;
        Ok(())
    }
}

impl<'lt> IProtobufWriter<'lt> for RpcWriter<'lt> {
    fn start_length_delimited_message(&mut self) -> anyhow::Result<()> {
        self.writer.coded_output_stream.flush()?;
        self.writer.start_length_delimited_message()
    }

    fn end_length_delimited_message(&mut self) -> anyhow::Result<()> {
        self.writer.coded_output_stream.flush()?;
        self.writer.end_length_delimited_message()
    }

    fn total_bytes_written(&self) -> u64 {
        self.writer.total_bytes_written()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let mut buffer = vec![0; 10];
        let writer = PublicationWriter::create_publication_writer(1, &mut buffer).unwrap();

        assert_eq!(writer.writer.nodes.len(), 1);
        assert_eq!(writer.writer.curr_node_index, 0);
        assert_eq!(writer.writer.nodes[0].parent, None);
        assert_eq!(writer.writer.nodes[0].data.length_delimiter_index, 0);
        assert_eq!(writer.writer.nodes[0].data.serialized_bytes_first_pass, 0);
        assert_eq!(writer.writer.nodes[0].data.delimiter_bytes_to_add, 0);
    }

    #[test]
    fn test_single_length_delimited_message() {
        let mut buffer = vec![0; 10];
        let mut writer = PublicationWriter::create_publication_writer(1, &mut buffer).unwrap();

        // Start message
        writer.start_length_delimited_message().unwrap();
        // We added a node for the length delimited field
        assert_eq!(writer.writer.nodes.len(), 2);
        // The current node points to the latest node we just added
        assert_eq!(writer.writer.curr_node_index, 1);
        // The parent is the placeholder node at index 0
        assert_eq!(writer.writer.nodes[1].parent, Some(0));
        // Length delimiter index was correctly stored
        assert_eq!(writer.writer.nodes[1].data.length_delimiter_index, 0);

        // Write some data
        writer.write_uint32_no_tag(150).unwrap(); // 150 is 0x96, 0x01 in varint

        // End message
        writer.end_length_delimited_message().unwrap();
        // The current node correctly resets to the parent node after we end a length delimited field
        assert_eq!(writer.writer.curr_node_index, 0);
        // We removed the node after processing the message since the length was small enough to fit in 1 byte
        assert_eq!(writer.writer.nodes.len(), 1);

        // Finalize
        let written_bytes = writer.total_bytes_written() as usize;
        assert_eq!(written_bytes, 3);

        writer.finalize().unwrap();

        buffer.resize(written_bytes + 1, 0);
        assert_eq!(buffer, vec![0x3, 0x2, 0x96, 0x01]);
    }

    #[test]
    fn test_nested_length_delimited_messages() {
        let mut buffer = vec![0; 100];
        let mut writer = PublicationWriter::create_publication_writer(1, &mut buffer).unwrap();

        // Outer message
        writer.start_length_delimited_message().unwrap(); // L1
        let l1_start_pos = writer.total_bytes_written() as usize;
        assert_eq!(l1_start_pos, 1);
        // We added a node for the length delimited message
        assert_eq!(writer.writer.nodes.len(), 2);
        // The current node points to the latest node we just added
        assert_eq!(writer.writer.curr_node_index, 1);
        // The parent is the placeholder node at index 0
        assert_eq!(writer.writer.nodes[1].parent, Some(0));
        // Length delimiter index was correctly stored
        assert_eq!(writer.writer.nodes[1].data.length_delimiter_index, 0);

        writer.write_bool_no_tag(true).unwrap(); // 1 byte

        // Inner message
        writer.start_length_delimited_message().unwrap(); // L2
        let l2_start_pos = writer.total_bytes_written() as usize;
        assert_eq!(l2_start_pos, 3);
        // We added a node for the length delimited message
        assert_eq!(writer.writer.nodes.len(), 3);
        // The current node points to the latest node we just added
        assert_eq!(writer.writer.curr_node_index, 2);
        // The parent is the outer message
        assert_eq!(writer.writer.nodes[2].parent, Some(1));
        // Length delimiter index was correctly stored
        assert_eq!(writer.writer.nodes[2].data.length_delimiter_index, 2);

        writer.write_uint32_no_tag(42).unwrap(); // 1 byte

        // End inner message
        writer.end_length_delimited_message().unwrap(); // L2
                                                        // The current node correctly resets to the parent node after we end a length delimited field
        assert_eq!(writer.writer.curr_node_index, 1);
        let l2_end_pos = writer.total_bytes_written() as usize;
        assert_eq!(l2_end_pos, 4);
        // We removed the L2 node after processing the message since the length was small enough to fit in 1 byte
        assert_eq!(writer.writer.nodes.len(), 2);

        writer.write_bool_no_tag(false).unwrap(); // 1 byte

        // End outer message
        writer.end_length_delimited_message().unwrap(); // L1
                                                        // The current node correctly resets to the parent node after we end a length delimited field
        assert_eq!(writer.writer.curr_node_index, 0);
        let l1_end_pos = writer.total_bytes_written() as usize;
        assert_eq!(l1_end_pos, 5);
        // We removed the L1 node after processing the message since the length was small enough to fit in 1 byte
        assert_eq!(writer.writer.nodes.len(), 1);
    }

    #[test]
    fn test_sibling_length_delimited_messages() {
        let mut buffer = vec![0; 100];
        let mut writer = PublicationWriter::create_publication_writer(1, &mut buffer).unwrap();

        // First sibling
        writer.start_length_delimited_message().unwrap();
        // We added a node for the length delimited message
        assert_eq!(writer.writer.curr_node_index, 1);
        assert_eq!(writer.writer.nodes.len(), 2);
        // The parent is the placeholder node at index 0
        assert_eq!(writer.writer.nodes[1].parent, Some(0));

        writer.write_uint32_no_tag(1).unwrap();
        writer.end_length_delimited_message().unwrap();
        // The current node correctly resets to the parent node after we end a length delimited field
        assert_eq!(writer.writer.curr_node_index, 0);
        // We removed the node after processing the message since the length was small enough to fit in 1 byte
        assert_eq!(writer.writer.nodes.len(), 1);

        // Second sibling
        writer.start_length_delimited_message().unwrap();
        // We added a node for the length delimited message
        assert_eq!(writer.writer.curr_node_index, 1);
        assert_eq!(writer.writer.nodes.len(), 2);
        // The parent is the placeholder node at index 0
        assert_eq!(writer.writer.nodes[1].parent, Some(0));
        writer.write_uint32_no_tag(2).unwrap();
        writer.end_length_delimited_message().unwrap();
        // The current node correctly resets to the parent node after we end a length delimited field
        assert_eq!(writer.writer.curr_node_index, 0);
        // We removed the node after processing the message since the length was small enough to fit in 1 byte
        assert_eq!(writer.writer.nodes.len(), 1);

        let written_bytes = writer.total_bytes_written() as usize;
        assert_eq!(written_bytes, 4);

        writer.finalize().unwrap();

        // add 1 for the message length
        buffer.resize(written_bytes + 1, 0);
        assert_eq!(buffer, vec![4, 1, 1, 1, 2]);
    }

    #[test]
    fn test_end_message_without_start_fails() {
        let mut buffer = vec![0; 10];
        let mut writer = PublicationWriter::create_publication_writer(1, &mut buffer).unwrap();
        writer.write_raw_byte(0).expect("Writing should succeed");
        let result = writer.end_length_delimited_message();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Internal error");
    }

    #[test]
    fn test_single_length_delimited_message_rpc() {
        let mut buffer = Vec::new();
        let mut writer = RpcWriter::create_rpc_writer(&mut buffer);

        // Start message
        writer.start_length_delimited_message().unwrap();
        // We added a node for the length delimited field
        assert_eq!(writer.writer.nodes.len(), 2);
        // The current node points to the latest node we just added
        assert_eq!(writer.writer.curr_node_index, 1);
        // The parent is the placeholder node at index 0
        assert_eq!(writer.writer.nodes[1].parent, Some(0));
        // Length delimiter index was correctly stored
        assert_eq!(writer.writer.nodes[1].data.length_delimiter_index, 0);

        // Write some data
        writer.write_uint32_no_tag(150).unwrap(); // 150 is 0x96, 0x01 in varint

        // End message
        writer.end_length_delimited_message().unwrap();
        // The current node correctly resets to the parent node after we end a length delimited field
        assert_eq!(writer.writer.curr_node_index, 0);
        // We removed the node after processing the message since the length was small enough to fit in 1 byte
        assert_eq!(writer.writer.nodes.len(), 1);

        // Finalize
        let written_bytes = writer.total_bytes_written() as usize;
        assert_eq!(written_bytes, 3);

        writer.finalize().unwrap();

        assert_eq!(buffer, vec![0x2, 0x96, 0x01]);
    }

    #[test]
    fn test_nested_length_delimited_messages_rpc() {
        let mut buffer = Vec::new();
        let mut writer = RpcWriter::create_rpc_writer(&mut buffer);

        // Outer message
        writer.start_length_delimited_message().unwrap(); // L1
        let l1_start_pos = writer.total_bytes_written() as usize;
        assert_eq!(l1_start_pos, 1);
        assert_eq!(writer.writer.nodes.len(), 2);
        assert_eq!(writer.writer.curr_node_index, 1);
        assert_eq!(writer.writer.nodes[1].parent, Some(0));
        assert_eq!(writer.writer.nodes[1].data.length_delimiter_index, 0);

        writer.write_bool_no_tag(true).unwrap(); // 1 byte

        // Inner message
        writer.start_length_delimited_message().unwrap(); // L2
        let l2_start_pos = writer.total_bytes_written() as usize;
        assert_eq!(l2_start_pos, 3);
        assert_eq!(writer.writer.nodes.len(), 3);
        assert_eq!(writer.writer.curr_node_index, 2);
        assert_eq!(writer.writer.nodes[2].parent, Some(1));
        assert_eq!(writer.writer.nodes[2].data.length_delimiter_index, 2);

        writer.write_uint32_no_tag(42).unwrap(); // 1 byte

        // End inner message
        writer.end_length_delimited_message().unwrap(); // L2
        assert_eq!(writer.writer.curr_node_index, 1);
        let l2_end_pos = writer.total_bytes_written() as usize;
        assert_eq!(l2_end_pos, 4);
        assert_eq!(writer.writer.nodes.len(), 2);

        writer.write_bool_no_tag(false).unwrap(); // 1 byte

        // End outer message
        writer.end_length_delimited_message().unwrap(); // L1
        assert_eq!(writer.writer.curr_node_index, 0);
        let l1_end_pos = writer.total_bytes_written() as usize;
        assert_eq!(l1_end_pos, 5);
        assert_eq!(writer.writer.nodes.len(), 1);

        writer.finalize().unwrap();
        assert_eq!(buffer, vec![4, 1, 1, 42, 0]);
    }

    #[test]
    fn test_sibling_length_delimited_messages_rpc() {
        let mut buffer = Vec::new();
        let mut writer = RpcWriter::create_rpc_writer(&mut buffer);

        writer.start_length_delimited_message().unwrap();
        writer.write_uint32_no_tag(1).unwrap();
        writer.end_length_delimited_message().unwrap();

        writer.start_length_delimited_message().unwrap();
        writer.write_uint32_no_tag(2).unwrap();
        writer.end_length_delimited_message().unwrap();

        writer.finalize().unwrap();

        assert_eq!(buffer, vec![1, 1, 1, 2]);
    }

    #[test]
    fn test_end_message_without_start_fails_rpc() {
        let mut buffer = Vec::new();
        let mut writer = RpcWriter::create_rpc_writer(&mut buffer);
        writer.write_raw_byte(0).expect("Writing should succeed");
        let result = writer.end_length_delimited_message();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Internal error");
    }

    #[test]
    fn test_encode_varint32() {
        let mut buffer = vec![0; 5];

        encode_varint32(1, &mut buffer);
        assert_eq!(&buffer[0..1], &[1]);

        encode_varint32(127, &mut buffer);
        assert_eq!(&buffer[0..1], &[127]);

        encode_varint32(128, &mut buffer);
        assert_eq!(&buffer[0..2], &[0x80, 0x01]);

        encode_varint32(300, &mut buffer);
        assert_eq!(&buffer[0..2], &[0xAC, 0x02]);
    }
}
