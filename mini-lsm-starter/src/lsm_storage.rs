#![allow(dead_code)] // REMOVE THIS LINE after fully implementing this functionality

use anyhow::{Context, anyhow};
use anyhow::{Ok, Result};
use bytes::Bytes;
use parking_lot::{Mutex, MutexGuard, RwLock};
use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File};
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use crate::block::Block;
use crate::compact::{
    CompactionController, CompactionOptions, LeveledCompactionController, LeveledCompactionOptions,
    SimpleLeveledCompactionController, SimpleLeveledCompactionOptions, TieredCompactionController,
};
use crate::iterators::StorageIterator;
use crate::iterators::concat_iterator::SstConcatIterator;
use crate::iterators::merge_iterator::MergeIterator;
use crate::iterators::two_merge_iterator::TwoMergeIterator;
use crate::key::KeySlice;
use crate::lsm_iterator::{FusedIterator, LsmIterator};
use crate::manifest::{Manifest, ManifestRecord};
use crate::mem_table::{self, MemTable};
use crate::mvcc::LsmMvccInner;
use crate::table::{FileObject, SsTable, SsTableBuilder, SsTableIterator};

// TODO: try this one https://github.com/cloudflare/pingora/tree/main/tinyufo with bech later
pub type BlockCache = moka::sync::Cache<(usize, usize), Arc<Block>>;

/// Represents the state of the storage engine.
#[derive(Clone)]
pub struct LsmStorageState {
    /// The current memtable. the memtable here do not need lock portection, since it is a crossbeam_skiplist::SkipMap
    /// if only operate the memtable, lock could be released as soon as possible
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
    /// the state behind Arc is read only, modify is done by replace with a new one,
    /// so read will get a snapshot, only the memtable in the snapshot will see the latest change with skipmap support
    pub(crate) state: Arc<RwLock<Arc<LsmStorageState>>>,
    // with the separete state_lock instead of rwlock only, the state can still be accessed while the state_lock is locked,
    // but with rwlock, that is impossible.
    // so the state_lock is only used in backgroud tasks, for example, like compaction, flush to imm_memtables, flush to l0,
    // so the foreground tasks are not blocked
    // kind of similar to https://twitter.com/MarkCallaghanDB/status/1574425353564475394
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
        self.flush_notifier.send(()).ok();
        if let Some(f) = self.flush_thread.lock().take() {
            f.join().map_err(|e| anyhow!("{:?}", e))?;
        }
        self.compaction_notifier.send(()).ok();
        if let Some(f) = self.compaction_thread.lock().take() {
            f.join().map_err(|e| anyhow!("{:?}", e))?;
        }
        if self.inner.options.enable_wal {
            self.inner.sync()?;
            self.inner.sync_dir()?;

            return Ok(());
        }

        // flush memtable to imm_memtable
        self.inner
            .force_freeze_with_new_memtable(MemTable::create(self.inner.next_sst_id()))?;

