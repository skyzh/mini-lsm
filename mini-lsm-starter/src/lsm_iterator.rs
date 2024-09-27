#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::ops::Bound;

use anyhow::Result;
use bytes::Bytes;

use crate::{
    iterators::{
        merge_iterator::MergeIterator, two_merge_iterator::TwoMergeIterator, StorageIterator,
    },
    mem_table::MemTableIterator,
    table::SsTableIterator,
};

/// Represents the internal type for an LSM iterator. This type will be changed across the tutorial for multiple times.
type LsmIteratorInner =
    TwoMergeIterator<MergeIterator<MemTableIterator>, MergeIterator<SsTableIterator>>;

pub struct LsmIterator {
    inner: LsmIteratorInner,
    upper_bound: Bound<Bytes>,
    out_of_bound: bool,
}

impl LsmIterator {
    pub(crate) fn new(iter: LsmIteratorInner) -> Result<Self> {
        let mut iter = Self {
            inner: iter,
            upper_bound: Bound::Unbounded,
            out_of_bound: false,
        };

        iter.skip_empty_value()?;
        Ok(iter)
    }

    pub(crate) fn skip_empty_value(&mut self) -> Result<()> {
        while self.inner.value().is_empty() && self.inner.is_valid() {
            self.inner.next()?;
        }
        Ok(())
    }
}

impl StorageIterator for LsmIterator {
    type KeyType<'a> = &'a [u8];

    fn is_valid(&self) -> bool {
        !self.out_of_bound && self.inner.is_valid()
    }

    fn key(&self) -> &[u8] {
        if self.out_of_bound {
            return &[];
        }
        self.inner.key().raw_ref()
    }

    fn value(&self) -> &[u8] {
        if self.out_of_bound {
            return &[];
        }
        self.inner.value()
    }

    fn next(&mut self) -> Result<()> {
        if self.out_of_bound {
            return Ok(());
        }
        self.inner.next()?;
        match &self.upper_bound {
            Bound::Unbounded => {}
            Bound::Included(key) => {
                if self.inner.key().raw_ref() > &key[..] {
                    self.out_of_bound = true
                }
            }
            Bound::Excluded(key) => {
                if self.inner.key().raw_ref() >= &key[..] {
                    self.out_of_bound = true
                }
            }
        }
        self.skip_empty_value()
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
    type KeyType<'a> = I::KeyType<'a> where Self: 'a;

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
            return Err(anyhow::anyhow!("iterator has errored"));
        }
        if !self.is_valid() {
            return Ok(());
        }
        if let e @ Err(_) = self.iter.next() {
            self.has_errored = true;
            return e;
        }
        Ok(())
    }
}
