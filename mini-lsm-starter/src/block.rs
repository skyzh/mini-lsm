#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

mod builder;
mod iterator;

pub use builder::BlockBuilder;
use bytes::{BufMut, Bytes, BytesMut};
pub use iterator::BlockIterator;

pub(crate) const SIZEOF_U16: usize = std::mem::size_of::<u16>();

/// A block is the smallest unit of read and caching in LSM tree. It is a collection of sorted key-value pairs.
pub struct Block {
    pub(crate) data: Vec<u8>,
    pub(crate) offsets: Vec<u16>,
}

impl Block {
    /// Encode the internal data to the data layout illustrated in the tutorial
    /// Note: You may want to recheck if any of the expected field is missing from your output
    pub fn encode(&self) -> Bytes {
        let number_of_elements = self.offsets.len() as u16;
        let mut data = self.data.clone();
        for num in &self.offsets {
            data.put_u16(*num);
        }
        data.put_u16(number_of_elements);
        data.into()
    }

    /// Decode from the data layout, transform the input `data` to a single `Block`
    pub fn decode(data: &[u8]) -> Self {
        let total_bytes = data.len();

        let number_of_elements =
            u16::from_be_bytes([data[total_bytes - SIZEOF_U16], data[total_bytes - 1]]) as usize;
        let offsets_start_position = (total_bytes - SIZEOF_U16) - number_of_elements * SIZEOF_U16;
        let key_pairs = data[..offsets_start_position].to_vec();
        let offsets = data[offsets_start_position..total_bytes - SIZEOF_U16]
            .chunks(SIZEOF_U16)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]])) // 转换为小端序的 u16
            .collect();

        Self {
            data: key_pairs,
            offsets,
        }
    }
}
