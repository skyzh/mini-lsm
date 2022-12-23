mod builder;
mod iterator;

use bytes::{Buf, BufMut, Bytes};

pub use builder::BlockBuilder;
pub use iterator::BlockIterator;

pub const SIZEOF_U16: usize = std::mem::size_of::<u16>();

/// A block is the smallest unit of read and caching in LSM tree. It is a collection of sorted key-value pairs.
pub struct Block {
    pub(self) data: Vec<u8>,
    pub(self) offsets: Vec<u16>,
}

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
        let entry_offsets_len = (&data[data.len() - SIZEOF_U16..]).get_u16() as usize;
        let data_end = data.len() - SIZEOF_U16 - entry_offsets_len * SIZEOF_U16;
        let offsets_raw = &data[data_end..data.len() - SIZEOF_U16];
        let offsets = offsets_raw
            .chunks(SIZEOF_U16)
            .map(|mut x| x.get_u16())
            .collect();
        let data = data[0..data_end].to_vec();
        Self { data, offsets }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{builder::BlockBuilder, iterator::BlockIterator, *};

    #[test]
    fn test_block_build_single_key() {
        let mut builder = BlockBuilder::new(16);
        assert!(builder.add(b"233", b"233333"));
        builder.build();
    }

    #[test]
    fn test_block_build_full() {
        let mut builder = BlockBuilder::new(16);
        assert!(builder.add(b"11", b"11"));
        assert!(!builder.add(b"22", b"22"));
        builder.build();
    }

    fn key_of(idx: usize) -> Vec<u8> {
        format!("key_{:03}", idx).into_bytes()
    }

    fn value_of(idx: usize) -> Vec<u8> {
        format!("value_{:010}", idx).into_bytes()
    }

    fn num_of_keys() -> usize {
        100
    }

    fn generate_block() -> Block {
        let mut builder = BlockBuilder::new(10000);
        for idx in 0..num_of_keys() {
            let key = key_of(idx);
            let value = value_of(idx);
            assert!(builder.add(&key[..], &value[..]));
        }
        builder.build()
    }

    #[test]
    fn test_block_build_all() {
        generate_block();
    }

    #[test]
    fn test_block_encode() {
        let block = generate_block();
        block.encode();
    }

    #[test]
    fn test_block_decode() {
        let block = generate_block();
        let encoded = block.encode();
        let decoded_block = Block::decode(&encoded);
        assert_eq!(block.offsets, decoded_block.offsets);
        assert_eq!(block.data, decoded_block.data);
    }

    fn as_bytes(x: &[u8]) -> Bytes {
        Bytes::copy_from_slice(x)
    }

    #[test]
    fn test_block_iterator() {
        let block = Arc::new(generate_block());
        let mut iter = BlockIterator::create_and_seek_to_first(block);
        for i in 0..num_of_keys() {
            let key = iter.key();
            let value = iter.value();
            assert_eq!(
                key,
                key_of(i),
                "expected key: {:?}, actual key: {:?}",
                as_bytes(&key_of(i)),
                as_bytes(key)
            );
            assert_eq!(
                value,
                value_of(i),
                "expected value: {:?}, actual value: {:?}",
                as_bytes(&value_of(i)),
                as_bytes(value)
            );
            iter.next();
        }
    }
}
