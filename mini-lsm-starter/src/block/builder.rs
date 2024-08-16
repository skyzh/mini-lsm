#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use bytes::BufMut;

use crate::key::{Key, KeySlice, KeyVec};

use super::Block;

pub(crate) const U16_SIZE: usize = std::mem::size_of::<u16>();

const NUM_ELEMENTS_SIZE: usize = U16_SIZE;

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
        Self {
            offsets: Vec::new(),
            data: Vec::new(),
            block_size,
            first_key: Key::new(),
        }
    }

    pub fn size(&mut self) -> usize {
        NUM_ELEMENTS_SIZE + self.data.len() + (self.offsets.len() * U16_SIZE)
    }

    /// Adds a key-value pair to the block. Returns false when the block is full.
    #[must_use]
    pub fn add(&mut self, key: KeySlice, value: &[u8]) -> bool {
        let estimated_size_for_data = key.len() + value.len();
        // key len, value len, and offset
        let estimated_size_for_metadata = 3 * U16_SIZE;

        if self.size() + estimated_size_for_data + estimated_size_for_metadata > self.block_size {
            if !self.is_empty() {
                return false;
            }
        }

        self.offsets.push(self.data.len() as u16);
        self.data.put_u16(key.len() as u16);
        self.data.put(key.into_inner());
        self.data.put_u16(value.len() as u16);
        self.data.put(value);

        if self.first_key.is_empty() {
            self.first_key = key.to_key_vec();
        }

        true
    }

    /// Check if there is no key-value pair in the block.
    pub fn is_empty(&self) -> bool {
        self.first_key.is_empty()
    }

    /// Finalize the block.
    pub fn build(self) -> Block {
        if self.is_empty() {
            panic!("Block is empty!");
        }
        Block {
            data: self.data,
            offsets: self.offsets,
        }
    }
}
