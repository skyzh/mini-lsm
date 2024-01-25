#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use anyhow::Result;

use crate::{
    iterators::{merge_iterator::MergeIterator, StorageIterator},
    mem_table::MemTableIterator,
};

/// Represents the internal type for an LSM iterator. This type will be changed across the tutorial for multiple times.
type LsmIteratorInner = MergeIterator<MemTableIterator>;

pub struct LsmIterator {
    inner: LsmIteratorInner,
}

impl LsmIterator {
    pub(crate) fn new(iter: LsmIteratorInner) -> Result<Self> {
        Ok(Self { inner: iter })
    }
}

impl StorageIterator for LsmIterator {
    type KeyType<'a> = &'a [u8];

    fn is_valid(&self) -> bool {
        unimplemented!()
    }

    fn key(&self) -> &[u8] {
        unimplemented!()
    }

    fn value(&self) -> &[u8] {
        unimplemented!()
    }

    fn next(&mut self) -> Result<()> {
        unimplemented!()
    }
}

/// A wrapper around existing iterator, will prevent users from calling `next` when the iterator is
/// invalid. If an iterator is already invalid, `next` does not do anything. If `next` returns an error,
/// `is_valid` should return false, and `next` should always return an error.
pub struct FusedIterator<I: StorageIterator> {
    iter: I,
}

impl<I: StorageIterator> FusedIterator<I> {
    pub fn new(iter: I) -> Self {
        Self { iter }
    }
}

impl<I: StorageIterator> StorageIterator for FusedIterator<I> {
    type KeyType<'a> = I::KeyType<'a> where Self: 'a;

    fn is_valid(&self) -> bool {
        unimplemented!()
    }

    fn key(&self) -> Self::KeyType<'_> {
        unimplemented!()
    }

    fn value(&self) -> &[u8] {
        unimplemented!()
    }

    fn next(&mut self) -> Result<()> {
        unimplemented!()
    }
}
