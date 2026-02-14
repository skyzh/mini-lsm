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

use std::sync::Arc;

use bytes::Buf;

use crate::key::{Key, KeySlice, KeyVec};

use super::{Block, SIZE_OF_U16};

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
        let first_key = Self::get_first_key(block.clone());

        Self {
            block,
            key: first_key.clone(),
            value_range: (0, 0),
            idx: 0,
            first_key,
        }
    }

    fn get_first_key(block: Arc<Block>) -> KeyVec {
        let mut data = &block.data[0..];
        data.get_u16(); // skip the first key overlap_len
        let key_len = data.get_u16() as usize;

        KeyVec::from_vec(data[..key_len].to_vec())
    }

    /// Creates a block iterator and seek to the first entry.
    pub fn create_and_seek_to_first(block: Arc<Block>) -> Self {
        let mut ret = BlockIterator::new(block.clone());
        ret.seek_to_first();

        ret
    }

    /// Creates a block iterator and seek to the first key that >= `key`.
    pub fn create_and_seek_to_key(block: Arc<Block>, key: KeySlice) -> Self {
        let mut ret = Self::new(block.clone());
        ret.seek_to_key(key);

        let mut lo = 0;
        let mut hi = block.offsets.len() - 1;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let i = block.offsets[mid] as usize;

            let mut data = &block.data[i..];
            let overlap_len = data.get_u16() as usize;
            let ret_key_len = data.get_u16() as usize;
            let ret_key = &data[..ret_key_len];
            ret.key.clear();
            ret.key.append(&ret.first_key.raw_ref()[..overlap_len]);
            ret.key.append(ret_key);
            match Key::from_slice(ret.key.raw_ref()).cmp(&key) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => {
                    lo = mid;
                    break;
                }
            }
        }

        let i = block.offsets[lo] as usize;
        let mut data = &block.data[i..];
        let overlap_len = data.get_u16() as usize;
        let ret_key_len = data.get_u16() as usize;
        let ret_key = &data[..ret_key_len];
        ret.key.clear();
        ret.key.append(&ret.first_key.raw_ref()[..overlap_len]);
        ret.key.append(ret_key);

        ret.idx = lo;

        ret
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
        !self.key().is_empty()
    }

    /// Seeks to the first key in the block.
    pub fn seek_to_first(&mut self) {
        self.seek_to_idx(0);
    }

    /// idx is the index of block.offsets
    fn seek_to_idx(&mut self, idx: usize) {
        self.idx = idx;
        // if invalid, set is_valid state by reset values
        if self.idx >= self.block.offsets.len() {
            self.key.clear();
            self.value_range = (0, 0);
            return;
        }

        let offset = self.block.offsets[self.idx] as usize;
        let mut data = &self.block.data[offset..];
        let overlap_len = data.get_u16() as usize;
        let ret_key_len = data.get_u16() as usize;
        let ret_key = &data[..ret_key_len];
        self.key.clear();
        self.key.append(&self.first_key.raw_ref()[..overlap_len]);
        self.key.append(ret_key);
        data.advance(ret_key_len);
        let value_len = data.get_u16() as usize;
        self.value_range = (
            offset + SIZE_OF_U16 * 2 + ret_key_len + SIZE_OF_U16,
            offset + SIZE_OF_U16 * 2 + ret_key_len + SIZE_OF_U16 + value_len,
        );
    }

    /// Move to the next key in the block.
    pub fn next(&mut self) {
        self.idx += 1;
        self.seek_to_idx(self.idx);
    }

    /// Seek to the first key that >= `key`.
    /// Note: You should assume the key-value pairs in the block are sorted when being added by
    /// callers.
    pub fn seek_to_key(&mut self, key: KeySlice) {
        let mut lo = 0;
        let mut hi = self.block.offsets.len() - 1;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            self.seek_to_idx(mid);
            match Key::from_slice(self.key.raw_ref()).cmp(&key) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => {
                    lo = mid;
                    break;
                }
            }
        }

        self.seek_to_idx(lo);
    }
}
