#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use crate::key::{KeySlice, KeyVec};

use super::Block;

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
        }
        let data_size = self.data.len() + key.len() + value.len() + 4;
        let offset_size = self.offsets.len() * 2 + 2;
        if data_size + offset_size > self.block_size && !self.is_empty() {
            return false;
        }
        self.offsets.push(self.data.len() as u16);
        self.data
            .extend_from_slice(&(key.len() as u16).to_le_bytes());
        self.data.extend_from_slice(key.into_inner());
        self.data
            .extend_from_slice(&(value.len() as u16).to_le_bytes());
        self.data.extend_from_slice(value);
        true
    }

    /// Check if there is no key-value pair in the block.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Finalize the block.
    pub fn build(self) -> Block {
        Block {
            data: self.data.clone(),
            offset: self.offset.clone()
        }
    }
}