        // flush all imm_memtable to disk
        while !self.inner.state.read().imm_memtables.is_empty() {
            self.inner.force_flush_next_imm_memtable()?;
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
        let mut state = LsmStorageState::create(&options);
        // seems the cache is not cleaned forever ? just let lru do the gc job.
        // better refill the cache somehow after compaction
        let block_cache = Arc::new(BlockCache::new(1024));
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

        let path = path.as_ref();
        if !path.exists() {
            fs::create_dir_all(path)?;
        }

        let mut max_id = state.memtable.id();
        let manifest_path = path.join("MANIFEST");
        let manifest = if !manifest_path.exists() {
            if options.enable_wal {
                state.memtable = Arc::new(MemTable::create_with_wal(
                    state.memtable.id(),
                    Self::path_of_wal_static(path, state.memtable.id()),
                )?)
            }
            let m = Manifest::create(manifest_path).context("failed to create manifest")?;
            m.add_record_when_init(ManifestRecord::NewMemtable(state.memtable.id()))?;

            m
        } else {
            let ret = Manifest::recover(manifest_path).context("failed to recover manifest")?;
            // need order by sst_id when recover
            let mut im_memtables = BTreeSet::new();
            // redo manifest log
            for record in ret.1 {
                match record {
                    ManifestRecord::NewMemtable(id) => {
                        im_memtables.insert(id);
                        max_id = std::cmp::max(max_id, id);
                    }
                    ManifestRecord::Flush(id) => {
                        if compaction_controller.flush_to_l0() {
                            state.l0_sstables.insert(0, id);
                        } else {
                            // in tiered compaction, Every time flush L0 SSTs,
                            // should flush the SST into a tier placed at the front of the vector
                            state.levels.insert(0, (id, vec![id]));
                        }
                        im_memtables.remove(&id);
                        max_id = std::cmp::max(max_id, id);
                    }
                    ManifestRecord::Compaction(task, ids) => {
                        let (new_state, _) = compaction_controller.apply_compaction_result(
                            &state,
                            &task,
                            ids.as_slice(),
                        );
                        state = new_state;
                        max_id = std::cmp::max(max_id, *ids.last().unwrap_or(&max_id));
                    }
                }
            }
            max_id += 1;
            // build imm_memtables and memtable
            if options.enable_wal {
                // just recover all to imm_memtables, then create a new memtable
                for id in im_memtables {
                    let m = MemTable::recover_from_wal(id, Self::path_of_wal_static(path, id))?;
                    if !m.is_empty() {
                        state.imm_memtables.insert(0, Arc::new(m));
                    }
                }
                state.memtable = Arc::new(MemTable::create_with_wal(
                    max_id,
                    Self::path_of_wal_static(path, max_id),
                )?);
            } else {
                state.memtable = Arc::new(MemTable::create(max_id));
            }
            ret.0
                .add_record_when_init(ManifestRecord::NewMemtable(max_id))?;

            //build sstables
            let ids = state
                .levels
                .iter()
                .flat_map(|(_, ids)| ids)
                .chain(state.l0_sstables.iter());
            for id in ids {
                // so the block_cache is shared by all sstables
                let sst = SsTable::open(
                    *id,
                    Some(block_cache.clone()),
                    FileObject::open(Self::path_of_sst_static(path, *id).as_path())
                        .context("failed to open SST")?,
                )?;
                state.sstables.insert(*id, Arc::new(sst));
            }

            ret.0
        };

        let storage = Self {
            state: Arc::new(RwLock::new(Arc::new(state))),
            state_lock: Mutex::new(()),
            path: path.to_path_buf(),
            block_cache,
            next_sst_id: AtomicUsize::new(max_id + 1),
            compaction_controller,
            manifest: Some(manifest),
            options: options.into(),
            mvcc: None,
            compaction_filters: Arc::new(Mutex::new(Vec::new())),
        };
        storage.sync_dir()?;

        Ok(storage)
    }

    pub fn sync(&self) -> Result<()> {
        self.state.read().memtable.sync_wal()
    }

    pub fn add_compaction_filter(&self, compaction_filter: CompactionFilter) {
        let mut compaction_filters = self.compaction_filters.lock();
        compaction_filters.push(compaction_filter);
    }

    /// Get a key from the storage. In day 7, this can be further optimized by using a bloom filter.
    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        let state = self.state.read().clone();
        if let Some(v) = state.memtable.get(key) {
            if v.is_empty() {
                return Ok(None);
            }
            return Ok(Some(v));
        }

