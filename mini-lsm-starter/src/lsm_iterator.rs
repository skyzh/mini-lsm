#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use crate::{
    iterators::{merge_iterator::MergeIterator, StorageIterator},
    mem_table::MemTableIterator,
};
use anyhow::Result;
use bytes::Bytes;

/// Represents the internal type for an LSM iterator. This type will be changed across the tutorial for multiple times.
type LsmIteratorInner = MergeIterator<MemTableIterator>;

pub struct LsmIterator {
    inner: LsmIteratorInner,
}

// LsmIterator is not supposed to return a deleted key.
// MergedTableIterator can return a deleted key at start.
impl LsmIterator {
    pub(crate) fn new(iter: LsmIteratorInner) -> Result<Self> {
        Ok(Self { inner: iter })
    }
}

impl StorageIterator for LsmIterator {
    type KeyType<'a> = &'a [u8];

    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }

    fn key(&self) -> &[u8] {
        if !self.is_valid() {
            panic!("invalid key")
        }
        self.inner.key().raw_ref()
    }

    fn value(&self) -> &[u8] {
        if !self.is_valid() {
            panic!("invalid key")
        }
        self.inner.value()
    }

    fn next(&mut self) -> Result<()> {
        self.inner.next()?;
        let k = Bytes::copy_from_slice(self.inner.key().raw_ref());
        let v = self.inner.value();
        let mut is_deleted = self.inner.value().is_empty();
        while is_deleted && self.is_valid() {
            self.inner.next()?;
            is_deleted = self.inner.value().is_empty();
        }
        Ok(())
    }
}

/// A wrapper around existing iterator, will prevent users from calling `next` when the iterator is
/// invalid. If an iterator is already invalid, `next` does not do anything. If `next` returns an error,
/// `is_valid` should return false, and `next` should always return an error.
pub struct FusedIterator<I: StorageIterator> {
    iter: I,
    has_errored: bool,
}

impl<I: StorageIterator> FusedIterator<I> {
    pub fn new(iter: I) -> Self {
        Self {
            iter,
            has_errored: false,
        }
    }
}

impl<I: StorageIterator> StorageIterator for FusedIterator<I> {
    type KeyType<'a> = I::KeyType<'a>
    where
        Self: 'a;

    fn is_valid(&self) -> bool {
        if self.has_errored {
            return false;
        }
        self.iter.is_valid()
    }

    fn key(&self) -> Self::KeyType<'_> {
        self.iter.key()
    }

    fn value(&self) -> &[u8] {
        self.iter.value()
    }

    fn next(&mut self) -> Result<()> {
        if self.has_errored {
            return Err(anyhow::anyhow!("iterator errored"));
        }

        if let e @ Err(_) = self.iter.next() {
            self.has_errored = true;
            return e;
        }
        Ok(())
    }
}
