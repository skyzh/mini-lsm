#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::ops::Bound;

use anyhow::Result;
use bytes::Bytes;
use crossbeam_skiplist::SkipMap;

use crate::iterators::impls::StorageIterator;
use crate::table::SsTableBuilder;

/// A basic mem-table based on crossbeam-skiplist
pub struct MemTable {
    map: SkipMap<Bytes, Bytes>,
}

impl MemTable {
    /// Create a new mem-table.
    pub fn create() -> Self {
        unimplemented!()
    }

    /// Get a value by key.
    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        unimplemented!()
    }

    /// Put a key-value pair into the mem-table.
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        unimplemented!()
    }

    /// Get an iterator over a range of keys.
    pub fn scan(&self, lower: Bound<&[u8]>, upper: Bound<&[u8]>) -> Result<MemTableIterator> {
        unimplemented!()
    }

    /// Flush the mem-table to SSTable.
    pub fn flush(&self, builder: &mut SsTableBuilder) -> Result<()> {
        unimplemented!()
    }
}

type SkipMapRangeIter<'a> =
    crossbeam_skiplist::map::Range<'a, Bytes, (Bound<Bytes>, Bound<Bytes>), Bytes, Bytes>;

/// An iterator over a range of `SkipMap`.
pub struct MemTableIterator<'a> {
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> MemTableIterator<'a> {
    fn new(iter: SkipMapRangeIter<'a>) -> Self {
        unimplemented!()
    }
}

impl StorageIterator for MemTableIterator<'_> {
    fn value(&self) -> &[u8] {
        unimplemented!()
    }

    fn key(&self) -> &[u8] {
        unimplemented!()
    }

    fn is_valid(&self) -> bool {
        unimplemented!()
    }

    fn next(&mut self) -> Result<()> {
        unimplemented!()
    }
}

#[cfg(test)]
#[path = "mem_table_test.rs"]
mod tests;
