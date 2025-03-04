// Copyright (c) 2022-2025 Alex Chi Z
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use crate::iterators::concat_iterator::SstConcatIterator;
use crate::iterators::merge_iterator::MergeIterator;
use crate::iterators::two_merge_iterator::TwoMergeIterator;
use crate::iterators::StorageIterator;
use crate::key::{KeySlice, TS_RANGE_BEGIN, TS_RANGE_END};
use crate::mem_table::{map_bound, map_bound_bytes, ts_bound_mapper, MemTable};
use crate::mvcc::txn::TxnIterator;
use crate::table::{FileObject, SsTableBuilder};
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::fs::File;
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use anyhow::{Context, Result};

use bytes::Bytes;
use parking_lot::{Mutex, MutexGuard, RwLock};

use crate::block::Block;
use crate::compact::{
    CompactionController, CompactionOptions, LeveledCompactionController, LeveledCompactionOptions,
    SimpleLeveledCompactionController, SimpleLeveledCompactionOptions, TieredCompactionController,
};
use crate::lsm_iterator::{FusedIterator, LsmIterator};
use crate::manifest::{Manifest, ManifestRecord};
use crate::mvcc::LsmMvccInner;
use crate::table::{SsTable, SsTableIterator};

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
        // Not sure why there is no wal here?
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
        // Send signal to terminate the running threads
        self.compaction_notifier.send(()).ok();
        self.flush_notifier.send(()).ok();
        //Take control of the running compaction thread and wait for it to end
        let mut compaction_thread = self.compaction_thread.lock();
        if let Some(compaction_thread) = compaction_thread.take() {
            compaction_thread
                .join()
                .map_err(|e| anyhow::anyhow!("{:?}", e))?;
        }

        // Take control of the running flush thread and wait for it to end
        let mut flush_thread = self.flush_thread.lock();
        if let Some(flush_thread) = flush_thread.take() {
            flush_thread
                .join()
                .map_err(|e| anyhow::anyhow!("{:?}", e))?;
        }

        self.inner.sync_dir()?;
        if !self.inner.options.enable_wal {
            if !self.inner.state.read().memtable.is_empty() {
                self.inner
                    .freeze_memtable(Arc::new(MemTable::create(self.inner.next_sst_id())))?;
            }
            while !self.inner.state.read().imm_memtables.is_empty() {
                self.inner.force_flush_next_imm_memtable()?;
            }
        }
        self.inner.sync_dir()?;
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

    pub fn scan<'a>(
        self: &'a Arc<Self>,
        lower: Bound<&[u8]>,
        upper: Bound<&[u8]>,
    ) -> Result<TxnIterator> {
        let txn_iter = self.inner.scan(lower, upper)?;
        Ok(txn_iter)
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
        let mut next_sst_id: usize = 1;
        let mut mvcc_ts = 0;
        let path = path.as_ref();
        let mut state = LsmStorageState::create(&options);
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

        let manifest_path = path.join("MANIFEST");
        let manifest_object;
        let manifest_records;
        if !manifest_path.is_file() {
            manifest_object = Manifest::create(manifest_path)?;
            manifest_records = vec![];
        } else {
            (manifest_object, manifest_records) = Manifest::recover(manifest_path)?;
        }

        let mut memtable_ids = BTreeSet::new();
        for record in manifest_records {
            if let ManifestRecord::Flush(sst_id) = record {
                if compaction_controller.flush_to_l0() {
                    state.l0_sstables.insert(0, sst_id);
                } else {
                    // Tiered Compaction
                    state.levels.insert(0, (sst_id, vec![sst_id]));
                }
                // Remove the flushed Memtables
                memtable_ids.remove(&sst_id);
                next_sst_id = next_sst_id.max(sst_id);
            } else if let ManifestRecord::Compaction(compaction_task, output_ids) = record {
                let (new_state, _) = compaction_controller.apply_compaction_result(
                    &state,
                    &compaction_task,
                    &output_ids[..],
                    true,
                ); // Not sure if apply_compaction_result must be true here
                state = new_state;

                next_sst_id = next_sst_id.max(output_ids.iter().max().copied().unwrap_or_default());
            } else if let ManifestRecord::NewMemtable(id) = record {
                // Insert the new memtable
                memtable_ids.insert(id);
                next_sst_id = next_sst_id.max(id);
            } else {
                panic!("Invalid Manifest Record type")
            }
        }

        let block_cache = Arc::new(BlockCache::new(1024));

        // Recover the ssts from the file objects
        // Note that deleted tables are never present in sstable map so no need to delete
        for sst_id_ptr in state
            .l0_sstables
            .iter()
            .chain(state.levels.iter().flat_map(|x| x.1.iter()))
        {
            let sst_id = *sst_id_ptr;
            let ss_table = SsTable::open(
                sst_id,
                Some(block_cache.clone()),
                FileObject::open(&Self::path_of_sst_static(path, sst_id))
                    .context("failed to open SST")?,
            )?;
            mvcc_ts = mvcc_ts.max(ss_table.max_ts());
            // println!("Mvcc,Ts {:?}",mvcc_ts);
            state.sstables.insert(sst_id, Arc::new(ss_table));
        }

        let state_lock = Mutex::new(());
        // println!("Reached till wal extraction");
        if options.enable_wal {
            let mut wal_cnt = 0;
            // Added the memtables with wal's to imm_memtables
            for id in memtable_ids {
                println!("Extracted {:?}", id);
                wal_cnt += 1;
                let mem_table = Arc::new(MemTable::recover_from_wal(
                    id,
                    Self::path_of_wal_static(path, id),
                )?);
                let mut memtable_iterator = mem_table.scan(Bound::Unbounded, Bound::Unbounded);
                while memtable_iterator.is_valid() {
                    mvcc_ts = mvcc_ts.max(memtable_iterator.key().ts());
                    memtable_iterator.next()?;
                }

                state.imm_memtables.insert(0, mem_table);
            }
            state.memtable = Arc::new(MemTable::create_with_wal(
                next_sst_id + 1,
                Self::path_of_wal_static(path, next_sst_id + 1),
            )?);
            manifest_object.add_record(
                &state_lock.lock(),
                ManifestRecord::NewMemtable(next_sst_id + 1),
            )?;
            println!("Extracted Wals {:?}", wal_cnt);
        } else {
            state.memtable = Arc::new(MemTable::create(next_sst_id + 1));
        }

        next_sst_id += 2;

        let storage = Self {
            state: Arc::new(RwLock::new(Arc::new(state))),
            state_lock,
            path: path.to_path_buf(),
            block_cache,
            next_sst_id: AtomicUsize::new(next_sst_id),
            compaction_controller,
            manifest: Some(manifest_object),
            options: options.into(),
            mvcc: Some(LsmMvccInner::new(mvcc_ts)),
            compaction_filters: Arc::new(Mutex::new(Vec::new())),
        };

        storage.sync_dir()?;
        Ok(storage)
    }

    pub fn sync(&self) -> Result<()> {
        let state = self.state.read();
        state.memtable.sync_wal()?;
        Ok(())
    }

    pub fn add_compaction_filter(&self, compaction_filter: CompactionFilter) {
        let mut compaction_filters = self.compaction_filters.lock();
        compaction_filters.push(compaction_filter);
    }

    fn key_within(table: &SsTable, key: &[u8]) -> bool {
        if table.first_key().key_ref() > key || table.last_key().key_ref() < key {
            return false;
        }
        true
    }

    pub fn range_overlap(table: &SsTable, _lower: Bound<&[u8]>, _upper: Bound<&[u8]>) -> bool {
        match _lower {
            Bound::Included(val) => {
                if val > table.last_key().key_ref() {
                    return false;
                }
            }
            Bound::Excluded(val) => {
                if val >= table.last_key().key_ref() {
                    return false;
                }
            }
            Bound::Unbounded => {}
        };

        match _upper {
            Bound::Included(val) => {
                if val < table.first_key().key_ref() {
                    return false;
                }
            }
            Bound::Excluded(val) => {
                if val <= table.first_key().key_ref() {
                    return false;
                }
            }
            Bound::Unbounded => {}
        };

        true
    }

    pub fn keep_table(sstable: Arc<SsTable>, _key: &[u8]) -> bool {
        if !Self::key_within(&sstable, _key)
            || (sstable.bloom.is_some()
                && !sstable
                    .bloom
                    .as_ref()
                    .unwrap()
                    .may_contain(farmhash::fingerprint32(_key)))
        {
            return false;
        }

        true
    }

    pub fn box_maker<I>(vec: Vec<I>) -> Vec<Box<I>>
    where
        I: StorageIterator,
    {
        let mut sol = Vec::new();
        for v in vec {
            sol.push(Box::new(v));
        }
        sol
    }

    /// Get a key from the storage. In day 7, this can be further optimized by using a bloom filter.
    pub fn get<'a>(self: &'a Arc<Self>, _key: &[u8]) -> Result<Option<Bytes>> {
        let txn = self.mvcc.as_ref().unwrap().new_txn(self.clone(), true);
        txn.get(_key)
    }

    pub fn get_with_ts(&self, _key: &[u8], read_ts: u64) -> Result<Option<Bytes>> {
        let data = {
            let guard = self.state.read();
            Arc::clone(&guard)
        };

        let mut mem_table_iters = Vec::with_capacity(data.imm_memtables.len() + 1);

        let lower = Bound::Included(KeySlice::from_slice(_key, TS_RANGE_BEGIN));
        let upper = Bound::Included(KeySlice::from_slice(_key, TS_RANGE_END));

        mem_table_iters.push(data.memtable.scan(lower, upper));

        for table in data.imm_memtables.iter() {
            mem_table_iters.push(table.scan(lower, upper))
        }

        let key_slice = KeySlice::from_slice(_key, TS_RANGE_BEGIN);
        let mut l0_iters = Vec::with_capacity(data.l0_sstables.len());
        for sst_index in data.l0_sstables.iter() {
            let sstable = data.sstables[sst_index].clone();

            if !Self::keep_table(sstable.clone(), _key) {
                continue;
            }

            l0_iters.push(SsTableIterator::create_and_seek_to_key(sstable, key_slice)?);
        }

        let mut level_iters = Vec::with_capacity(data.levels.len());

        for i in 0..data.levels.len() {
            let mut sst_level_tables = Vec::new();
            for table_id in data.levels[i].1.iter() {
                if Self::keep_table(data.sstables[table_id].clone(), _key) {
                    sst_level_tables.push(data.sstables[table_id].clone());
                }
            }

            level_iters.push(SstConcatIterator::create_and_seek_to_key(
                sst_level_tables,
                key_slice,
            )?);
        }

        let complete_iterator = LsmIterator::new(
            TwoMergeIterator::create(
                TwoMergeIterator::create(
                    MergeIterator::create(Self::box_maker(mem_table_iters)),
                    MergeIterator::create(Self::box_maker(l0_iters)),
                )?,
                MergeIterator::create(Self::box_maker(level_iters)),
            )?,
            Bound::Unbounded,
            read_ts,
        )?;

        if complete_iterator.is_valid()
            && complete_iterator.key() == _key
            && !complete_iterator.value().is_empty()
        {
            return Ok(Some(Bytes::copy_from_slice(complete_iterator.value())));
        }

        Ok(None)
    }

    /// Write a batch of data into the storage. Implement in week 2 day 7.
    pub fn write_batch<T: AsRef<[u8]>>(&self, batch: &[WriteBatchRecord<T>]) -> Result<()> {
        for record in batch.iter() {
            let _write_guard = self.mvcc.as_ref().unwrap().write_lock.lock();
            let ts = self.mvcc.as_ref().unwrap().latest_commit_ts() + 1;
            match record {
                WriteBatchRecord::Put(key, value) => {
                    let _key = key.as_ref();
                    let _value = value.as_ref();
                    assert!(!_value.is_empty(), "Value cannot be empty");
                    assert!(!_key.is_empty(), "Key cannot be empty");
                    println!("Put-Ts: {:?}", ts);
                    let size;
                    {
                        let data = self.state.read();
                        data.memtable.put(KeySlice::from_slice(_key, ts), _value)?;
                        size = data.memtable.approximate_size()
                    }

                    self.try_freeze(size)?;
                }
                WriteBatchRecord::Del(key) => {
                    let _key = key.as_ref();
                    assert!(!_key.is_empty(), "Value cannot be empty");

                    let size;
                    {
                        let data = self.state.read();
                        data.memtable.put(KeySlice::from_slice(_key, ts), &[])?;
                        size = data.memtable.approximate_size()
                    }

                    self.try_freeze(size)?;
                }
            };
            self.mvcc.as_ref().unwrap().update_commit_ts(ts);
        }
        Ok(())
    }

    /// Put a key-value pair into the storage by writing into the current memtable.
    pub fn put(&self, _key: &[u8], _value: &[u8]) -> Result<()> {
        self.write_batch(&[WriteBatchRecord::Put(_key, _value)])
    }

    /// Remove a key from the storage by writing an empty value.
    pub fn delete(&self, _key: &[u8]) -> Result<()> {
        self.write_batch(&[WriteBatchRecord::Del(_key)])
    }

    fn try_freeze(&self, size: usize) -> Result<()> {
        // Check for the condition for size before entering the lock
        if self.options.target_sst_size <= size {
            // Use the state lock for singular access while freezing
            let state_lock = self.state_lock.lock();
            let guard = self.state.read();

            if self.options.target_sst_size <= size {
                // Drop the read lock to memtable before freezing
                drop(guard);
                self.force_freeze_memtable(&state_lock)?;
            }
        }
        Ok(())
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

    pub(crate) fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub(super) fn sync_dir(&self) -> Result<()> {
        File::open(self.path.clone())?.sync_all()?;
        Ok(())
    }

    /// Force freeze the current memtable to an immutable memtable
    pub fn force_freeze_memtable(&self, state_lock_observer: &MutexGuard<'_, ()>) -> Result<()> {
        let memtable_id = self.next_sst_id();
        let memtable;
        if self.options.enable_wal {
            memtable = Arc::new(MemTable::create_with_wal(
                memtable_id,
                self.path_of_wal(memtable_id),
            )?);
            self.manifest.as_ref().unwrap().add_record(
                state_lock_observer,
                ManifestRecord::NewMemtable(memtable_id),
            )?;
        } else {
            memtable = Arc::new(MemTable::create(memtable_id));
        };

        self.freeze_memtable(memtable)
    }

    fn freeze_memtable(&self, memtable: Arc<MemTable>) -> Result<()> {
        let old_memtable;
        {
            // Create a mutable reference which can overwrite the state
            let mut guard = self.state.write();
            // Create a copy to save all changes before updating the state
            let mut snapshot = guard.as_ref().clone();

            old_memtable = std::mem::replace(&mut snapshot.memtable, memtable);
            if self.options.enable_wal {
                old_memtable.sync_wal()?;
            }
            snapshot.imm_memtables.insert(0, old_memtable);
            *guard = Arc::new(snapshot);
        }
        Ok(())
    }

    /// Force flush the earliest-created immutable memtable to disk
    pub fn force_flush_next_imm_memtable(&self) -> Result<()> {
        let state_lock = self.state_lock.lock();
        let flush_memtable;
        {
            let guard = self.state.read();
            flush_memtable = guard.imm_memtables.last().unwrap().clone()
        };

        let mut builder = SsTableBuilder::new(self.options.block_size);
        flush_memtable.flush(&mut builder)?;
        let id = flush_memtable.id();
        let block_cache = Some(self.block_cache.clone());
        let new_sst = Arc::new(builder.build(id, block_cache, self.path_of_sst(id))?);

        {
            let mut guard = self.state.write(); // Acquires a write lock
                                                // Create a copy to save all changes before updating the state
            let mut snapshot = guard.as_ref().clone();
            snapshot.imm_memtables.pop();
            if self.compaction_controller.flush_to_l0() {
                snapshot.l0_sstables.insert(0, id);
            } else {
                snapshot
                    .levels
                    .insert(0, (new_sst.sst_id(), vec![new_sst.sst_id()]));
            }
            snapshot.sstables.insert(id, new_sst);
            *guard = Arc::new(snapshot);
        };

        self.manifest
            .as_ref()
            .unwrap()
            .add_record(&state_lock, ManifestRecord::Flush(id))?;
        self.sync_dir()?;

        println!("Flushed sst_id {:?} ", id);
        Ok(())
    }

    pub fn new_txn(&self) -> Result<()> {
        // no-op
        Ok(())
    }

    pub fn sst_concat_iter_level(
        cloned_data: Arc<LsmStorageState>,
        idx: usize,
        _lower: Bound<&[u8]>,
        _upper: Bound<&[u8]>,
    ) -> Result<SstConcatIterator> {
        assert!(
            idx < cloned_data.levels.len(),
            "Expected index ({}) to be less than number of levels ({})",
            idx,
            cloned_data.levels.len()
        );

        let mut sst_level_tables = Vec::new();
        for table_id in cloned_data.levels[idx].1.iter() {
            if Self::range_overlap(
                cloned_data.sstables[table_id].clone().as_ref(),
                _lower,
                _upper,
            ) {
                sst_level_tables.push(cloned_data.sstables[table_id].clone());
            }
        }

        Ok(match _lower {
            Bound::Included(key) => SstConcatIterator::create_and_seek_to_key(
                sst_level_tables,
                KeySlice::from_slice(key, TS_RANGE_BEGIN),
            )?,
            Bound::Excluded(key) => {
                let mut iter = SstConcatIterator::create_and_seek_to_key(
                    sst_level_tables,
                    KeySlice::from_slice(key, TS_RANGE_BEGIN),
                )?;
                if iter.is_valid() && iter.key().key_ref() == key {
                    iter.next()?;
                }
                iter
            }
            Bound::Unbounded => SstConcatIterator::create_and_seek_to_first(sst_level_tables)?,
        })
    }

    /// Create an iterator over a range of keys.
    pub fn scan(
        self: &Arc<Self>,
        _lower: Bound<&[u8]>,
        _upper: Bound<&[u8]>,
    ) -> Result<TxnIterator> {
        let txn = self.mvcc.as_ref().unwrap().new_txn(self.clone(), true);
        txn.scan(_lower, _upper)
    }

    pub fn scan_with_ts(
        &self,
        _lower: Bound<&[u8]>,
        _upper: Bound<&[u8]>,
        read_ts: u64,
    ) -> Result<FusedIterator<LsmIterator>> {
        let cloned_data = {
            let guard = self.state.read();
            Arc::clone(&guard)
        };
        // drop the lock here, to keep the execution time to minimum

        let lower = map_bound(_lower, ts_bound_mapper(_lower, true));
        let upper = map_bound(_upper, ts_bound_mapper(_upper, false));

        let mut memtables = Vec::with_capacity(cloned_data.imm_memtables.len() + 1);
        memtables.push(Box::new(cloned_data.memtable.scan(lower, upper)));
        for imm_memtable in cloned_data.imm_memtables.iter() {
            memtables.push(Box::new(imm_memtable.scan(lower, upper)));
        }

        let mut sstables = Vec::with_capacity(cloned_data.l0_sstables.len());
        for sstable_index in cloned_data.l0_sstables.iter() {
            let sstable = cloned_data.sstables[sstable_index].clone();
            if !Self::range_overlap(&sstable, _lower, _upper) {
                continue;
            }
            let iter = match lower {
                Bound::Included(key) => SsTableIterator::create_and_seek_to_key(sstable, key)?,
                Bound::Excluded(key) => {
                    let mut iter = SsTableIterator::create_and_seek_to_key(sstable, key)?;
                    if iter.is_valid() && iter.key().key_ref() == key.key_ref() {
                        iter.next()?;
                    }
                    iter
                }
                Bound::Unbounded => SsTableIterator::create_and_seek_to_first(sstable)?,
            };
            sstables.push(Box::new(iter));
        }

        let iter_mem = MergeIterator::create(memtables);
        let iter_sst = MergeIterator::create(sstables);

        let mut iter_levels = Vec::new();
        for i in 0..cloned_data.levels.len() {
            iter_levels.push(Box::new(Self::sst_concat_iter_level(
                cloned_data.clone(),
                i,
                _lower,
                _upper,
            )?));
        }
        Ok(FusedIterator::new(LsmIterator::new(
            TwoMergeIterator::create(
                TwoMergeIterator::create(iter_mem, iter_sst)?,
                MergeIterator::create(iter_levels),
            )?,
            map_bound_bytes(_upper),
            read_ts,
        )?))
    }
}
