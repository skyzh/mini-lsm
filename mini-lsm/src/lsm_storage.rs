use std::ops::Bound;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use arc_swap::ArcSwap;
use bytes::Bytes;
use parking_lot::Mutex;

use crate::iterators::impls::StorageIterator;
use crate::iterators::merge_iterator::MergeIterator;
use crate::iterators::two_merge_iterator::TwoMergeIterator;
use crate::lsm_iterator::{FusedIterator, LsmIterator};
use crate::mem_table::{map_bound, MemTable};
use crate::table::{SsTable, SsTableBuilder, SsTableIterator};

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
    flush_lock: Mutex<()>,
}

impl LsmStorage {
    pub fn open(_path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            inner: ArcSwap::from_pointee(LsmStorageInner::create()),
            flush_lock: Mutex::new(()),
        })
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        let snapshot = self.inner.load();
        for memtable in snapshot.memtables.iter().rev() {
            if let Some(value) = memtable.get(key)? {
                if value.is_empty() {
                    // found tomestone, return key not exists
                    return Ok(None);
                }
                return Ok(Some(value));
            }
        }
        let mut iters = Vec::new();
        iters.reserve(snapshot.l0_sstables.len());
        for table in snapshot.l0_sstables.iter().rev() {
            iters.push(Box::new(SsTableIterator::create_and_seek_to_key(
                table.clone(),
                key,
            )?));
        }
        let iter = MergeIterator::create(iters);
        if iter.is_valid() {
            return Ok(Some(Bytes::copy_from_slice(iter.value())));
        }
        Ok(None)
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        assert!(!value.is_empty(), "value cannot be empty");
        assert!(!key.is_empty(), "key cannot be empty");
        let snapshot = self.inner.load();
        snapshot.memtables[0].put(key, value)?;
        Ok(())
    }

    pub fn delete(&self, key: &[u8]) -> Result<()> {
        let snapshot = self.inner.load();
        snapshot.memtables[0].put(key, b"")?;
        Ok(())
    }

    pub fn sync(&self) -> Result<()> {
        let _flush_lock = self.flush_lock.lock();
        let mut snapshot = {
            let snapshot = self.inner.load();
            snapshot.as_ref().clone()
        };

        let mut builder = SsTableBuilder::new(4096);
        let memtable = snapshot.memtables.pop().unwrap();
        assert!(snapshot.memtables.is_empty());
        memtable.flush(&mut builder)?;
        snapshot.l0_sstables.push(Arc::new(builder.build("")?));
        self.inner.store(Arc::new(snapshot));
        Ok(())
    }

    pub fn scan(
        &self,
        lower: Bound<&[u8]>,
        upper: Bound<&[u8]>,
    ) -> Result<FusedIterator<LsmIterator>> {
        let snapshot = self.inner.load();

        let mut memtable_iters = Vec::new();
        memtable_iters.reserve(snapshot.memtables.len());
        for memtable in snapshot.memtables.iter().rev() {
            memtable_iters.push(Box::new(memtable.scan(lower, upper)?));
        }
        let memtable_iter = MergeIterator::create(memtable_iters);

        let mut table_iters = Vec::new();
        table_iters.reserve(snapshot.l0_sstables.len());
        for table in snapshot.l0_sstables.iter().rev() {
            let iter = match lower {
                Bound::Included(key) => {
                    SsTableIterator::create_and_seek_to_key(table.clone(), key)?
                }
                Bound::Excluded(key) => {
                    let mut iter = SsTableIterator::create_and_seek_to_key(table.clone(), key)?;
                    if iter.is_valid() && iter.key() == key {
                        iter.next()?;
                    }
                    iter
                }
                Bound::Unbounded => SsTableIterator::create_and_seek_to_first(table.clone())?,
            };

            table_iters.push(Box::new(iter));
        }
        let table_iter = MergeIterator::create(table_iters);

        let iter = TwoMergeIterator::create(memtable_iter, table_iter)?;

        Ok(FusedIterator::new(LsmIterator::new(
            iter,
            map_bound(upper),
        )?))
    }
}
