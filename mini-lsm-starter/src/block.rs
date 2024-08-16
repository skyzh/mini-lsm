#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

mod builder;
mod iterator;

pub use builder::BlockBuilder;
use bytes::{Buf, BufMut, Bytes};
pub use iterator::BlockIterator;

pub(crate) const U16_SIZE: usize = std::mem::size_of::<u16>();

/// A block is the smallest unit of read and caching in LSM tree. It is a collection of sorted key-value pairs.
pub struct Block {
    pub(crate) data: Vec<u8>,
    pub(crate) offsets: Vec<u16>,
}

impl Block {
    /// Encode the internal data to the data layout illustrated in the tutorial
    /// Note: You may want to recheck if any of the expected field is missing from your output
    pub fn encode(&self) -> Bytes {

        // data (k,v pair) at start of block
        let mut buf = self.data.clone();
        for offset in &self.offsets {
            // all offsets in middle of block
            buf.put_u16(*offset);
        }
        // number of elements to end of block
        buf.put_u16(self.offsets.len()  as u16);
        buf.into()
    }

    /// Decode from the data layout, transform the input `data` to a single `Block`
    /// 4,plum,5,fruit,5,onion,5,vegetable,0,13,2
    pub fn decode(data: &[u8]) -> Self {
        let num_elements_in_block = (&data[data.len() - U16_SIZE..]).get_u16() as usize;
        let data_end_slice = data.len() - U16_SIZE - (num_elements_in_block * U16_SIZE);
        let offsets = &data[data_end_slice..data.len() - U16_SIZE];
        let offsets = offsets.chunks(U16_SIZE).map(|mut x| x.get_u16()).collect();

        let data = data[0..data_end_slice].into();

        Self { data, offsets}
    }
}
