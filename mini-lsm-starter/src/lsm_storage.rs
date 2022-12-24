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
    pub fn open(_path: &Path) -> Result<Self> {
        Ok(Self {
            inner: ArcSwap::from_pointee(LsmStorageInner::create()),
        })
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        unimplemented!()
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        assert!(!value.is_empty(), "value cannot be empty");
        assert!(!key.is_empty(), "key cannot be empty");
        unimplemented!()
    }

    pub fn delete(&mut self, _key: &[u8]) -> Result<()> {
        unimplemented!()
    }

    pub fn sync(&mut self) -> Result<()> {
        unimplemented!()
    }

    pub fn scan(
        &self,
        _lower: Bound<&[u8]>,
        _upper: Bound<&[u8]>,
    ) -> Result<FusedIterator<LsmIterator>> {
        unimplemented!()
    }
}
