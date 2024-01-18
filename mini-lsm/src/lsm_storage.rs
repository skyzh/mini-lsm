use std::collections::HashMap;
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use anyhow::Result;
use bytes::Bytes;
use parking_lot::{Mutex, RwLock};

use crate::block::Block;
use crate::compact::{
    CompactionController, CompactionOptions, LeveledCompactionController, LeveledCompactionOptions,
    SimpleLeveledCompactionController, SimpleLeveledCompactionOptions, TieredCompactionController,
};
use crate::iterators::merge_iterator::MergeIterator;
use crate::iterators::two_merge_iterator::TwoMergeIterator;
use crate::iterators::StorageIterator;
use crate::lsm_iterator::{FusedIterator, LsmIterator};
use crate::mem_table::{map_bound, MemTable};
use crate::table::{SsTable, SsTableBuilder, SsTableIterator};

pub type BlockCache = moka::sync::Cache<(usize, usize), Arc<Block>>;

#[derive(Clone)]
pub struct LsmStorageState {
    /// The current memtable.
    pub memtable: Arc<MemTable>,
    /// Immutable memtables, from earliest to latest.
    pub imm_memtables: Vec<Arc<MemTable>>,
    /// L0 SSTs, from earliest to latest.
    pub l0_sstables: Vec<usize>,
    /// SsTables sorted by key range; L1 - L_max for leveled compaction, or tiers for tiered
    /// compaction.
    pub levels: Vec<(usize, Vec<usize>)>,
    /// SST objects.
    pub sstables: HashMap<usize, Arc<SsTable>>,
}

impl LsmStorageState {
    fn create(options: &LsmStorageOptions) -> Self {
        match &options.compaction_options {
            CompactionOptions::Leveled(LeveledCompactionOptions { max_levels, .. })
            | CompactionOptions::Simple(SimpleLeveledCompactionOptions { max_levels, .. }) => {
                Self {
                    memtable: Arc::new(MemTable::create()),
                    imm_memtables: Vec::new(),
                    l0_sstables: Vec::new(),
                    levels: (1..=*max_levels)
                        .map(|level| (level, Vec::new()))
                        .collect::<Vec<_>>(),
                    sstables: Default::default(),
                }
            }
            CompactionOptions::Tiered(_) | CompactionOptions::NoCompaction => Self {
                memtable: Arc::new(MemTable::create()),
                imm_memtables: Vec::new(),
                l0_sstables: Vec::new(),
                levels: Vec::new(),
                sstables: Default::default(),
            },
        }
    }
}

pub struct LsmStorageOptions {
    block_size: usize,
    target_sst_size: usize,
    compaction_options: CompactionOptions,
}

impl LsmStorageOptions {
    pub fn default_for_week1_test() -> Self {
        Self {
            block_size: 4096,
            target_sst_size: 2 << 20,
            compaction_options: CompactionOptions::NoCompaction,
        }
    }
}

/// The storage interface of the LSM tree.
pub(crate) struct LsmStorageInner {
    pub(crate) state: Arc<RwLock<Arc<LsmStorageState>>>,
    state_lock: Mutex<()>,
    path: PathBuf,
    pub(crate) block_cache: Arc<BlockCache>,
    next_sst_id: AtomicUsize,
    options: Arc<LsmStorageOptions>,
    compaction_controller: CompactionController,
}

pub struct MiniLsm {
    inner: Arc<LsmStorageInner>,
    compaction_notifier: std::sync::mpsc::Sender<()>,
    compaction_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Drop for MiniLsm {
    fn drop(&mut self) {
        self.compaction_notifier.send(()).ok();
    }
}

impl MiniLsm {
    pub fn close(&self) -> Result<()> {
        self.compaction_notifier.send(()).ok();
        let mut compaction_thread = self.compaction_thread.lock();
        if let Some(mut compaction_thread) = compaction_thread.take() {
            compaction_thread
                .join()
                .map_err(|e| anyhow::anyhow!("{:?}", e))?;
        }
        Ok(())
    }

    pub fn open(path: impl AsRef<Path>, options: LsmStorageOptions) -> Result<Arc<Self>> {
        let inner = Arc::new(LsmStorageInner::open(path, options)?);
        let (tx, rx) = std::sync::mpsc::channel();
        let compaction_thread = inner.spawn_compaction_thread(rx)?;
        Ok(Arc::new(Self {
            inner,
            compaction_notifier: tx,
            compaction_thread: Mutex::new(compaction_thread),
        }))
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.inner.get(key)
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.inner.put(key, value)
    }

    pub fn delete(&self, key: &[u8]) -> Result<()> {
        self.inner.delete(key)
    }

