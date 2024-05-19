use crate::key::{KeySlice, KeyVec};
use bytes::BufMut;

use super::Block;

const U16_SIZE: usize = 2;

/// Builds a block.
pub struct BlockBuilder {
    /// Offsets of each key-value entries.
    /// | 0 | Entry1_len |
    offsets: Vec<u16>,
    /// All serialized key-value pairs in the block.
    /// |key_len(2)|Key|value_len(2)|value|
    data: Vec<u8>,
    /// The expected block size.
    block_size: usize,
    /// The first key in the block
    first_key: KeyVec,
}

impl BlockBuilder {
    /// Creates a new block builder.
    pub fn new(block_size: usize) -> Self {
        Self {
            offsets: vec![],
            data: vec![],
            block_size,
            first_key: KeyVec::new(),
        }
    }

    /// Adds a key-value pair to the block. Returns false when the block is full.
    #[must_use]
    pub fn add(&mut self, key: KeySlice, value: &[u8]) -> bool {
        if self.is_empty() {
            self.first_key = key.to_key_vec();
        } else {
            // offsets is u16 and data is u8 so we need to multiply by 2,last U16 is element number at the end of the data
            let data_size = self.data.len() + self.offsets.len() * U16_SIZE + U16_SIZE;
            // 3 U16 for key_len, value_len, offset
            let entry_size = key.len() + value.len() + U16_SIZE * 3;
            // println!("data_size: {}, entry_size: {}", data_size, entry_size);
            if data_size + entry_size > self.block_size {
                return false;
            }
        }

        self.offsets.push(self.data.len() as u16);
        self.data.put_u16(key.len() as u16);
        self.data.put(key.raw_ref());
        self.data.put_u16(value.len() as u16);
        self.data.put(value);
        true
    }

    /// Check if there is no key-value pair in the block.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Finalize the block.
    pub fn build(self) -> Block {
        Block {
            data: (self.data),
            offsets: (self.offsets),
        }
    }
}
