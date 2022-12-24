#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::ops::Bound;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use arc_swap::ArcSwap;
use bytes::Bytes;

use crate::block::Block;
use crate::lsm_iterator::{FusedIterator, LsmIterator};
use crate::mem_table::MemTable;
use crate::table::SsTable;

pub type BlockCache = moka::sync::Cache<(usize, usize), Arc<Block>>;

#[derive(Clone)]
pub struct LsmStorageInner {
    /// MemTables, from oldest to earliest.
    memtables: Vec<Arc<MemTable>>,
    /// L0 SsTables, from oldest to earliest.
    l0_sstables: Vec<Arc<SsTable>>,
}

impl LsmStorageInner {
    fn create() -> Self {
        Self {
            memtables: vec![Arc::new(MemTable::create())],
            l0_sstables: vec![],
        }
    }
}

/// The storage interface of the LSM tree.
pub struct LsmStorage {
    inner: ArcSwap<LsmStorageInner>,
}

impl LsmStorage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            inner: ArcSwap::from_pointee(LsmStorageInner::create()),
        })
    }

    /// Get a key from the storage. In day 7, this can be further optimized by using a bloom filter.
    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        unimplemented!()
    }

    /// Put a key-value pair into the storage by writing into the current memtable.
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        assert!(!value.is_empty(), "value cannot be empty");
        assert!(!key.is_empty(), "key cannot be empty");
        unimplemented!()
    }

    /// Remove a key from the storage by writing an empty value.
    pub fn delete(&self, _key: &[u8]) -> Result<()> {
        unimplemented!()
    }

    /// Persist data to disk.
    ///
    /// In day 3: flush the current memtable to disk as L0 SST.
    /// In day 6: call `fsync` on WAL.
    pub fn sync(&self) -> Result<()> {
        unimplemented!()
    }

    /// Create an iterator over a range of keys.
    pub fn scan(
        &self,
        _lower: Bound<&[u8]>,
        _upper: Bound<&[u8]>,
    ) -> Result<FusedIterator<LsmIterator>> {
        unimplemented!()
    }
}
