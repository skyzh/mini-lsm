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

    /// Read the key and value length from the block data at the given offset.
    fn read_entry_by_offset(&self, offset: usize) -> (usize, &[u8], usize, (usize, usize)) {
        let key_len = Block::read_u16(&self.block.data, offset) as usize;
        let key_range_end = offset + key_len + 2;
        let value_len = Block::read_u16(&self.block.data, key_range_end) as usize;
        (
            key_len,
            &self.block.data[offset + 2..offset + 2 + key_len],
            value_len,
            (key_range_end + 2, key_range_end + 2 + value_len),
        )
    }

    /// Seeks to the first key in the block.
    pub fn seek_to_first(&mut self) {
        self.idx = 0;
        if self.block.data.is_empty() {
            return;
        }
        // Entry format: key_len(2) | key(key_len) | value_len(2) | value(value_len)
        let (key_len, key_slice, value_len, value_range) = self.read_entry_by_offset(0);
        self.key = KeyVec::from_vec(key_slice.to_vec());
        self.first_key = self.key.clone();
        self.value_range = value_range;
    }

    /// Move to the next key in the block.
    pub fn next(&mut self) {
        if self.idx >= self.block.offsets.len() - 1 {
            self.key.clear();
            return;
        }
        self.idx += 1;
        let (key_len, key_slice, value_len, value_range) =
            self.read_entry_by_offset(self.block.offsets[self.idx] as usize);
        self.key = KeyVec::from_vec(key_slice.to_vec());
        self.value_range = value_range;
    }

    /// Seek to the first key that >= `key`.
    /// Note: You should assume the key-value pairs in the block are sorted when being added by
    /// callers.
    pub fn seek_to_key(&mut self, key: KeySlice) {
        if self.block.data.is_empty() {
            return;
        }
        for (i, offset) in self.block.offsets.iter().enumerate() {
            let offset = *offset as usize;
            let (key_len, key_slice, ..) = self.read_entry_by_offset(offset);
            if key_slice >= key.raw_ref() {
                self.idx = i;
                break;
            }
        }

        let (key_len, key_slice, value_len, value_range) =
            self.read_entry_by_offset(self.block.offsets[self.idx] as usize);
        self.key = KeyVec::from_vec(key_slice.to_vec());
        self.value_range = value_range;
    }
}
