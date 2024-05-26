use std::{cmp::Ordering, sync::Arc};

use crate::key::{Key, KeySlice, KeyVec};

use super::Block;
use std::ops::Range;

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
    is_valid: bool,
}

impl BlockIterator {
    fn new(block: Arc<Block>) -> Self {
        Self {
            block,
            key: KeyVec::new(),
            value_range: (0, 0),
            idx: 0,
            first_key: KeyVec::new(),
            is_valid: true,
        }
    }
    fn get_key_range(&self, idx: usize) -> Range<usize> {
        let key_start = self.block.offsets[idx] as usize;
        let key_len =
            u16::from_be_bytes([self.block.data[key_start], self.block.data[key_start + 1]])
                as usize;
        // first 2 elements in data is length so need to skip it
        key_start + 2..key_start + 2 + key_len
    }
    fn get_value_range(&self, idx: usize) -> (usize, usize) {
        let key_start = self.block.offsets[idx] as usize;
        let key_len =
            u16::from_be_bytes([self.block.data[key_start], self.block.data[key_start + 1]])
                as usize;

        let value_start = key_start + 2 + key_len;
        let value_len = u16::from_be_bytes([
            self.block.data[value_start],
            self.block.data[value_start + 1],
        ]) as usize;

        // first 2 elements in data is length so need to skip it
        (value_start + 2, value_start + 2 + value_len)
    }

    /// Creates a block iterator and seek to the first entry.
    pub fn create_and_seek_to_first(block: Arc<Block>) -> Self {
        assert!(!block.data.is_empty());
        assert!(!block.offsets.is_empty());

        let mut block_iter = Self::new(block);
        block_iter.seek_to_first();
        block_iter
    }

    /// Creates a block iterator and seek to the first key that >= `key`.
    pub fn create_and_seek_to_key(block: Arc<Block>, key: KeySlice) -> Self {
        assert!(!block.data.is_empty());
        assert!(!block.offsets.is_empty());

        let mut block_iter = Self::create_and_seek_to_first(block);
        block_iter.seek_to_key(key);
        block_iter
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
        !self.key.is_empty() && self.is_valid
        // !self.key.is_empty()
    }

    /// Seeks to the first key in the block.
    pub fn seek_to_first(&mut self) {
        assert!(!self.block.data.is_empty());
        // update value range
        if self.first_key.is_empty() {
            self.first_key = KeyVec::from_vec(self.block.data[self.get_key_range(0)].to_vec());
        }
        self.key = self.first_key.clone();

        self.idx = 0;
        self.value_range = self.get_value_range(0);
    }

    /// Move to the next key in the block.
    pub fn next(&mut self) {
        assert!(self.is_valid());

        if self.idx >= self.block.offsets.len() - 1 {
            self.key = KeyVec::new();
            return;
        }

        self.idx += 1;
        self.key = KeyVec::from_vec(self.block.data[self.get_key_range(self.idx)].to_vec());
        self.value_range = self.get_value_range(self.idx);
    }

    /// Seek to the first key that >= `key`.
    /// Note: You should assume the key-value pairs in the block are sorted when being added by
    /// callers.
    pub fn seek_to_key(&mut self, key: KeySlice) {
        self.is_valid = true;
        
        if self.key == key.to_key_vec() {
            return;
        }
        // target key is not exzit, must need is_valid = false
        let last_key = KeyVec::from_vec(
            self.block.data[self.get_key_range(self.block.offsets.len() - 1)].to_vec(),
        );

        if key.cmp(&last_key.as_key_slice()) == Ordering::Greater {
            self.seek_to_first();
            self.is_valid = false;
            return;
        }

        match key.cmp(&self.key.as_key_slice()){
            Ordering::Equal=>return,
            Ordering::Less=> self.seek_to_first(),
            Ordering::Greater=>{
                while self.is_valid() && key.cmp(&self.key.as_key_slice()) == Ordering::Greater {
                    self.next();
                }
            }
        }
    }
}
