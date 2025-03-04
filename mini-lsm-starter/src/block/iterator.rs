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

use super::{Block, SIZE_OF_U16};
use crate::key::{KeySlice, KeyVec};
use bytes::Buf;
use std::{cmp::Ordering, sync::Arc};
const U64_SIZE: usize = 8;

/// Iterates on a block.
pub struct BlockIterator {
    /// The internal `Block`, wrapped by an `Arc`
    block: Arc<Block>,
    /// The current key, empty represents the iterator is invalid
    key: KeyVec,
    /// the current value range in the block.data, corresponds to the current key
    value_range: (usize, usize),
    /// Current index of the key-value pair, should be in range of [0, num_of_elements)
    idx: usize,
    /// The first key in the block
    first_key: KeyVec,
}

impl BlockIterator {
    fn new(block: Arc<Block>) -> Self {
        Self {
            block,
            key: KeyVec::new(),
            value_range: (0, 0),
            idx: 0,
            first_key: KeyVec::new(),
        }
    }

    // First key as a keyVec
    fn get_first_key(block: Arc<Block>) -> KeyVec {
        let rest = (&block.data[SIZE_OF_U16..2 * SIZE_OF_U16]).get_u16() as usize;
        let mut key = KeyVec::new();
        key.append(&block.data[2 * SIZE_OF_U16..2 * SIZE_OF_U16 + rest].to_vec()[..]);
        key.set_ts(
            (&block.data[2 * SIZE_OF_U16 + rest..2 * SIZE_OF_U16 + rest + U64_SIZE]).get_u64(),
        );
        key
    }

    /// Creates a block iterator and seek to the first entry.
    pub fn create_and_seek_to_first(block: Arc<Block>) -> Self {
        // let key_size = (&block.data[0..SIZE_OF_U16]).get_u16() as usize;

        let mut temp_self = Self {
            block: block.clone(),
            first_key: BlockIterator::get_first_key(block.clone()),
            key: KeyVec::new(),
            value_range: (0, 0),
            idx: 0,
        };

        temp_self.seek_to_idx(0);
        temp_self
    }

    /// Creates a block iterator and seek to the first key that >= `key`.
    pub fn create_and_seek_to_key(block: Arc<Block>, key: KeySlice) -> Self {
        let key_size = (&block.data[0..SIZE_OF_U16]).get_u16() as usize;

        let mut temp_self = Self {
            block: block.clone(),
            first_key: BlockIterator::get_first_key(block.clone()),
            key: KeyVec::new(),
            value_range: (0, 0),
            idx: 0,
        };

        temp_self.seek_to_key(key);
        temp_self
    }

    /// Returns the key of the current entry.
    pub fn key(&self) -> KeySlice {
        self.key.as_key_slice()
    }

    /// Returns the value of the current entry.
    pub fn value(&self) -> &[u8] {
        &self.block.data[self.value_range.0..self.value_range.1]
    }

    /// Returns true if the iterator is valid.
    /// Note: You may want to make use of `key`
    pub fn is_valid(&self) -> bool {
        self.idx < self.block.offsets.len()
    }

    /// Seeks to the first key in the block.
    pub fn seek_to_first(&mut self) {
        self.seek_to_idx(0);
    }

    /// Move to the next key in the block.
    pub fn next(&mut self) {
        self.seek_to_idx(self.idx + 1);
    }

    /// Seek to the first key that >= `key`.
    /// Note: You should assume the key-value pairs in the block are sorted when being added by
    /// callers.
    pub fn seek_to_key(&mut self, key: KeySlice) {
        let mut lo: i32 = 0;
        let mut high: i32 = (self.block.offsets.len() - 1) as i32;

        while lo <= high {
            let mid = (lo + high) / 2;
            self.seek_to_idx(mid as usize);
            let cmp = self.key().cmp(&key);

            if cmp == Ordering::Less {
                lo = mid + 1;
            } else {
                high = mid - 1;
            }
        }
        self.seek_to_idx(lo as usize);
    }

    // fn decode_key(&self,overlap: u16,key_value: &[u8]) -> &[u8] {
    //     let newKey =
    // }
    fn seek_to_idx(&mut self, idx: usize) {
        self.idx = idx;
        // In case someone seeks outside the bounds
        if idx >= self.block.offsets.len() {
            return;
        }
        let start_index = self.block.offsets[idx] as usize;
        let key_overlap =
            (&self.block.data[start_index..start_index + SIZE_OF_U16]).get_u16() as usize;
        let key_rem = (&self.block.data[start_index + SIZE_OF_U16..start_index + 2 * SIZE_OF_U16])
            .get_u16() as usize;

        let rem_key_value = &(self.block.data
            [start_index + 2 * SIZE_OF_U16..start_index + 2 * SIZE_OF_U16 + key_rem]
            .to_vec())[..];
        let rem_key_ts = (&self.block.data[start_index + 2 * SIZE_OF_U16 + key_rem
            ..start_index + 2 * SIZE_OF_U16 + key_rem + U64_SIZE]
            .to_vec()[..])
            .get_u64();
        let value_size = (&self.block.data[start_index + 2 * SIZE_OF_U16 + key_rem + U64_SIZE
            ..start_index + 3 * SIZE_OF_U16 + key_rem + U64_SIZE])
            .get_u16() as usize;
        self.key.clear();
        self.key.append(&self.first_key.key_ref()[..key_overlap]);
        self.key.append(rem_key_value);
        self.key.set_ts(rem_key_ts);
        self.value_range = (
            start_index + 3 * SIZE_OF_U16 + key_rem + U64_SIZE,
            start_index + 3 * SIZE_OF_U16 + key_rem + value_size + U64_SIZE,
        );
    }
}