    pub fn scan(
        &self,
        lower: Bound<&[u8]>,
        upper: Bound<&[u8]>,
    ) -> Result<FusedIterator<LsmIterator>> {
        self.inner.scan(lower, upper)
    }
}

impl LsmStorageInner {
    pub(crate) fn next_sst_id(&self) -> usize {
        self.next_sst_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    pub(crate) fn open(path: impl AsRef<Path>, options: LsmStorageOptions) -> Result<Self> {
        Ok(Self {
            state: Arc::new(RwLock::new(Arc::new(LsmStorageState::create(&options)))),
            state_lock: Mutex::new(()),
            path: path.as_ref().to_path_buf(),
            block_cache: Arc::new(BlockCache::new(1 << 20)), // 4GB block cache,
            next_sst_id: AtomicUsize::new(1),
            compaction_controller: match &options.compaction_options {
                CompactionOptions::Leveled(options) => {
                    CompactionController::Leveled(LeveledCompactionController::new(options.clone()))
                }
                CompactionOptions::Tiered(options) => {
                    CompactionController::Tiered(TieredCompactionController::new(options.clone()))
                }
                CompactionOptions::Simple(options) => CompactionController::Simple(
                    SimpleLeveledCompactionController::new(options.clone()),
                ),
                CompactionOptions::NoCompaction => CompactionController::NoCompaction,
            },
            options: options.into(),
        })
    }

    /// Get a key from the storage. In day 7, this can be further optimized by using a bloom filter.
    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        let snapshot = {
            let guard = self.state.read();
            Arc::clone(&guard)
        }; // drop global lock here

        // Search on the current memtable.
        if let Some(value) = snapshot.memtable.get(key) {
            if value.is_empty() {
                // found tomestone, return key not exists
                return Ok(None);
            }
            return Ok(Some(value));
        }
        // Search on immutable memtables.
        for memtable in snapshot.imm_memtables.iter().rev() {
            if let Some(value) = memtable.get(key) {
                if value.is_empty() {
                    // found tomestone, return key not exists
                    return Ok(None);
                }
                return Ok(Some(value));
            }
        }
        let mut iters = Vec::with_capacity(snapshot.l0_sstables.len());
        for table in snapshot.l0_sstables.iter().rev() {
            iters.push(Box::new(SsTableIterator::create_and_seek_to_key(
                snapshot.sstables[table].clone(),
                key,
            )?));
        }
        let iter = MergeIterator::create(iters);
        if iter.is_valid() {
            return Ok(Some(Bytes::copy_from_slice(iter.value())));
        }
        Ok(None)
    }

    /// Put a key-value pair into the storage by writing into the current memtable.
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        assert!(!value.is_empty(), "value cannot be empty");
        assert!(!key.is_empty(), "key cannot be empty");

        let guard = self.state.read();
        guard.memtable.put(key, value);

        Ok(())
    }

    /// Remove a key from the storage by writing an empty value.
    pub fn delete(&self, key: &[u8]) -> Result<()> {
        assert!(!key.is_empty(), "key cannot be empty");

        let guard = self.state.read();
        guard.memtable.put(key, b"");

        Ok(())
    }

    pub(crate) fn path_of_sst(&self, id: usize) -> PathBuf {
        self.path.join(format!("{:05}.sst", id))
    }

    /// Force freeze the current memetable to an immutable memtable
    pub fn force_freeze_memtable(&self) -> Result<()> {
        Ok(())
    }

    /// Force flush the all immutable memtables to disk
    pub fn force_flush_imm_memtables(&self) -> Result<()> {
        let _flush_lock = self.state_lock.lock();

        let flush_memtable;
        let sst_id;

        // Move mutable memtable to immutable memtables.
        {
            let mut guard = self.state.write();
            // Swap the current memtable with a new one.
            let mut snapshot = guard.as_ref().clone();
            let memtable = std::mem::replace(&mut snapshot.memtable, Arc::new(MemTable::create()));
            flush_memtable = memtable.clone();
            sst_id = self.next_sst_id();
            // Add the memtable to the immutable memtables.
            snapshot.imm_memtables.push(memtable);
            // Update the snapshot.
            *guard = Arc::new(snapshot);
        }

        // At this point, the old memtable should be disabled for write, and all write threads
        // should be operating on the new memtable. We can safely flush the old memtable to
        // disk.

        let mut builder = SsTableBuilder::new(4096);
        flush_memtable.flush(&mut builder)?;
        let sst = Arc::new(builder.build(
            sst_id,
            Some(self.block_cache.clone()),
            self.path_of_sst(sst_id),
        )?);

        // Add the flushed L0 table to the list.
        {
            let mut guard = self.state.write();
            let mut snapshot = guard.as_ref().clone();
            // Remove the memtable from the immutable memtables.
            snapshot.imm_memtables.pop();
            // Add L0 table
            if self.compaction_controller.flush_to_l0() {
                // In leveled compaction or no compaction, simply flush to L0
                snapshot.l0_sstables.push(sst_id);
            } else {
                // In tiered compaction, create a new tier
                snapshot.levels.insert(0, (sst_id, vec![sst_id]));
            }
            snapshot.sstables.insert(sst_id, sst);
            // Update the snapshot.
            *guard = Arc::new(snapshot);
        }

        Ok(())
    }

    /// Create an iterator over a range of keys.
    pub fn scan(
        &self,
        lower: Bound<&[u8]>,
        upper: Bound<&[u8]>,
    ) -> Result<FusedIterator<LsmIterator>> {
        let snapshot = {
            let guard = self.state.read();
            Arc::clone(&guard)
        }; // drop global lock here

        let mut memtable_iters = Vec::with_capacity(snapshot.imm_memtables.len() + 1);
        memtable_iters.push(Box::new(snapshot.memtable.scan(lower, upper)));
        for memtable in snapshot.imm_memtables.iter().rev() {
            memtable_iters.push(Box::new(memtable.scan(lower, upper)));
        }
        let memtable_iter = MergeIterator::create(memtable_iters);

        let mut table_iters = Vec::with_capacity(snapshot.l0_sstables.len());
        for table_id in snapshot.l0_sstables.iter().rev() {
            let table = snapshot.sstables[table_id].clone();
            let iter = match lower {
                Bound::Included(key) => SsTableIterator::create_and_seek_to_key(table, key)?,
                Bound::Excluded(key) => {
                    let mut iter = SsTableIterator::create_and_seek_to_key(table, key)?;
                    if iter.is_valid() && iter.key() == key {
                        iter.next()?;
                    }
                    iter
                }
                Bound::Unbounded => SsTableIterator::create_and_seek_to_first(table)?,
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