        for m in state.imm_memtables.iter() {
            if let Some(v) = m.get(key) {
                if v.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(v));
            }
        }

        // L0 SSTs, from latest to earliest.
        let mut sstables_l0 = vec![];
        state.l0_sstables.iter().for_each(|id| {
            if let Some(s) = state.sstables.get(id) {
                if key < s.first_key().raw_ref() || key > s.last_key().raw_ref() {
                    return;
                }
                if let Some(b) = &s.bloom {
                    let key_hash = farmhash::hash32(key);
                    if !b.may_contain(key_hash) {
                        return;
                    }
                }

                sstables_l0.push(s.clone());
            }
        });

        for s in sstables_l0.iter() {
            let s_it =
                SsTableIterator::create_and_seek_to_key(s.clone(), KeySlice::from_slice(key))?;
            if s_it.is_valid() && s_it.key().raw_ref() == key {
                if s_it.value().is_empty() {
                    return Ok(None);
                }
                return Ok(Some(Bytes::copy_from_slice(s_it.value())));
            }
        }

        // L1-lmax SSTs, from latest to earliest.
        for (_, sst_ids) in state.levels.iter() {
            let mut sstables = vec![];
            sst_ids.iter().for_each(|id| {
                if let Some(s) = state.sstables.get(id) {
                    if key < s.first_key().raw_ref() || key > s.last_key().raw_ref() {
                        return;
                    }
                    if let Some(b) = &s.bloom {
                        let key_hash = farmhash::hash32(key);
                        if !b.may_contain(key_hash) {
                            return;
                        }
                    }

                    sstables.push(s.clone());
                }
            });

            let s_it =
                SstConcatIterator::create_and_seek_to_key(sstables, KeySlice::from_slice(key))?;
            if s_it.is_valid() && s_it.key().raw_ref() == key {
                if s_it.value().is_empty() {
                    return Ok(None);
                }
                return Ok(Some(Bytes::copy_from_slice(s_it.value())));
            }
        }

        Ok(None)
    }

    /// Write a batch of data into the storage. Implement in week 2 day 7.
    pub fn write_batch<T: AsRef<[u8]>>(&self, batch: &[WriteBatchRecord<T>]) -> Result<()> {
        let mut data = vec![];
        for record in batch {
            match record {
                WriteBatchRecord::Del(key) => {
                    data.push((KeySlice::from_slice(key.as_ref()), b"".as_slice()));
                }
                WriteBatchRecord::Put(key, value) => {
                    data.push((KeySlice::from_slice(key.as_ref()), value.as_ref()));
                }
            }
        }

        {
            let state = self.state.read();
            state.memtable.put_batch(data.as_slice())?;
        }

        self.try_freeze_memtable()
    }

    fn try_freeze_memtable(&self) -> Result<()> {
        let state = self.state.read();
        if state.memtable.approximate_size() >= self.options.target_sst_size {
            drop(state);
            let lock = &self.state_lock.lock();
            // reset approximate_size when force_freeze_memtable is called
            // check again
            let state = self.state.read();
            if state.memtable.approximate_size() >= self.options.target_sst_size {
                drop(state);
                self.force_freeze_memtable(lock)?;
            }
        }

        Ok(())
    }

    /// Put a key-value pair into the storage by writing into the current memtable.
    /// As our memtable implementation only requires an immutable reference for put,
    /// you ONLY need to take the read lock on state in order to modify the memtable.
    /// This allows concurrent access to the memtable from multiple threads.
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        {
            let state = self.state.read();
            state.memtable.put(key, value)?;
        }

        self.try_freeze_memtable()
    }

    /// Remove a key from the storage by writing an empty value.
    pub fn delete(&self, key: &[u8]) -> Result<()> {
        self.put(key, &[])
    }

    pub(crate) fn path_of_sst_static(path: impl AsRef<Path>, id: usize) -> PathBuf {
        path.as_ref().join(format!("{id:05}.sst"))
    }

    pub(crate) fn path_of_sst(&self, id: usize) -> PathBuf {
        Self::path_of_sst_static(&self.path, id)
    }

    pub(crate) fn path_of_wal_static(path: impl AsRef<Path>, id: usize) -> PathBuf {
        path.as_ref().join(format!("{id:05}.wal"))
    }

    pub(crate) fn path_of_wal(&self, id: usize) -> PathBuf {
        Self::path_of_wal_static(&self.path, id)
    }

    // only needed when have files created or deleted
    pub(super) fn sync_dir(&self) -> Result<()> {
        File::open(&self.path)?
            .sync_all()
            .context("failed to sync dir")
    }

    fn force_freeze_with_new_memtable(&self, new_memtable: mem_table::MemTable) -> Result<()> {
        let mut guard = self.state.write();
        let mut state = guard.as_ref().clone();
        let m = std::mem::replace(&mut state.memtable, new_memtable.into());
        // make test happy. but why? kind of wired design decision
        state.imm_memtables.insert(0, m.clone());
        *guard = Arc::new(state);

        Ok(())
    }

    /// Force freeze the current memtable to an immutable memtable,
    /// the `_state_lock_observer` will be dropped after `force_freeze_memtable` called
    pub fn force_freeze_memtable(&self, _state_lock_observer: &MutexGuard<'_, ()>) -> Result<()> {
        let sst_id = self.next_sst_id();
        let mem_table = if self.options.enable_wal {
            mem_table::MemTable::create_with_wal(sst_id, self.path_of_wal(sst_id))?
        } else {
            mem_table::MemTable::create(sst_id)
        };
        self.force_freeze_with_new_memtable(mem_table)?;

        self.sync_dir()?;

        self.manifest
            .as_ref()
            .unwrap()
            .add_record(_state_lock_observer, ManifestRecord::NewMemtable(sst_id))
    }

    /// Force flush the earliest-created immutable memtable to disk
    pub fn force_flush_next_imm_memtable(&self) -> Result<()> {
        let state_lock = self.state_lock.lock();
        // since update state is just create a new one and replace it with the old one,
        // so this is a snapshot, no need to hold the lock for the whole process
        let memtable_to_flush = {
            let guard = self.state.read();
            guard.imm_memtables.last().unwrap().clone()
        };

        let mut ss_table_builder = SsTableBuilder::new(self.options.block_size);
        memtable_to_flush.flush(&mut ss_table_builder)?;
        let sst_id = memtable_to_flush.id();
        let sst = ss_table_builder.build(
            sst_id,
            Some(self.block_cache.clone()),
            self.path_of_sst(sst_id),
        )?;

        {
            let mut guard = self.state.write();
            let mut state = guard.as_ref().clone();

            state.imm_memtables.pop();
            if self.compaction_controller.flush_to_l0() {
                state.l0_sstables.insert(0, sst.sst_id());
            } else {
                // in tiered compaction, Every time flush L0 SSTs,
                // should flush the SST into a tier placed at the front of the vector
                state.levels.insert(0, (sst.sst_id(), vec![sst.sst_id()]));
            }
            state.sstables.insert(sst.sst_id(), Arc::new(sst));
            *guard = Arc::new(state);
        }

        self.sync_dir()?;

        self.manifest
            .as_ref()
            .unwrap()
            .add_record(&state_lock, ManifestRecord::Flush(sst_id))
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
        let state = self.state.read().clone();

        // memtable
        let mut m_merge_iterators = vec![Box::new(state.memtable.scan(lower, upper))];
        for i in state.imm_memtables.iter() {
            let it = i.scan(lower, upper);
            m_merge_iterators.push(Box::new(it));
        }
        let m_memo_iter = MergeIterator::create(m_merge_iterators);

        // l0 sstables
        let mut l0_iters = vec![];
        for i in state.l0_sstables.iter() {
            let t = state.sstables[i].clone();
            if !t.range_overlap(lower, upper) {
                continue;
            }

            let s = match lower {
                Bound::Included(lower) => {
                    SsTableIterator::create_and_seek_to_key(t.clone(), KeySlice::from_slice(lower))?
                }
                Bound::Excluded(lower) => {
                    // if t.first_key().as_key_slice() >= KeySlice::from_slice(lower)
                    //     || t.last_key().as_key_slice() <= KeySlice::from_slice(lower)
                    // {
                    //     continue;
                    // }
                    let mut s = SsTableIterator::create_and_seek_to_key(
                        t.clone(),
                        KeySlice::from_slice(lower),
                    )?;
                    if s.is_valid() && s.key().raw_ref() == lower {
                        s.next()?;
                    }

                    s
                }
                Bound::Unbounded => SsTableIterator::create_and_seek_to_first(t.clone())?,
            };
            l0_iters.push(Box::new(s));
        }
        let m_l0_iter = MergeIterator::create(l0_iters);
        let two_l0_iter = TwoMergeIterator::create(m_memo_iter, m_l0_iter)?;

        // l1-lmax sstables
        let mut concat_iters = vec![];
        for (_, sst_ids) in &state.levels {
            let mut ss_tables = vec![];
            for i in sst_ids {
                let t = state.sstables[i].clone();
                ss_tables.push(t);
            }
            let concat_iter = match lower {
                Bound::Included(lower) => SstConcatIterator::create_and_seek_to_key(
                    ss_tables,
                    KeySlice::from_slice(lower),
                )?,
                Bound::Excluded(lower) => {
                    let mut iter = SstConcatIterator::create_and_seek_to_key(
                        ss_tables,
                        KeySlice::from_slice(lower),
                    )?;
                    if iter.is_valid() && iter.key().raw_ref() == lower {
                        iter.next()?;
                    }

                    iter
                }
                Bound::Unbounded => SstConcatIterator::create_and_seek_to_first(ss_tables)?,
            };
            concat_iters.push(Box::new(concat_iter));
        }
        let m_iter = MergeIterator::create(concat_iters);
        let two_m = TwoMergeIterator::create(two_l0_iter, m_iter)?;
        let lit = LsmIterator::new(two_m, Self::into_vec(upper))?;

        Ok(FusedIterator::new(lit))
    }

    fn into_vec(b: Bound<&[u8]>) -> Bound<Vec<u8>> {
        match b {
            Bound::Included(k) => Bound::Included(k.to_vec()),
            Bound::Excluded(k) => Bound::Excluded(k.to_vec()),
            Bound::Unbounded => Bound::Unbounded,
        }
    }
}
