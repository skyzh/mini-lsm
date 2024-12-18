#![allow(dead_code)] // REMOVE THIS LINE after fully implementing this functionality

use std::collections::HashMap;
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use anyhow::Result;
use bytes::Bytes;
use parking_lot::{Mutex, MutexGuard, RwLock};

use crate::block::Block;
use crate::compact::{
    CompactionController, CompactionOptions, LeveledCompactionController, LeveledCompactionOptions,
    SimpleLeveledCompactionController, SimpleLeveledCompactionOptions, TieredCompactionController,
};

use crate::iterators::concat_iterator::SstConcatIterator;
use crate::iterators::merge_iterator::MergeIterator;
use crate::iterators::two_merge_iterator::TwoMergeIterator;
use crate::iterators::StorageIterator;
use crate::key::KeySlice;
use crate::lsm_iterator::{FusedIterator, LsmIterator};
use crate::manifest::Manifest;
use crate::mem_table::MemTable;
use crate::mvcc::LsmMvccInner;
use crate::table::{SsTable, SsTableBuilder, SsTableIterator};

pub type BlockCache = moka::sync::Cache<(usize, usize), Arc<Block>>;

/// Represents the state of the storage engine.
#[derive(Clone)]
pub struct LsmStorageState {
    /// The current memtable.
    pub memtable: Arc<MemTable>,
    /// Immutable memtables, from latest to earliest.
    pub imm_memtables: Vec<Arc<MemTable>>,
    /// L0 SSTs, from latest to earliest.
    pub l0_sstables: Vec<usize>,
    /// SsTables sorted by key range; L1 - L_max for leveled compaction, or tiers for tiered
    /// compaction.
    pub levels: Vec<(usize, Vec<usize>)>,
    /// SST objects.
    pub sstables: HashMap<usize, Arc<SsTable>>,
}

pub enum WriteBatchRecord<T: AsRef<[u8]>> {
    Put(T, T),
    Del(T),
}

