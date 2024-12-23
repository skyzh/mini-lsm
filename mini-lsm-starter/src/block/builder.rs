use crate::key::{KeySlice, KeyVec};
use bytes::BufMut;

use super::{Block, SIZEOF_U16};

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
        BlockBuilder {
            offsets: Vec::new(),
            data: Vec::new(),
            block_size,
            first_key: KeyVec::new(),
        }
    }

    /// Adds a key-value pair to the block. Returns false when the block is full.
    #[must_use]
    pub fn add(&mut self, key: KeySlice, value: &[u8]) -> bool {
        let key_len = key.len();
        let value_len = value.len();

        if key_len + value_len + SIZEOF_U16 * 3 + self.current_size() > self.block_size
            && !self.is_empty()
        {
            return false;
        }

        if self.offsets.is_empty() {
            self.first_key = key.to_key_vec().clone();
        }

        self.offsets.push(self.data.len() as u16);
        self.data.put_u16(key_len as u16);
        self.data.extend(key.raw_ref());
        self.data.put_u16(value_len as u16);
        self.data.extend(value);
        true
    }

    /// Check if there is no key-value pair in the block.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Finalize the block.
    pub fn build(self) -> Block {
        Block {
            data: self.data,
            offsets: self.offsets,
        }
    }

    fn current_size(&self) -> usize {
        self.data.len() + self.offsets.len() * SIZEOF_U16 + SIZEOF_U16
    }

    pub fn first_key(&self) -> KeyVec {
        self.first_key.clone()
    }
}
