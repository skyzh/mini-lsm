mod builder;
mod iterator;

pub use builder::BlockBuilder;
use bytes::{BufMut, Bytes, BytesMut};
/// You may want to check `bytes::BufMut` out when manipulating continuous chunks of memory
pub use iterator::BlockIterator;

/// A block is the smallest unit of read and caching in LSM tree.
/// It is a collection of sorted key-value pairs.
/// The `actual` storage format is as below (After `Block::encode`):
///
/// ----------------------------------------------------------------------------------------------------
/// |             Data Section             |              Offset Section             |      Extra      |
/// ----------------------------------------------------------------------------------------------------
/// | Entry #1 | Entry #2 | ... | Entry #N | Offset #1 | Offset #2 | ... | Offset #N | num_of_elements |
/// ----------------------------------------------------------------------------------------------------
pub struct Block {
    data: Vec<u8>,
    offsets: Vec<u16>,
}

impl Block {
    /// Encode the internal data to the data layout illustrated in the tutorial
    /// Note: You may want to recheck if any of the expected field is missing from your output
    pub fn encode(&self) -> Bytes {
        let size = self.data.len() + (self.offsets.len() << 1) + 2;
        let mut buf = BytesMut::with_capacity(size);

        for i in 0..self.data.len() {
            buf.put_u8(self.data[i]);
        }

        for offset in &self.offsets {
            buf.put_u16(*offset);
        }

        buf.put_u16(self.data.len() as u16);

        buf.freeze()
    }

    /// Decode from the data layout, transform the input `data` to a single `Block`
    pub fn decode(data: &[u8]) -> Self {
        let num_of_elements = (data[data.len() - 2] as u16) << 8 | data[data.len() - 1] as u16;

        let block_data: Vec<u8> = data[0..num_of_elements as usize].to_vec();
        let mut block_offset: Vec<u16> = Vec::new();

        let mut idx = num_of_elements as usize;

        while idx < data.len() - 2 {
            block_offset.push((data[idx] as u16) << 8 | (data[idx + 1] as u16));
            idx += 2;
        }

        Block {
            data: block_data,
            offsets: block_offset,
        }
    }
}

#[cfg(test)]
mod tests;
