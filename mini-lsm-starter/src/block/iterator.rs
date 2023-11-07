use std::sync::Arc;

use super::Block;

/// Iterates on a block.
pub struct BlockIterator {
    /// The internal `Block`, wrapped by an `Arc`
    block: Arc<Block>,
    /// The current key, empty represents the iterator is invalid
    key: Vec<u8>,
    /// The corresponding value, can be empty
    value: Vec<u8>,
    /// Current index of the key-value pair, should be in range of [0, num_of_elements)
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
        let mut block_iterator = BlockIterator::new(block);
        block_iterator.seek_to_first();
        block_iterator
    }

    /// Creates a block iterator and seek to the first key that >= `key`.
    pub fn create_and_seek_to_key(block: Arc<Block>, key: &[u8]) -> Self {
        let mut block_iterator = BlockIterator::new(block);
        block_iterator.seek_to_key(key);
        block_iterator
    }

    /// Returns the key of the current entry.
    pub fn key(&self) -> &[u8] {
        assert!(self.is_valid());
        &self.key
    }

    /// Returns the value of the current entry.
    pub fn value(&self) -> &[u8] {
        assert!(self.is_valid());
        &self.value
    }

    /// Returns true if the iterator is valid.
    /// Note: You may want to make use of `key`
    pub fn is_valid(&self) -> bool {
        !self.key.is_empty()
    }

    /// Seeks to the first key in the block.
    pub fn seek_to_first(&mut self) {
        self.seek_to(0);
    }

    /// Move to the next key in the block.
    pub fn next(&mut self) {
        self.seek_to(self.idx + 1);
    }

    /// Seek to the first key that >= `key`.
    /// Note: You should assume the key-value pairs in the block are sorted when being added by callers.
    pub fn seek_to_key(&mut self, key: &[u8]) {
        // Binary Search
        let mut low = 0;
        let mut high = self.block.offsets.len();
        while low < high {
            let mid = low + (high - low) / 2;
            self.seek_to(mid);
            assert!(self.is_valid());
            match self.key().cmp(key) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => high = mid,
            }
        }
        self.seek_to(low);
    }

    fn seek_to(&mut self, index: usize) {
        self.idx = index;
        self.key.clear();
        self.value.clear();

        if index >= self.block.offsets.len() {
            return;
        }

        // get offset
        let offset = self.block.offsets[index] as usize;

        // first 2 bytes key len
        let key_len =
            (self.block.data[offset] as usize) << 8 | self.block.data[offset + 1] as usize;

        // next key_len bytes is key
        for i in (offset + 2)..(key_len + offset + 2) {
            self.key.push(self.block.data[i]);
        }

        // next 2 bytes value len
        let value_len = (self.block.data[key_len + offset + 2] as usize) << 8
            | self.block.data[key_len + offset + 3] as usize;

        // next value_len bytes is value
        for i in (key_len + offset + 4)..(key_len + offset + 4 + value_len) {
            self.value.push(self.block.data[i]);
        }
    }
}
