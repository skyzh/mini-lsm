#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

mod builder;
mod iterator;

pub use builder::BlockBuilder;
use bytes::Bytes;
pub use iterator::BlockIterator;

/// A block is the smallest unit of read and caching in LSM tree. It is a collection of sorted
/// key-value pairs.
pub struct Block {
    data: Vec<u8>,
    offsets: Vec<u16>,
}

impl Block {
    pub fn encode(&self) -> Bytes {
        unimplemented!()
    }

    pub fn decode(data: &[u8]) -> Self {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests;
