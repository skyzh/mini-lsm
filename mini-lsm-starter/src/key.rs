use std::{
    borrow::Borrow,
    cmp::{self, Reverse},
};

use bytes::Bytes;

pub type Key = Vec<u8>;

pub type KeyBytes = Bytes;

pub type KeySlice<'a> = &'a [u8];

pub fn extract_raw_key(key_slice: &[u8]) -> &[u8] {
    key_slice
}
