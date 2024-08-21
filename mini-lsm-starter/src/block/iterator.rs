#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::sync::Arc;

use bytes::Buf;

use crate::key::{KeySlice, KeyVec};

use super::{Block, U16_SIZE};

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
        let mut iterator = Self::new(block);
        iterator.seek_to_first();
        iterator
    }

    /// Creates a block iterator and seek to the first key that >= `key`.
    pub fn create_and_seek_to_key(block: Arc<Block>, key: KeySlice) -> Self {
        let mut iterator = Self::new(block);
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
        self.seek_to(0)
    }

    /// Move to the next key in the block.
    pub fn next(&mut self) {
        self.idx += 1;
        self.seek_to(self.idx);
    }

    fn seek_to(&mut self, index: usize) {
        if index >= self.block.offsets.len() {
            self.key.clear();
            self.value_range = (0, 0);
            return;
        }
        let offset = self.block.offsets[index] as usize;
        self.seek_to_offset(offset);
        self.idx = index;
    }

    fn seek_to_offset(&mut self, offset: usize) {
        // Local iteration of block.data to construct the key and value from the offset

        // get slice of data starting from offset. it's okay to take till the
        // end of the slice as we will be advancing our buffer to construct the
        // key and value ranges
        let mut item = &self.block.data[offset..];

        // An entry or item in a block has format key_len, key, val_len, val

        // first extract the key_len, always 2 bytes.
        let key_length = item.get_u16() as usize; // get u16 automatically advances pointer (2 bytes)

        // item started from offset, and advanced by 2 bytes above. We must be at the start of
        // our key. Let's extract it as we know it's length!
        let key = &item[..key_length];

        // the above didn't advance the item. we need to do so as we've read the key and
        // interested in the value length as the next step.
        item.advance(key_length);

        // the next 2 bytes represent the value length.
        let value_length = item.get_u16() as usize; // auto advances 2 bytes

        // Value begins at (where we started -> offset) + (2 bytes to store key leng) + (key itself) + (2 bytes to store val len)
        let value_range_begin = offset + U16_SIZE + key_length + U16_SIZE;

        // Value ends at where it began plus the length of the value itself
        let value_range_end = value_range_begin + value_length;

        // we have to again advance our item as we've read the value range
        item.advance(value_length);

        // Updating our Structure/State:

        // reset the key
        self.key.clear();
        self.key.append(key);

        // set the value range
        self.value_range = (value_range_begin, value_range_end);
    }

    /// Seek to the first key that >= `key`.
    /// Note: You should assume the key-value pairs in the block are sorted when being added by
    /// callers.
    pub fn seek_to_key(&mut self, key: KeySlice) {
        // Since the key-value pairs in the block are sorted, we can do a binary
        // search over the entire search space. Our search space is defined by
        // the total entries in an LSM block, which can be easily obtained by the
        // length of the offsets array as it stores the offset of each entry.
        let mut start_offset = 0;
        let mut end_offset = self.block.offsets.len();

        // <= also works and we'd just return from within in some cases.
        while start_offset < end_offset {
            let mid = (start_offset) + (end_offset - start_offset) / 2;

            // remember we have to seek to extract the key at mid and compare it
            self.seek_to(mid);
            assert!(self.is_valid());
            match self.key.cmp(&key.to_key_vec()) {
                std::cmp::Ordering::Less => start_offset = mid + 1,
                std::cmp::Ordering::Equal => return,
                // since we need the key >= the passed key, we aren't
                // doing mid - 1 here.
                std::cmp::Ordering::Greater => end_offset = mid,
            }
        }

        // typical bisection search pattern, if we came here it means the actual key
        // doesn't exist although the start landed at just the key greater than the passed
        // one.
        self.seek_to(start_offset)
    }
}
