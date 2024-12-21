#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use crate::key::{KeySlice, KeyVec};
use bytes::Buf;
use std::cmp::{max, min, Ordering};
use std::sync::Arc;

use super::{Block, SIZEOF_U16};

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
        let (first_key, value_range) = if !block.offsets.is_empty() {
            let (key, key_end_offset) = Self::read_key(&block, 0);
            (key, Self::value_range(&block, key_end_offset))
        } else {
            (KeyVec::new(), (0, 0))
        };

        Self {
            block,
            key: first_key.clone(),
            value_range,
            idx: 0,
            first_key,
        }
    }

    /// Creates a block iterator and seek to the first key that >= `key`.
    pub fn create_and_seek_to_key(block: Arc<Block>, key: KeySlice) -> Self {
        let mut iterator = Self::create_and_seek_to_first(block);
        iterator.seek_to_key(key);
        iterator
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
        !self.key.is_empty()
    }

    /// Seeks to the first key in the block.
    pub fn seek_to_first(&mut self) {
        let (first_key, value_range) = if !self.block.offsets.is_empty() {
            let (key, key_end_offset) = Self::read_key(&self.block, 0);
            (key, Self::value_range(&self.block, key_end_offset))
        } else {
            (KeyVec::new(), (0, 0))
        };

        self.first_key = first_key.clone();
        self.key = first_key;
        self.idx = 0;
        self.value_range = value_range;
    }

    /// Move to the next key in the block.
    pub fn next(&mut self) {
        if self.is_valid() && self.idx < self.block.offsets.len() - 1 {
            self.idx += 1;
            let (key, key_end_offset) =
                Self::read_key(&self.block, self.block.offsets[self.idx] as usize);
            let range = Self::value_range(&self.block, key_end_offset);
            self.value_range = range;
            self.key = key;
        }
    }

    /// Seek to the first key that >= `key`.
    /// Note: You should assume the key-value pairs in the block are sorted when being added by
    /// callers.
    pub fn seek_to_key(&mut self, key: KeySlice) {
        if self.is_valid() {
            let mut right = self.block.offsets.len() - 1;
            let mut left = 0;
            let mut found_key = KeyVec::new();
            let mut found_key_end_offset = 0;
            let key = key.to_key_vec();
            let mut load_key = true;
            while left <= right {
                let mid = left + (right - left) / 2;
                (found_key, found_key_end_offset) =
                    Self::read_key(&self.block, self.block.offsets[mid] as usize);
                match found_key.cmp(&key) {
                    Ordering::Greater => {
                        if mid == 0 {
                            break;
                        }
                        right = mid - 1;
                    }
                    Ordering::Less => {
                        left = mid + 1;
                    }
                    _ => {
                        left = mid;
                        load_key = false;
                        break;
                    }
                }
            }
            left = min(left, self.block.offsets.len() - 1);
            if load_key {
                (found_key, found_key_end_offset) =
                    Self::read_key(&self.block, self.block.offsets[left] as usize);
            }
            self.idx = left;
            self.key = found_key;
            self.value_range = Self::value_range(&self.block, found_key_end_offset);
        }
    }

    fn read_key(block: &Arc<Block>, offset: usize) -> (KeyVec, usize) {
        let key_start_offset = offset + SIZEOF_U16;
        let key_len = (&block.data[offset..key_start_offset]).get_u16() as usize;
        let key_end_offset = key_start_offset + key_len;
        (
            KeyVec::from_vec(block.data[key_start_offset..key_end_offset].to_vec()),
            key_end_offset,
        )
    }

    fn value_range(block: &Arc<Block>, offset: usize) -> (usize, usize) {
        let value_len = (&block.data[offset..offset + SIZEOF_U16]).get_u16() as usize;
        let value_start = offset + SIZEOF_U16;
        let value_end = value_start + value_len;
        (value_start, value_end)
    }
}
