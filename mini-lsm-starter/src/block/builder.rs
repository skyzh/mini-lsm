use std::u16;

use bytes::BufMut;

use super::Block;

/// Builds a block.
pub struct BlockBuilder {
    data: Vec<u8>,
    offsets: Vec<u16>,
    size: usize,
}

impl BlockBuilder {
    /// Creates a new block builder.
    pub fn new(block_size: usize) -> Self {
        BlockBuilder {
            data: Vec::new(),
            offsets: Vec::new(),
            size: block_size,
        }
    }

    /// Adds a key-value pair to the block. Returns false when the block is full.
    #[must_use]
    pub fn add(&mut self, key: &[u8], value: &[u8]) -> bool {
        let key_size = key.len();
        let value_size = value.len();

        if self.data.len() + (self.offsets.len() << 1) + key_size + value_size + 6 > self.size {
            return false;
        }

        // push the current offset
        self.offsets.push(self.data.len() as u16); // as takes lower bits.

        // push key length
        self.data.put_u16(key_size as u16); // MSB LSB

        // push key
        for key_item in key {
            self.data.push(*key_item);
        }

        // push value length
        self.data.put_u16(value_size as u16); // MSB LSB

        // push value
        for value_item in value {
            self.data.push(*value_item);
        }

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
}
