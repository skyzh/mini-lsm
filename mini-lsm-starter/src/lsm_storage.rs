use std::ops::Bound;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use arc_swap::ArcSwap;
use bytes::Bytes;

use crate::lsm_iterator::LsmIterator;
use crate::mem_table::MemTable;
use crate::table::{SsTable, SsTableIterator};

pub struct LsmStorageInner {
    memtables: Vec<Arc<MemTable>>,
    sstables: Vec<Arc<SsTable>>,
}

impl LsmStorageInner {
    fn create() -> Self {
        Self {
            memtables: vec![Arc::new(MemTable::create())],
            sstables: vec![],
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
        let snapshot = self.inner.load();
        for memtable in &snapshot.memtables {
            if let Some(value) = memtable.get(key)? {
                if value.is_empty() {
                    // found tomestone, return key not exists
                    return Ok(None);
                }
                return Ok(Some(value));
            }
        }
        let mut iters = Vec::new();
        iters.reserve(snapshot.sstables.len());
        for table in snapshot.sstables.iter().rev() {
            iters.push(SsTableIterator::create_and_seek_to_key(table.clone(), key)?);
        }
        Ok(None)
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

    pub fn scan(&self, _lower: Bound<&[u8]>, _upper: Bound<&[u8]>) -> Result<LsmIterator> {
        unimplemented!()
    }
}
