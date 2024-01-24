use std::fmt::Debug;

use bytes::Bytes;

pub struct Key<T: AsRef<[u8]>>(T);

pub type KeySlice<'a> = Key<&'a [u8]>;
pub type KeyVec = Key<Vec<u8>>;
pub type KeyBytes = Key<Bytes>;

impl<T: AsRef<[u8]>> Key<T> {
    pub fn into_inner(self) -> T {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.as_ref().len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.as_ref().is_empty()
    }
}

impl Key<Vec<u8>> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn from_vec(key: Vec<u8>) -> Self {
        Self(key)
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }

    pub fn append(&mut self, data: &[u8]) {
        self.0.extend(data)
    }

    pub fn set_from_slice(&mut self, key_slice: KeySlice) {
        self.0.clear();
        self.0.extend(key_slice.0);
    }

    pub fn as_key_slice(&self) -> KeySlice {
        Key(self.0.as_slice())
    }

    pub fn into_key_bytes(self) -> KeyBytes {
        Key(self.0.into())
    }

    pub fn for_testing_key_ref(&self) -> &[u8] {
        self.0.as_ref()
    }

    /// Always use `raw_ref` to access the key in week 1 + 2.
    pub fn raw_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl Key<Bytes> {
    pub fn as_key_slice(&self) -> KeySlice {
        Key(&self.0)
    }

    pub fn from_bytes(bytes: Bytes) -> KeyBytes {
        Key(bytes)
    }

    pub fn for_testing_from_bytes_zero_ts(bytes: Bytes) -> KeyBytes {
        Key(bytes)
    }

    pub fn for_testing_key_ref(&self) -> &[u8] {
        self.0.as_ref()
    }

    /// Always use `raw_ref` to access the key in week 1 + 2.
    pub fn raw_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl<'a> Key<&'a [u8]> {
    pub fn to_key_vec(self) -> KeyVec {
        Key(self.0.to_vec())
    }

    pub fn from_slice(slice: &'a [u8]) -> Self {
        Self(slice)
    }

    pub fn for_testing_key_ref(self) -> &'a [u8] {
        self.0.as_ref()
    }

    /// Always use `raw_ref` to access the key in week 1 + 2.
    pub fn raw_ref(self) -> &'a [u8] {
        self.0.as_ref()
    }
}

impl<T: AsRef<[u8]> + Debug> Debug for Key<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: AsRef<[u8]> + Default> Default for Key<T> {
    fn default() -> Self {
        Self(T::default())
    }
}

impl<T: AsRef<[u8]> + PartialEq> PartialEq for Key<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl<T: AsRef<[u8]> + Eq> Eq for Key<T> {}

impl<T: AsRef<[u8]> + Clone> Clone for Key<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: AsRef<[u8]> + Copy> Copy for Key<T> {}

impl<T: AsRef<[u8]> + PartialOrd> PartialOrd for Key<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl<T: AsRef<[u8]> + Ord> Ord for Key<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}