impl LsmStorageState {
    fn create(options: &LsmStorageOptions) -> Self {
        let levels = match &options.compaction_options {
            CompactionOptions::Leveled(LeveledCompactionOptions { max_levels, .. })
            | CompactionOptions::Simple(SimpleLeveledCompactionOptions { max_levels, .. }) => (1
                ..=*max_levels)
                .map(|level| (level, Vec::new()))
                .collect::<Vec<_>>(),
            CompactionOptions::Tiered(_) => Vec::new(),
            CompactionOptions::NoCompaction => vec![(1, Vec::new())],
        };
        Self {
            memtable: Arc::new(MemTable::create(0)),
            imm_memtables: Vec::new(),
            l0_sstables: Vec::new(),
            levels,
            sstables: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LsmStorageOptions {
    // Block size in bytes
    pub block_size: usize,
    // SST size in bytes, also the approximate memtable capacity limit
    pub target_sst_size: usize,
    // Maximum number of memtables in memory, flush to L0 when exceeding this limit
    pub num_memtable_limit: usize,
    pub compaction_options: CompactionOptions,
    pub enable_wal: bool,
    pub serializable: bool,
}

impl LsmStorageOptions {
    pub fn default_for_week1_test() -> Self {
        Self {
            block_size: 4096,
            target_sst_size: 2 << 20,
            compaction_options: CompactionOptions::NoCompaction,
            enable_wal: false,
            num_memtable_limit: 50,
            serializable: false,
        }
    }

    pub fn default_for_week1_day6_test() -> Self {
        Self {
            block_size: 4096,
            target_sst_size: 2 << 20,
            compaction_options: CompactionOptions::NoCompaction,
            enable_wal: false,
            num_memtable_limit: 2,
            serializable: false,
        }
    }

    pub fn default_for_week2_test(compaction_options: CompactionOptions) -> Self {
        Self {
            block_size: 4096,
            target_sst_size: 1 << 20, // 1MB
            compaction_options,
            enable_wal: false,
            num_memtable_limit: 2,
            serializable: false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum CompactionFilter {
    Prefix(Bytes),
}

/// The storage interface of the LSM tree.
pub(crate) struct LsmStorageInner {
    pub(crate) state: Arc<RwLock<Arc<LsmStorageState>>>,
    pub(crate) state_lock: Mutex<()>,
    path: PathBuf,
    pub(crate) block_cache: Arc<BlockCache>,
    next_sst_id: AtomicUsize,
    pub(crate) options: Arc<LsmStorageOptions>,
    pub(crate) compaction_controller: CompactionController,
    pub(crate) manifest: Option<Manifest>,
    pub(crate) mvcc: Option<LsmMvccInner>,
    pub(crate) compaction_filters: Arc<Mutex<Vec<CompactionFilter>>>,
}

/// A thin wrapper for `LsmStorageInner` and the user interface for MiniLSM.
pub struct MiniLsm {
    pub(crate) inner: Arc<LsmStorageInner>,
    /// Notifies the L0 flush thread to stop working. (In week 1 day 6)
    flush_notifier: crossbeam_channel::Sender<()>,
    /// The handle for the flush thread. (In week 1 day 6)
    flush_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    /// Notifies the compaction thread to stop working. (In week 2)
    compaction_notifier: crossbeam_channel::Sender<()>,
    /// The handle for the compaction thread. (In week 2)
    compaction_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Drop for MiniLsm {
    fn drop(&mut self) {
        self.compaction_notifier.send(()).ok();
        self.flush_notifier.send(()).ok();
    }
}

impl MiniLsm {
    pub fn close(&self) -> Result<()> {
        let mut thread_guard = self.flush_thread.lock();

        self.flush_notifier.send(())?;

        if let Some(flush_thread) = thread_guard.take() {
            flush_thread
                .join()
                .map_err(|e| anyhow::anyhow!("{:?}", e))?;
        }

        Ok(())
    }

    /// Start the storage engine by either loading an existing directory or creating a new one if the directory does
    /// not exist.
    pub fn open(path: impl AsRef<Path>, options: LsmStorageOptions) -> Result<Arc<Self>> {
        let inner = Arc::new(LsmStorageInner::open(path, options)?);
        let (tx1, rx) = crossbeam_channel::unbounded();
        let compaction_thread = inner.spawn_compaction_thread(rx)?;
        let (tx2, rx) = crossbeam_channel::unbounded();
        let flush_thread = inner.spawn_flush_thread(rx)?;
        Ok(Arc::new(Self {
            inner,
            flush_notifier: tx2,
            flush_thread: Mutex::new(flush_thread),
            compaction_notifier: tx1,
            compaction_thread: Mutex::new(compaction_thread),
        }))
    }

    pub fn new_txn(&self) -> Result<()> {
        self.inner.new_txn()
    }

    pub fn write_batch<T: AsRef<[u8]>>(&self, batch: &[WriteBatchRecord<T>]) -> Result<()> {
        self.inner.write_batch(batch)
    }

    pub fn add_compaction_filter(&self, compaction_filter: CompactionFilter) {
        self.inner.add_compaction_filter(compaction_filter)
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

    pub fn sync(&self) -> Result<()> {
        self.inner.sync()
    }

    pub fn scan(
        &self,
        lower: Bound<&[u8]>,
        upper: Bound<&[u8]>,
    ) -> Result<FusedIterator<LsmIterator>> {
        self.inner.scan(lower, upper)
    }

    /// Only call this in test cases due to race conditions
    pub fn force_flush(&self) -> Result<()> {
        if !self.inner.state.read().memtable.is_empty() {
            self.inner
                .force_freeze_memtable(&self.inner.state_lock.lock())?;
        }
        if !self.inner.state.read().imm_memtables.is_empty() {
            self.inner.force_flush_next_imm_memtable()?;
        }
        Ok(())
    }

    pub fn force_full_compaction(&self) -> Result<()> {
        self.inner.force_full_compaction()
    }
}

impl LsmStorageInner {
    pub(crate) fn next_sst_id(&self) -> usize {
        self.next_sst_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Start the storage engine by either loading an existing directory or creating a new one if the directory does
    /// not exist.
    pub(crate) fn open(path: impl AsRef<Path>, options: LsmStorageOptions) -> Result<Self> {
        let path = path.as_ref();

        std::fs::create_dir_all(path)?;

        let state = LsmStorageState::create(&options);

        let compaction_controller = match &options.compaction_options {
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
        };

        let storage = Self {
            state: Arc::new(RwLock::new(Arc::new(state))),
            state_lock: Mutex::new(()),
            path: path.to_path_buf(),
            block_cache: Arc::new(BlockCache::new(1024)),
            next_sst_id: AtomicUsize::new(1),
            compaction_controller,
            manifest: None,
            options: options.into(),
            mvcc: None,
            compaction_filters: Arc::new(Mutex::new(Vec::new())),
        };

        Ok(storage)
    }

    pub fn sync(&self) -> Result<()> {
        unimplemented!()
    }

    pub fn add_compaction_filter(&self, compaction_filter: CompactionFilter) {
        let mut compaction_filters = self.compaction_filters.lock();
        compaction_filters.push(compaction_filter);
    }

    /// Get a key from the storage. In day 7, this can be further optimized by using a bloom filter.
    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        if let Some(value) = self.state.read().memtable.get(key) {
            return Ok(if value.is_empty() { None } else { Some(value) });
        }
        for imm_memtable in self.state.read().imm_memtables.clone() {
            if let Some(value) = imm_memtable.get(key) {
                return Ok(if value.is_empty() { None } else { Some(value) });
            }
        }

        let read_guard = self.state.read();
        let l0_sst_iters: Vec<_> = read_guard
            .l0_sstables
            .iter()
            .map(|sstable_key| read_guard.sstables.get(sstable_key).unwrap().clone())
            .filter(|sst| {
                let lt = sst.last_key().raw_ref() < key;
                let gt = sst.first_key().raw_ref() > key;
                if lt || gt {
                    return false;
                }
                if let Some(bloom) = &sst.bloom {
                    let may_contain = bloom.may_contain(farmhash::fingerprint32(key));
                    if may_contain {
                        return true;
                    }
                }
                true
            })
            .collect();

        let l1_sst: Vec<Arc<SsTable>> = read_guard
            .levels
            .iter()
            .filter(|level_pair| level_pair.0 == 0)
            .flat_map(|level_pair| level_pair.1.clone())
            .map(|sst_id| read_guard.sstables.get(&sst_id).unwrap().clone())
            .collect();

        drop(read_guard);

        let l0_sst_iters: Result<Vec<_>> = l0_sst_iters
            .into_iter()
            .map(|sst| SsTableIterator::create_and_seek_to_key(sst, KeySlice::from_slice(key)))
            .collect();

        let sst_iters = TwoMergeIterator::create(
            MergeIterator::create(l0_sst_iters?.into_iter().map(Box::new).collect()),
            SstConcatIterator::create_and_seek_to_key(l1_sst, KeySlice::from_slice(key))?,
        )?;

        if key == sst_iters.key().raw_ref() && !sst_iters.value().is_empty() {
            return Ok(Some(Bytes::from(sst_iters.value().to_vec())));
        }

        Ok(None)
    }

    /// Write a batch of data into the storage. Implement in week 2 day 7.
    pub fn write_batch<T: AsRef<[u8]>>(&self, _batch: &[WriteBatchRecord<T>]) -> Result<()> {
        unimplemented!()
    }

    /// Put a key-value pair into the storage by writing into the current memtable.
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let size;
        {
            let read_guard = self.state.read();
            read_guard.memtable.put(key, value)?;
            size = read_guard.memtable.approximate_size();
        }
        if size >= self.options.target_sst_size {
            let lock = self.state_lock.lock();
            let size = self.state.read().memtable.approximate_size();
            if size >= self.options.target_sst_size {
                self.force_freeze_memtable(&lock)?;
            }
        }
        Ok(())
    }

    /// Remove a key from the storage by writing an empty value.
    pub fn delete(&self, key: &[u8]) -> Result<()> {
        self.put(key, &[])
    }

    pub(crate) fn path_of_sst_static(path: impl AsRef<Path>, id: usize) -> PathBuf {
        path.as_ref().join(format!("{:05}.sst", id))
    }

    pub(crate) fn path_of_sst(&self, id: usize) -> PathBuf {
        Self::path_of_sst_static(&self.path, id)
    }

    pub(crate) fn path_of_wal_static(path: impl AsRef<Path>, id: usize) -> PathBuf {
        path.as_ref().join(format!("{:05}.wal", id))
    }

    pub(crate) fn path_of_wal(&self, id: usize) -> PathBuf {
        Self::path_of_wal_static(&self.path, id)
    }

    pub(super) fn sync_dir(&self) -> Result<()> {
        unimplemented!()
    }

    /// Force freeze the current memtable to an immutable memtable
    pub fn force_freeze_memtable(&self, _state_lock_observer: &MutexGuard<'_, ()>) -> Result<()> {
        let new_memtable = Arc::new(MemTable::create(self.next_sst_id()));
        let mut new_state = self.state.read().as_ref().clone();

        let old_memtable = std::mem::replace(&mut new_state.memtable, new_memtable);
        new_state.imm_memtables.insert(0, old_memtable.clone());

        {
            let mut write_guard = self.state.write();
            *write_guard = Arc::new(new_state);
        }

        Ok(())
    }

    /// Force flush the earliest-created immutable memtable to disk
    pub fn force_flush_next_imm_memtable(&self) -> Result<()> {
        let _state_guard = self.state_lock.lock();

        // Read last imm_memtable
        let last_memtable = self
            .state
            .read()
            .imm_memtables
            .last()
            .expect("no more immutable memtables")
            .clone();
        // Build new sst
        let mut builder = SsTableBuilder::new(self.options.block_size);
        last_memtable.flush(&mut builder)?;
        let id = last_memtable.id();
        let sst = Arc::new(builder.build(
            id,
            Some(self.block_cache.clone()),
            self.path_of_sst(id).clone(),
        )?);

        // Flush sst to l0_sstables
        {
            let mut write_guard = self.state.write();
            let mut write_ref = write_guard.as_ref().clone();

            write_ref
                .imm_memtables
                .pop()
                .expect("no more immutable memtables to pop");
            write_ref.l0_sstables.insert(0, id);
            write_ref.sstables.insert(id, sst);

            *write_guard = Arc::new(write_ref);
        }
        Ok(())
    }

    pub fn new_txn(&self) -> Result<()> {
        // no-op
        Ok(())
    }

    /// Create an iterator over a range of keys.
    pub fn scan(
        &self,
        lower: Bound<&[u8]>,
        upper: Bound<&[u8]>,
    ) -> Result<FusedIterator<LsmIterator>> {
        let read_guard = self.state.read();
        let iter = read_guard.memtable.scan(lower, upper);
        let mut iters: Vec<_> = read_guard
            .imm_memtables
            .iter()
            .map(|memtable| memtable.scan(lower, upper))
            .collect();
        iters.insert(0, iter);

        let l0_sst_iters: Vec<_> = read_guard
            .l0_sstables
            .iter()
            .map(|sstable_key| read_guard.sstables.get(sstable_key).unwrap().clone())
            .filter(|sst| {
                let lt = match lower {
                    Bound::Included(key) => sst.last_key().raw_ref() < key,
                    Bound::Excluded(key) => sst.last_key().raw_ref() <= key,
                    _ => false,
                };
                let gt = match upper {
                    Bound::Included(key) => sst.first_key().raw_ref() > key,
                    Bound::Excluded(key) => sst.first_key().raw_ref() >= key,
                    _ => false,
                };
                !lt && !gt
            })
            .collect();

        let l1_sst: Vec<Arc<SsTable>> = read_guard
            .levels
            .iter()
            .filter(|level_pair| level_pair.0 == 0)
            .flat_map(|level_pair| level_pair.1.clone())
            .map(|sst_id| read_guard.sstables.get(&sst_id).unwrap().clone())
            .collect();

        drop(read_guard);

        let l0_sst_iters: Result<Vec<_>> = l0_sst_iters
            .into_iter()
            .map(|sst| match lower {
                Bound::Included(key) => {
                    SsTableIterator::create_and_seek_to_key(sst, KeySlice::from_slice(key))
                }
                Bound::Excluded(key) => {
                    let mut iter =
                        SsTableIterator::create_and_seek_to_key(sst, KeySlice::from_slice(key))?;
                    if iter.is_valid() && iter.key().raw_ref() == key {
                        iter.next()?;
                    }
                    Ok(iter)
                }
                Bound::Unbounded => SsTableIterator::create_and_seek_to_first(sst),
            })
            .collect();

        Ok(FusedIterator::new(LsmIterator::new(
            TwoMergeIterator::create(
                MergeIterator::create(iters.into_iter().map(Box::new).collect()),
                TwoMergeIterator::create(
                    MergeIterator::create(l0_sst_iters?.into_iter().map(Box::new).collect()),
                    SstConcatIterator::create_and_seek_to_first(l1_sst)?,
                )?,
            )?,
            match upper {
                Bound::Included(key) => Bound::Included(Bytes::copy_from_slice(key)),
                Bound::Excluded(key) => Bound::Excluded(Bytes::copy_from_slice(key)),
                Bound::Unbounded => Bound::Unbounded,
            },
        )?))
    }
}
