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

use std::sync::Arc;

use crate::key::{KeySlice, KeyVec};

use super::Block;

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

    /// Creates a block iterator and seek to the first entry.
    pub fn create_and_seek_to_first(block: Arc<Block>) -> Self {
        let mut iter = Self::new(block);
        iter.seek_to_first();
        iter
    }

    /// Creates a block iterator and seek to the first key that >= `key`.
    pub fn create_and_seek_to_key(block: Arc<Block>, key: KeySlice) -> Self {
        let mut iter = Self::new(block);
        iter.seek_to_key(key);
        iter
    }

    /// Returns the key of the current entry.
    pub fn key(&self) -> KeySlice<'_> {
        self.key.as_key_slice()
    }

    /// Returns the value of the current entry.
    pub fn value(&self) -> &[u8] {
        &self.block.data[self.value_range.0..self.value_range.1]
    }

    /// Returns true if the iterator is valid.
    /// Note: You may want to make use of `key`
    pub fn is_valid(&self) -> bool {
        !self.key.is_empty()
    }

    /// Seeks to the first key in the block.
    pub fn seek_to_first(&mut self) {
        self.first_key.clear();
        self.seek_to_idx(0);
    }

    /// Move to the next key in the block.
    pub fn next(&mut self) {
        self.seek_to_idx(self.idx.saturating_add(1));
    }

    /// Seek to the first key that >= `key`.
    /// Note: You should assume the key-value pairs in the block are sorted when being added by
    /// callers.
    pub fn seek_to_key(&mut self, key: KeySlice) {
        self.seek_to_first();
        let mut low = 0;
        let mut high = self.block.offsets.len();
        while low < high {
            let mid = low + (high - low) / 2;
            self.seek_to_idx(mid);
            if self.key() < key {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        self.seek_to_idx(low);
    }

    fn seek_to_idx(&mut self, idx: usize) {
        self.idx = idx;
        if idx >= self.block.offsets.len() {
            self.key.clear();
            self.value_range = (0, 0);
            return;
        }

        let data = &self.block.data;
        let entry_start = self.block.offsets[idx] as usize;
        assert!(entry_start + 4 <= data.len(), "truncated block key header");
        let overlap =
            u16::from_be_bytes(data[entry_start..entry_start + 2].try_into().unwrap()) as usize;
        let rest_len =
            u16::from_be_bytes(data[entry_start + 2..entry_start + 4].try_into().unwrap()) as usize;
        let rest_start = entry_start + 4;
        let rest_end = rest_start
            .checked_add(rest_len)
            .expect("block key length overflow");
        assert!(rest_end + 2 <= data.len(), "truncated block key");
        assert!(
            overlap <= self.first_key.len(),
            "key overlap exceeds first key"
        );
        let value_len =
            u16::from_be_bytes(data[rest_end..rest_end + 2].try_into().unwrap()) as usize;
        let value_start = rest_end + 2;
        let value_end = value_start
            .checked_add(value_len)
            .expect("block value length overflow");
        assert!(value_end <= data.len(), "truncated block value");

        self.key.clear();
        self.key.append(&self.first_key.raw_ref()[..overlap]);
        self.key.append(&data[rest_start..rest_end]);
        if idx == 0 {
            assert_eq!(overlap, 0, "first block key must have zero overlap");
            self.first_key.set_from_slice(self.key.as_key_slice());
        }
        self.value_range = (value_start, value_end);
    }
}
