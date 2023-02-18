mod builder;
mod iterator;

pub use builder::BlockBuilder;
use bytes::{Buf, BufMut, Bytes};
pub use iterator::BlockIterator;

/// A block is the smallest unit of read and caching in LSM tree. It is a collection of sorted
/// key-value pairs. The creator is responsible for sorting the key value pairs when building the block.
/// A block serializes to less than or equal to a given size in bytes.
pub struct Block {
    data: Vec<u8>,
    offsets: Vec<u16>,
}

const U16_SIZE: usize = std::mem::size_of::<u16>();

impl Block {
    pub fn encode(&self) -> Bytes {
        let mut buf = self.data.clone();
        let offsets_len = self.offsets.len();
        for offset in &self.offsets {
            buf.put_u16(*offset);
        }
        buf.put_u16(offsets_len as u16);
        buf.into()
    }

    pub fn decode(data: &[u8]) -> Self {
        let number_of_entries = (&data[data.len() - U16_SIZE..]).get_u16() as usize;
        let data_end_idx = data.len() - U16_SIZE * (number_of_entries + 1);
        // now read the offsets
        let offsets_bytes = &data[data_end_idx..data.len() - U16_SIZE];
        let offsets: Vec<u16> = offsets_bytes
            .chunks(U16_SIZE)
            .map(|mut x| x.get_u16())
            .collect();
        Block {
            data: data[..data_end_idx].to_vec(),
            offsets,
        }
    }
}

#[cfg(test)]
mod tests;
