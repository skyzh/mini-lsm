#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use crate::key::{KeySlice, KeyVec};

use super::Block;

fn diff_key(s1: &KeyVec, s2: &KeyVec) -> KeyVec {
    let mut common_prefix_bytes = 0;

    let mut s1_iter = s1.raw_ref().iter();
    let mut s2_iter = s2.raw_ref().iter();

    loop {
        let s1_char = s1_iter.next();
        let s2_char = s2_iter.next();

        match (s1_char, s2_char) {
            (Some(c1), Some(c2)) if c1 == c2 => {
                common_prefix_bytes += 1;
            }
            _ => break,
        }
    }

    let s2_unique_suffix = s2.raw_ref()[common_prefix_bytes..].to_vec();
    let s2_unique_suffix_length = s2.len() - common_prefix_bytes;

    let mut compose_buf: Vec<u8> = vec![];
    compose_buf.extend((common_prefix_bytes as u16).to_le_bytes());
    compose_buf.extend((s2_unique_suffix_length as u16).to_le_bytes());
    compose_buf.extend(s2_unique_suffix);

    KeyVec::from_vec(compose_buf)
}

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

        let diff_key = if !self.is_empty() {
            diff_key(&self.first_key, &key.to_key_vec())
        } else {
            key.to_key_vec()
        };

        let data_size = self.data.len() + diff_key.len() + value.len() + 4;
        let offset_size = self.offsets.len() * 2 + 2;
        if data_size + offset_size > self.block_size && !self.is_empty() {
            return false;
        }
        self.offsets.push(self.data.len() as u16);
        self.data
            .extend_from_slice(&(diff_key.len() as u16).to_le_bytes());
        self.data.extend_from_slice(diff_key.raw_ref());
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
            offsets: self.offsets.clone(),
        }
    }
}
