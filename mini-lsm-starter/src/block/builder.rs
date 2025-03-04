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

#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use crate::key::{KeySlice, KeyVec};
use bytes::BufMut;

use super::Block;
use std::mem;

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

    fn size_calc(&self) -> usize {
        let offset_size = self.offsets.len() * mem::size_of::<u16>();
        let data_size = self.data.len() * mem::size_of::<u8>();
        offset_size + data_size
    }

    pub fn compute_overlap(first_key: KeySlice, key: KeySlice) -> usize {
        let raw_first_key = first_key.key_ref().to_vec(); // Create a copy as Vec<u8>
        let raw_key = key.key_ref().to_vec(); // Create a copy as Vec<u8>

        raw_first_key
            .iter()
            .zip(raw_key.iter())
            .take_while(|(a, b)| a == b)
            .count()
    }

    /// Adds a key-value pair to the block. Returns false when the block is full.
    #[must_use]
    pub fn add(&mut self, key: KeySlice, value: &[u8]) -> bool {
        assert!(!key.is_empty(), "Key should not be empty");
        if !self.data.is_empty()
            && self.size_calc() + key.raw_len() + value.len() + mem::size_of::<u16>()
                >= self.block_size
        {
            return false;
        }

        let overlap = BlockBuilder::compute_overlap(self.first_key.as_key_slice(), key);
        self.offsets.push(self.data.len() as u16);

        // Encode key-encoded content.
        self.data.put_u16(overlap as u16);
        self.data.put_u16((key.key_len() - overlap) as u16);
        self.data.extend(&key.key_ref()[overlap..]);
        self.data.put_u64(key.ts());
        // Encode value length.
        self.data.put_u16(value.len() as u16);
        // Encode value content.
        self.data.put(value);

        if self.first_key.is_empty() {
            self.first_key.set_from_slice(key);
        }

        true
    }

    /// Check if there is no key-value pair in the block.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Finalize the block.
    pub fn build(self) -> Block {
        if self.is_empty() {
            panic!("Empty Block cannot be built");
        }
        Block {
            data: self.data,
            offsets: self.offsets,
        }
    }
}
