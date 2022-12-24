use std::ops::Bound;

use anyhow::Result;
use bytes::Bytes;
use crossbeam_skiplist::map::Entry;
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
        Self {
            map: SkipMap::new(),
        }
    }

    /// Get a value by key.
    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        let entry = self.map.get(key).map(|e| e.value().clone());
        Ok(entry)
    }

    /// Put a key-value pair into the mem-table.
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.map
            .insert(Bytes::copy_from_slice(key), Bytes::copy_from_slice(value));
        Ok(())
    }

    fn map_bound(bound: Bound<&[u8]>) -> Bound<Bytes> {
        match bound {
            Bound::Included(x) => Bound::Included(Bytes::copy_from_slice(x)),
            Bound::Excluded(x) => Bound::Excluded(Bytes::copy_from_slice(x)),
            Bound::Unbounded => Bound::Unbounded,
        }
    }

    /// Get an iterator over a range of keys.
    pub fn scan(&self, lower: Bound<&[u8]>, upper: Bound<&[u8]>) -> Result<MemTableIterator> {
        let iter = self
            .map
            .range((Self::map_bound(lower), Self::map_bound(upper)));
        Ok(MemTableIterator::new(iter))
    }

    /// Flush the mem-table to SSTable.
    pub fn flush(&self, builder: &mut SsTableBuilder) -> Result<()> {
        for entry in self.map.iter() {
            builder.add(&entry.key()[..], &entry.value()[..]);
        }
        Ok(())
    }
}

type SkipMapRangeIter<'a> =
    crossbeam_skiplist::map::Range<'a, Bytes, (Bound<Bytes>, Bound<Bytes>), Bytes, Bytes>;

/// An iterator over a range of `SkipMap`.
pub struct MemTableIterator<'a> {
    iter: SkipMapRangeIter<'a>,
    item: (Bytes, Bytes),
}

impl<'a> MemTableIterator<'a> {
    fn entry_to_item(entry: Option<Entry<'a, Bytes, Bytes>>) -> (Bytes, Bytes) {
        entry
            .map(|x| (x.key().clone(), x.value().clone()))
            .unwrap_or_else(|| (Bytes::from_static(&[]), Bytes::from_static(&[])))
    }

    fn new(mut iter: SkipMapRangeIter<'a>) -> Self {
        let entry = iter.next();

        Self {
            item: Self::entry_to_item(entry),
            iter,
        }
    }
}

impl StorageIterator for MemTableIterator<'_> {
    fn value(&self) -> &[u8] {
        &self.item.1[..]
    }

    fn key(&self) -> &[u8] {
        &self.item.0[..]
    }

    fn is_valid(&self) -> bool {
        !self.item.0.is_empty()
    }

    fn next(&mut self) -> Result<()> {
        let entry = self.iter.next();
        self.item = Self::entry_to_item(entry);
        Ok(())
    }
}

#[cfg(test)]
#[path = "mem_table_test.rs"]
mod tests;
