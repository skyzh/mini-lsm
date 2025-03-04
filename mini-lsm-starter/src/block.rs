#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

mod builder;
mod iterator;
use std::mem;

pub use builder::BlockBuilder;
use bytes::{Buf, BufMut, Bytes};
pub use iterator::BlockIterator;

pub(crate) const SIZE_OF_U16: usize = mem::size_of::<u16>();

/// A block is the smallest unit of read and caching in LSM tree. It is a collection of sorted key-value pairs.
pub struct Block {
    pub(crate) data: Vec<u8>,
    pub(crate) offsets: Vec<u16>,
}

impl Block {
    /// Encode the internal data to the data layout illustrated in the tutorial
    /// Note: You may want to recheck if any of the expected field is missing from your output
    pub fn encode(&self) -> Bytes {
        let mut ans = self.data.clone();
        let offset_len = self.offsets.len();
        // println!("{}", ans.len());
        for offset_data in self.offsets.iter() {
            ans.put_u16(*offset_data);
        }
        ans.put_u16(offset_len as u16);
        // println!("{}",offset_len);
        ans.into()
    }

    /// Decode from the data layout, transform the input `data` to a single `Block`
    pub fn decode(data: &[u8]) -> Self {
        let data_points = (&data[data.len() - SIZE_OF_U16..data.len()]).get_u16() as usize;
        let data_end = data.len() - SIZE_OF_U16 - SIZE_OF_U16 * data_points;
        let offset_raw = &data[data_end..data.len() - SIZE_OF_U16];
        let offset = offset_raw
            .chunks(SIZE_OF_U16)
            .map(|mut x| x.get_u16())
            .collect();
        let data_value = data[0..data_end].to_vec();
        Self {
            data: data_value,
            offsets: offset,
        }
    }
}
