// Copyright (c) 2022-2025 Alex Chi Z
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use bytes::BufMut;

use crate::key::{KeySlice, KeyVec};

use super::{Block, SIZE_OF_U16};

/// Builds a block.
pub struct BlockBuilder {
    /// Offsets of each key-value entries.
    offsets: Vec<u16>,
    /// All serialized key-value pairs in the block.
    data: Vec<u8>,
    /// The expected block size.
    block_size: usize,
    /// The first key in the block
    first_key: KeyVec,
}

impl BlockBuilder {
    /// Creates a new block builder.
    pub fn new(block_size: usize) -> Self {
        Self {
            offsets: Vec::new(),
            data: Vec::new(),
            block_size,
            first_key: KeyVec::new(),
        }
    }

    fn current_size(&self) -> usize {
        SIZE_OF_U16 /*num_of_elements*/ + self.data.len() /* key value pairs*/ + self.offsets.len() *SIZE_OF_U16
        /*offsets*/
    }

    /// overlap_len returns the number of bytes that overlap with `first_key` in the block.
    /// ref: https://users.rust-lang.org/t/how-to-find-common-prefix-of-two-byte-slices-effectively/25815/4
    fn overlap_len(&self, key: &[u8]) -> usize {
        let chunk_size = 128;
        let offset = std::iter::zip(
            self.first_key.raw_ref().chunks_exact(chunk_size),
            key.chunks_exact(chunk_size),
        )
        .take_while(|(a, b)| a == b)
        .count()
            * chunk_size;

        offset
            + std::iter::zip(&self.first_key.raw_ref()[offset..], &key[offset..])
                .take_while(|(a, b)| a == b)
                .count()

        // let mut ret = 0;
        // while ret < self.first_key.len()
        //     && ret < key.len()
        //     && self.first_key.raw_ref()[ret] == key[ret]
        // {
        //     ret += 1;
        // }

        // ret
    }

    /// Adds a key-value pair to the block. Returns false when the block is full.
    #[must_use]
    pub fn add(&mut self, key: KeySlice, value: &[u8]) -> bool {
        // if first data, skip check
        if !self.is_empty()
            && self.current_size() + key.len() + value.len() + 3 * SIZE_OF_U16 >= self.block_size
        {
            return false;
        }

        self.offsets.push(self.data.len() as u16);

        let overlap = self.overlap_len(key.raw_ref());
        self.data.put_u16(overlap as u16);
        self.data.put_u16((key.len() - overlap) as u16);
        self.data.put(&key.raw_ref()[overlap..]);

        self.data.put_u16(value.len() as u16);
        self.data.put(value);

        if self.first_key.is_empty() {
            self.first_key = key.to_key_vec();
        }

        true
    }

    /// Check if there is no key-value pair in the block.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Finalize the block.
    pub fn build(self) -> Block {
        Block {
            data: self.data,
            offsets: self.offsets,
        }
    }
}
