use bytes::Bytes;

pub struct Key(Vec<u8>, u64);
pub struct KeyBytes(Bytes, u64);
pub struct KeySlice<'a>(&'a [u8], u64);
