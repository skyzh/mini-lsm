use bytes::Buf;
use std::sync::Arc;

use super::Block;

/// Iterates on a block.
pub struct BlockIterator {
    block: Arc<Block>,
    key: Vec<u8>,
    value: Vec<u8>,
    idx: usize,
}

impl BlockIterator {
    fn new(block: Arc<Block>) -> Self {
        Self {
            block,
            key: Vec::new(),
            value: Vec::new(),
            idx: 0,
        }
    }

    /// Creates a block iterator and seek to the first entry.
    pub fn create_and_seek_to_first(block: Arc<Block>) -> Self {
        let mut iterator = BlockIterator::new(block);
        iterator.seek_to_first();
        iterator
    }

    /// Creates a block iterator and seek to the first key that >= `key`.
    pub fn create_and_seek_to_key(block: Arc<Block>, key: &[u8]) -> Self {
        let mut iterator = BlockIterator::new(block);
        iterator.seek_to_key(key);
        iterator
    }

    /// Returns the key of the current entry.
    pub fn key(&self) -> &[u8] {
        &self.key
    }

    /// Returns the value of the current entry.
    pub fn value(&self) -> &[u8] {
        &self.value
    }

    /// Returns true if the iterator is valid.
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
        self.seek_to(self.idx)
    }

    /// Seek to the first key that >= `key`.
    pub fn seek_to_key(&mut self, key: &[u8]) {
        // implemented with dochotomy ince we know keys are sorted in a block
        let mut low = 0;
        let mut high = self.block.offsets.len();
        while low < high {
            let mid = low + (high - low) / 2;
            self.seek_to(mid);
            assert!(self.is_valid());
            match self.key().cmp(key) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => return,
            }
        }
        self.seek_to(low);
    }

    fn seek_to(&mut self, idx: usize) {
        if idx >= self.block.offsets.len() {
            // we reached the end of the block, let's invalidate the iterator
            self.key.clear();
            self.value.clear();
            return;
        }
        // get the data of the entry and put it into some Bytes
        let entry_start = self.block.offsets[idx] as usize;

        let mut entry_raw = &self.block.data[entry_start..];

        // read the entry into self key and value
        let key_len = entry_raw.get_u16();
        self.key.clear();
        self.key
            .extend(entry_raw.copy_to_bytes(key_len as usize).to_vec());

        let value_len = entry_raw.get_u16();
        self.value.clear();
        self.value
            .extend(entry_raw.copy_to_bytes(value_len as usize).to_vec());

        self.idx = idx
    }
}
