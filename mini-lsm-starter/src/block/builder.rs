use bytes::BufMut;

use super::{Block, U16_SIZE};

/// Builds a block.
pub struct BlockBuilder {
    block_size: usize,
    data: Vec<u8>,
    offsets: Vec<u16>,
}

impl BlockBuilder {
    /// Creates a new block builder.
    pub fn new(block_size: usize) -> Self {
        Self {
            block_size,
            data: Vec::default(),
            offsets: Vec::default(),
        }
    }

    /// Adds a key-value pair to the block. Returns false when the block is full.
    #[must_use]
    pub fn add(&mut self, key: &[u8], value: &[u8]) -> bool {
        assert!(!key.is_empty(), "Cannot add an entry for an empty key");
        let key_len: u16 = key
            .len()
            .try_into()
            .expect("Cannot add key bigger than max length of 65536");
        let value_len: u16 = value
            .len()
            .try_into()
            .expect("Cannot add key bigger than max length of 65536");

        if self.current_block_size() + key_len as usize + value_len as usize + 3 * U16_SIZE
            > self.block_size
            && !self.is_empty()
        {
            return false;
        }
        self.offsets.push(self.data.len() as u16);

        self.data.put_u16(key_len);
        self.data.put(key);
        self.data.put_u16(value_len);
        self.data.put(value);

        true
    }

    /// Check if there is no key-value pair in the block.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Finalize the block.
    pub fn build(self) -> Block {
        assert!(!self.is_empty(), "Cannot buiold block with no entry");

        Block {
            data: self.data,
            offsets: self.offsets,
        }
    }

    /// Utility function to get the current block size in serialized form, in number of bytes
    fn current_block_size(&self) -> usize {
        self.data.len() + self.offsets.len() * U16_SIZE + U16_SIZE
    }
}
