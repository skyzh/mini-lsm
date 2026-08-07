// Copyright (c) 2022-2026 Alex Chi Z
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

use bytes::BufMut;

use crate::key::{KeySlice, KeyVec};

use super::Block;

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

    /// Adds a key-value pair to the block. Returns false when the block is full.
    /// You may find the `bytes::BufMut` trait useful for manipulating binary data.
    #[must_use]
    pub fn add(&mut self, key: KeySlice, value: &[u8]) -> bool {
        if key.is_empty() || self.offsets.len() == u16::MAX as usize {
            return false;
        }

        let overlap = if self.is_empty() {
            0
        } else {
            self.first_key
                .key_ref()
                .iter()
                .zip(key.key_ref())
                .take_while(|(a, b)| a == b)
                .count()
        };
        let rest = &key.key_ref()[overlap..];
        if overlap > u16::MAX as usize
            || rest.len() > u16::MAX as usize
            || value.len() > u16::MAX as usize
            || self.data.len() > u16::MAX as usize
        {
            return false;
        }

        let entry_size = 2 + 2 + rest.len() + 8 + 2 + value.len();
        let projected_size = self.data.len()
            + entry_size
            + (self.offsets.len() + 1) * size_of::<u16>()
            + size_of::<u16>();
        if !self.is_empty() && projected_size > self.block_size {
            return false;
        }

        self.offsets.push(self.data.len() as u16);
        self.data.put_u16(overlap as u16);
        self.data.put_u16(rest.len() as u16);
        self.data.extend_from_slice(rest);
        self.data.put_u64(key.ts());
        self.data.put_u16(value.len() as u16);
        self.data.extend_from_slice(value);
        if self.first_key.is_empty() {
            self.first_key.set_from_slice(key);
        }
        true
    }

    /// Check if there is no key-value pair in the block.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Finalize the block.
    pub fn build(self) -> Block {
        assert!(!self.is_empty(), "cannot build an empty block");
        Block {
            data: self.data,
            offsets: self.offsets,
        }
    }
}
