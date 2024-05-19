#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

mod builder;
mod iterator;

pub use builder::BlockBuilder;
use bytes::BufMut;
use bytes::Bytes;
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
        let entry_size = self.offsets.len();
        let mut result = self.data.clone();
        for offset in &self.offsets {
            result.put_u16(*offset);
        }
        result.push(entry_size as u8);
        result.into()
    }

    /// Decode from the data layout, transform the input `data` to a single `Block`
    pub fn decode(data: &[u8]) -> Self {
        let entry_size = data[data.len() - 1];
        Self {
            // offset is u16 spillit to 2 u8 , so we need to multiply by 2
            // -1 for element number at the end of the data
            data: data[..data.len() - entry_size as usize * 2 - 1].to_vec(),
            offsets: data[data.len() - entry_size as usize * 2 - 1..data.len() - 1]
                .chunks(2)
                .map(|x| u16::from_be_bytes([x[0], x[1]]))
                .collect(),
        }
    }
}
