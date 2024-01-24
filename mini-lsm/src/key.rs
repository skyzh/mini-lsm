use std::{
    borrow::Borrow,
    cmp::{self, Reverse},
};

use bytes::Bytes;

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct Key(pub Vec<u8>, pub u64);

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct KeyBytes(pub Bytes, pub u64);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct KeySlice<'a>(pub &'a [u8], pub u64);

impl KeyBytes {
    pub fn key_len(&self) -> usize {
        self.0.len()
    }

    pub fn raw_len(&self) -> usize {
        self.0.len() + std::mem::size_of::<u64>()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_key_slice(&self) -> KeySlice<'_> {
        KeySlice(&self.0, self.1)
    }
}

impl Key {
    pub fn key_len(&self) -> usize {
        self.0.len()
    }

    pub fn raw_len(&self) -> usize {
        self.0.len() + std::mem::size_of::<u64>()
    }

    pub fn empty() -> Self {
        Self(Vec::new(), 0)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_key_slice(&self) -> KeySlice<'_> {
        KeySlice(&self.0, self.1)
    }

    pub fn clear(&mut self) {
        self.0.clear();
        self.1 = 0;
    }

    pub fn set_from_slice(&mut self, key_slice: KeySlice) {
        self.0.clear();
        self.0.extend(key_slice.0);
        self.1 = key_slice.1;
    }

    pub fn into_key_bytes(self) -> KeyBytes {
        KeyBytes(self.0.into(), self.1)
    }
}

impl KeySlice<'_> {
    pub fn key_len(&self) -> usize {
        self.0.len()
    }

    pub fn raw_len(&self) -> usize {
        self.0.len() + std::mem::size_of::<u64>()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn to_key(&self) -> Key {
        Key(self.0.to_vec(), self.1)
    }

    pub fn max_ts(key: &[u8]) -> KeySlice<'_> {
        KeySlice(key, std::u64::MAX)
    }

    pub fn min_ts(key: &[u8]) -> KeySlice<'_> {
        KeySlice(key, std::u64::MIN)
    }

    pub fn range_begin(key: &[u8]) -> KeySlice<'_> {
        Self::max_ts(key)
    }

    pub fn range_end(key: &[u8]) -> KeySlice<'_> {
        Self::min_ts(key)
    }
}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.as_key_slice().partial_cmp(&other.as_key_slice())
    }
}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.as_key_slice().cmp(&other.as_key_slice())
    }
}

impl PartialOrd for KeyBytes {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.as_key_slice().partial_cmp(&other.as_key_slice())
    }
}

impl Ord for KeyBytes {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.as_key_slice().cmp(&other.as_key_slice())
    }
}

impl PartialOrd for KeySlice<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        (self.0, Reverse(self.1)).partial_cmp(&(other.0, Reverse(other.1)))
    }
}

impl Ord for KeySlice<'_> {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        (self.0, Reverse(self.1)).cmp(&(other.0, Reverse(other.1)))
    }
}

pub fn extract_raw_key(key_slice: KeySlice<'_>) -> &[u8] {
    key_slice.0
}
