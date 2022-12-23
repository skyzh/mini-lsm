#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use super::Block;

/// Builds a block
pub struct BlockBuilder {}

impl BlockBuilder {
    /// Creates a new block builder
    pub fn new(target_size: usize) -> Self {
        unimplemented!()
    }

    /// Adds a key-value pair to the block
    #[must_use]
    pub fn add(&mut self, key: &[u8], value: &[u8]) -> bool {
        unimplemented!()
    }

    pub fn is_empty(&self) -> bool {
        unimplemented!()
    }

    /// Builds a block
    pub fn build(self) -> Block {
        unimplemented!()
    }
}
