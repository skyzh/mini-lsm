use super::{Block, SIZEOF_U16};
use bytes::BufMut;

/// Builds a block.
pub struct BlockBuilder {
    /// Offsets of each key-value entries.
    offsets: Vec<u16>,
    /// All key-value pairs in the block.
    data: Vec<u8>,
    /// The expected block size.
    block_size: usize,
}

impl BlockBuilder {
    /// Creates a new block builder.
    pub fn new(block_size: usize) -> Self {
        Self {
            offsets: Vec::new(),
            data: Vec::new(),
            block_size: block_size,
        }
    }

    /// Adds a key-value pair to the block. Returns false when the block is full.
    #[must_use]
    pub fn add(&mut self, key: &[u8], value: &[u8]) -> bool {
        let key_len = key.len();
        let value_len: usize = value.len();
        // add new data failed if exceed the block size limit
        if self.current_size() + key_len + value_len + 2 * SIZEOF_U16 > self.block_size {
            print!("exceed block size limit");
            return false;
        }

        self.offsets.push(self.data.len() as u16);
        self.data.put_u16(key_len as u16);
        self.data.put(key);
        self.data.put_u16(value_len as u16);
        self.data.put(value);
        true
    }

    /// Check if there is no key-value pair in the block.
    pub fn is_empty(&self) -> bool {
        return self.offsets.is_empty();
    }

    /// Finalize the block.
    pub fn build(self) -> Block {
        Block {
            data: self.data,
            offsets: self.offsets,
        }
    }

    fn current_size(&self) -> usize {
        self.offsets.len() * SIZEOF_U16 + self.data.len() + SIZEOF_U16
    }
}
