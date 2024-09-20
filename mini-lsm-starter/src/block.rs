#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

mod builder;
mod iterator;

pub use builder::BlockBuilder;
use bytes::{Bytes, BytesMut};
pub use iterator::BlockIterator;

/// A block is the smallest unit of read and caching in LSM tree. It is a collection of sorted key-value pairs.
pub struct Block {
    pub(crate) data: Vec<u8>,
    pub(crate) offsets: Vec<u16>,
}

impl Block {
    /// Encode the internal data to the data layout illustrated in the tutorial
    /// Note: You may want to recheck if any of the expected field is missing from your output
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();
        buf.extend_from_slice(&self.data);
        for offset in self.offsets.iter() {
            buf.extend_from_slice(&offset.to_le_bytes());
        }
        buf.extend_from_slice(&(self.offsets.len() as u16).to_le_bytes());
        buf.freeze()
    }

    /// Read a u16 from `&[u8]` after the given cursor position
    pub fn read_u16(data: &[u8], cursor: usize) -> u16 {
        let offset_bytes = &data[cursor..cursor + 2];
        u16::from_le_bytes([offset_bytes[0], offset_bytes[1]])
    }

    /// Decode from the data layout, transform the input `data` to a single `Block`
    pub fn decode(data: &[u8]) -> Self {
        let num_of_offsets = Block::read_u16(data, data.len() - 2);
        let offsets: Vec<u16> = (0..num_of_offsets)
            .rev()
            .map(|index| Block::read_u16(data, data.len() - 2 * (index + 2) as usize))
            .collect();
        let data_len = data.len() - 2 * (num_of_offsets + 1) as usize;
        Block {
            offsets,
            data: data[..data_len].to_vec(),
        }
    }
}
