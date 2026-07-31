// Copyright (c) 2022-2026 Alex Chi Z
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

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use anyhow::Result;
use bytes::Bytes;
use parking_lot::{Mutex, MutexGuard, RwLock};

use crate::block::Block;
use crate::compact::{
    CompactionController, CompactionOptions, LeveledCompactionController, LeveledCompactionOptions,
    SimpleLeveledCompactionController, SimpleLeveledCompactionOptions, TieredCompactionController,
};
use crate::iterators::StorageIterator;
use crate::iterators::concat_iterator::SstConcatIterator;
use crate::iterators::merge_iterator::MergeIterator;
use crate::iterators::two_merge_iterator::TwoMergeIterator;
use crate::lsm_iterator::{FusedIterator, LsmIterator};
use crate::manifest::{Manifest, ManifestRecord};
use crate::mem_table::{MemTable, map_bound};
use crate::mvcc::LsmMvccInner;
use crate::table::{FileObject, SsTable, SsTableIterator};

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
        self.compaction_notifier.send(()).ok();
        self.flush_notifier.send(()).ok();
        if let Some(handle) = self.compaction_thread.lock().take() {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("compaction thread panicked"))?;
        }
        if let Some(handle) = self.flush_thread.lock().take() {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("flush thread panicked"))?;
        }
        if self.inner.options.enable_wal {
            let memtables = {
                let state = self.inner.state.read();
                std::iter::once(state.memtable.clone())
                    .chain(state.imm_memtables.iter().cloned())
                    .collect::<Vec<_>>()
            };
            for memtable in memtables {
                memtable.sync_wal()?;
            }
        } else {
            if !self.inner.state.read().memtable.is_empty() {
                let state_lock = self.inner.state_lock.lock();
                if !self.inner.state.read().memtable.is_empty() {
                    self.inner.force_freeze_memtable(&state_lock)?;
                }
            }
            while !self.inner.state.read().imm_memtables.is_empty() {
                self.inner.force_flush_next_imm_memtable()?;
            }
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

    pub(crate) fn mvcc(&self) -> &LsmMvccInner {
        self.mvcc.as_ref().unwrap()
    }

    /// Start the storage engine by either loading an existing directory or creating a new one if the directory does
    /// not exist.
    pub(crate) fn open(path: impl AsRef<Path>, options: LsmStorageOptions) -> Result<Self> {
        let path = path.as_ref();
        std::fs::create_dir_all(path)?;
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
        let (manifest, records) = if manifest_path.exists() {
            Manifest::recover(&manifest_path)?
        } else {
            let manifest = Manifest::create(&manifest_path)?;
            File::open(path)?.sync_all()?;
            (manifest, Vec::new())
        };

        let mut live_memtable_ids = Vec::new();
        let mut max_id = 0usize;
        for record in records {
            match record {
                ManifestRecord::Flush(sst_id) => {
                    max_id = max_id.max(sst_id);
                    live_memtable_ids.retain(|id| *id != sst_id);
                    if compaction_controller.flush_to_l0() {
                        state.l0_sstables.insert(0, sst_id);
                    } else {
                        state.levels.insert(0, (sst_id, vec![sst_id]));
                    }
                }
                ManifestRecord::NewMemtable(memtable_id) => {
                    max_id = max_id.max(memtable_id);
                    live_memtable_ids.push(memtable_id);
                }
                ManifestRecord::Compaction(task, output) => {
                    if let Some(output_max) = output.iter().max() {
                        max_id = max_id.max(*output_max);
                    }
                    (state, _) =
                        compaction_controller.apply_compaction_result(&state, &task, &output, true);
                }
            }
        }

        let block_cache = Arc::new(BlockCache::new(1024));
        let live_sst_ids = state
            .l0_sstables
            .iter()
            .chain(state.levels.iter().flat_map(|(_, sst_ids)| sst_ids.iter()))
            .copied()
            .collect::<HashSet<_>>();
        for sst_id in live_sst_ids {
            max_id = max_id.max(sst_id);
            let file = FileObject::open(&Self::path_of_sst_static(path, sst_id))?;
            let sst = SsTable::open(sst_id, Some(block_cache.clone()), file)?;
            state.sstables.insert(sst_id, Arc::new(sst));
        }
        if matches!(options.compaction_options, CompactionOptions::Leveled(_)) {
            let sstables = &state.sstables;
            for (_, sst_ids) in &mut state.levels {
                sst_ids.sort_by(|left, right| {
                    sstables
                        .get(left)
                        .unwrap()
                        .first_key()
                        .cmp(sstables.get(right).unwrap().first_key())
                });
            }
        }

        if options.enable_wal {
            for memtable_id in live_memtable_ids.into_iter().rev() {
                let memtable = MemTable::recover_from_wal(
                    memtable_id,
                    Self::path_of_wal_static(path, memtable_id),
                )?;
                state.imm_memtables.push(Arc::new(memtable));
            }
        }

        let mut current_memtable_id = max_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("SST ID exhausted"))?;
        while Self::path_of_sst_static(path, current_memtable_id).exists()
            || Self::path_of_wal_static(path, current_memtable_id).exists()
        {
            current_memtable_id = current_memtable_id
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("SST ID exhausted"))?;
        }
        state.memtable = if options.enable_wal {
            let memtable = Arc::new(MemTable::create_with_wal(
                current_memtable_id,
                Self::path_of_wal_static(path, current_memtable_id),
            )?);
            File::open(path)?.sync_all()?;
            manifest.add_record_when_init(ManifestRecord::NewMemtable(current_memtable_id))?;
            memtable
        } else {
            Arc::new(MemTable::create(current_memtable_id))
        };
        let next_sst_id = current_memtable_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("SST ID exhausted"))?;

        let storage = Self {
            state: Arc::new(RwLock::new(Arc::new(state))),
            state_lock: Mutex::new(()),
            path: path.to_path_buf(),
            block_cache,
            next_sst_id: AtomicUsize::new(next_sst_id),
            compaction_controller,
            manifest: Some(manifest),
            options: options.into(),
            mvcc: None,
            compaction_filters: Arc::new(Mutex::new(Vec::new())),
        };

        Ok(storage)
    }

    pub fn sync(&self) -> Result<()> {
        if self.options.enable_wal {
            self.state.read().memtable.sync_wal()?;
        }
        Ok(())
    }

    pub fn add_compaction_filter(&self, compaction_filter: CompactionFilter) {
        let mut compaction_filters = self.compaction_filters.lock();
        compaction_filters.push(compaction_filter);
    }

    /// Get a key from the storage. In day 7, this can be further optimized by using a bloom filter.
    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        let snapshot = {
            let guard = self.state.read();
            guard.clone()
        };
        if let Some(value) = snapshot.memtable.get(key) {
            return Ok((!value.is_empty()).then_some(value));
        }
        for memtable in &snapshot.imm_memtables {
            if let Some(value) = memtable.get(key) {
                return Ok((!value.is_empty()).then_some(value));
            }
        }

        let key_hash = farmhash::fingerprint32(key);
        let mut l0_iters = Vec::new();
        for sst_id in &snapshot.l0_sstables {
            let sst = snapshot.sstables.get(sst_id).unwrap().clone();
            if key < sst.first_key().raw_ref() || key > sst.last_key().raw_ref() {
                continue;
            }
            if sst
                .bloom
                .as_ref()
                .is_some_and(|bloom| !bloom.may_contain(key_hash))
            {
                continue;
            }
            l0_iters.push(Box::new(SsTableIterator::create_and_seek_to_key(
                sst,
                crate::key::KeySlice::from_slice(key),
            )?));
        }
        let l0_iter = MergeIterator::create(l0_iters);

        let mut level_iters = Vec::with_capacity(snapshot.levels.len());
        for (_, level_sst_ids) in &snapshot.levels {
            let level_ssts = level_sst_ids
                .iter()
                .map(|sst_id| snapshot.sstables.get(sst_id).unwrap().clone())
                .collect();
            level_iters.push(Box::new(SstConcatIterator::create_and_seek_to_key(
                level_ssts,
                crate::key::KeySlice::from_slice(key),
            )?));
        }
        let levels_iter = MergeIterator::create(level_iters);
        let sst_iter = TwoMergeIterator::create(l0_iter, levels_iter)?;
        if sst_iter.is_valid() && sst_iter.key().raw_ref() == key {
            return Ok(
                (!sst_iter.value().is_empty()).then(|| Bytes::copy_from_slice(sst_iter.value()))
            );
        }
        Ok(None)
    }

    /// Write a batch of data into the storage. Implement in week 2 day 7.
    pub fn write_batch<T: AsRef<[u8]>>(&self, batch: &[WriteBatchRecord<T>]) -> Result<()> {
        let size = {
            let state = self.state.read();
            for record in batch {
                match record {
                    WriteBatchRecord::Put(key, value) => {
                        state.memtable.put(key.as_ref(), value.as_ref())?;
                    }
                    WriteBatchRecord::Del(key) => state.memtable.put(key.as_ref(), &[])?,
                }
            }
            state.memtable.approximate_size()
        };

        if size >= self.options.target_sst_size {
            let state_lock = self.state_lock.lock();
            if self.state.read().memtable.approximate_size() >= self.options.target_sst_size {
                self.force_freeze_memtable(&state_lock)?;
            }
        }
        Ok(())
    }

    /// Put a key-value pair into the storage by writing into the current memtable.
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.write_batch(&[WriteBatchRecord::Put(key, value)])
    }

    /// Remove a key from the storage by writing an empty value.
    pub fn delete(&self, key: &[u8]) -> Result<()> {
        self.write_batch(&[WriteBatchRecord::Del(key)])
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
        File::open(&self.path)?.sync_all()?;
        Ok(())
    }

    /// Force freeze the current memtable to an immutable memtable
    pub fn force_freeze_memtable(&self, state_lock_observer: &MutexGuard<'_, ()>) -> Result<()> {
        let new_memtable_id = self.next_sst_id();
        let new_memtable = if self.options.enable_wal {
            let memtable = Arc::new(MemTable::create_with_wal(
                new_memtable_id,
                self.path_of_wal(new_memtable_id),
            )?);
            self.sync_dir()?;
            memtable
        } else {
            Arc::new(MemTable::create(new_memtable_id))
        };
        let mut state_guard = self.state.write();
        let mut snapshot = state_guard.as_ref().clone();
        snapshot.memtable.sync_wal()?;
        if self.options.enable_wal {
            self.manifest.as_ref().unwrap().add_record(
                state_lock_observer,
                ManifestRecord::NewMemtable(new_memtable_id),
            )?;
        }
        let old_memtable = std::mem::replace(&mut snapshot.memtable, new_memtable);
        snapshot.imm_memtables.insert(0, old_memtable);
        *state_guard = Arc::new(snapshot);
        Ok(())
    }

    /// Force flush the earliest-created immutable memtable to disk
    pub fn force_flush_next_imm_memtable(&self) -> Result<()> {
        let state_lock = self.state_lock.lock();
        let memtable = {
            let state = self.state.read();
            let Some(memtable) = state.imm_memtables.last() else {
                return Ok(());
            };
            memtable.clone()
        };

        let sst_id = memtable.id();
        let mut builder = crate::table::SsTableBuilder::new(self.options.block_size);
        memtable.flush(&mut builder)?;
        let sst = Arc::new(builder.build(
            sst_id,
            Some(self.block_cache.clone()),
            self.path_of_sst(sst_id),
        )?);
        self.sync_dir()?;

        self.manifest
            .as_ref()
            .unwrap()
            .add_record(&state_lock, ManifestRecord::Flush(sst_id))?;

        let mut state_guard = self.state.write();
        let mut snapshot = state_guard.as_ref().clone();
        let removed = snapshot.imm_memtables.pop().unwrap();
        assert_eq!(
            removed.id(),
            sst_id,
            "installed SST must match flushed memtable"
        );
        snapshot.sstables.insert(sst_id, sst);
        if self.compaction_controller.flush_to_l0() {
            snapshot.l0_sstables.insert(0, sst_id);
        } else {
            snapshot.levels.insert(0, (sst_id, vec![sst_id]));
        }
        *state_guard = Arc::new(snapshot);
        drop(state_guard);

        if self.options.enable_wal {
            std::fs::remove_file(self.path_of_wal(sst_id))?;
            self.sync_dir()?;
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
        let snapshot = {
            let guard = self.state.read();
            guard.clone()
        };
        let mut memtable_iters = Vec::with_capacity(1 + snapshot.imm_memtables.len());
        memtable_iters.push(Box::new(snapshot.memtable.scan(lower, upper)));
        for memtable in &snapshot.imm_memtables {
            memtable_iters.push(Box::new(memtable.scan(lower, upper)));
        }
        let memtable_iter = MergeIterator::create(memtable_iters);

        let mut l0_iters: Vec<Box<SsTableIterator>> = Vec::new();
        for sst_id in &snapshot.l0_sstables {
            let sst = snapshot.sstables.get(sst_id).unwrap().clone();
            if !range_overlaps_sst(lower, upper, &sst) {
                continue;
            }
            let mut iter = match lower {
                Bound::Included(key) | Bound::Excluded(key) => {
                    SsTableIterator::create_and_seek_to_key(
                        sst,
                        crate::key::KeySlice::from_slice(key),
                    )?
                }
                Bound::Unbounded => SsTableIterator::create_and_seek_to_first(sst)?,
            };
            if matches!(
                lower,
                Bound::Excluded(key) if iter.is_valid() && iter.key().raw_ref() == key
            ) {
                iter.next()?;
            }
            l0_iters.push(Box::new(iter));
        }
        let l0_iter = MergeIterator::create(l0_iters);
        let memtable_l0_iter = TwoMergeIterator::create(memtable_iter, l0_iter)?;

        let mut level_iters = Vec::with_capacity(snapshot.levels.len());
        for (_, level_sst_ids) in &snapshot.levels {
            let level_ssts = level_sst_ids
                .iter()
                .map(|sst_id| snapshot.sstables.get(sst_id).unwrap().clone())
                .filter(|sst| range_overlaps_sst(lower, upper, sst))
                .collect();
            let mut level_iter = match lower {
                Bound::Included(key) | Bound::Excluded(key) => {
                    SstConcatIterator::create_and_seek_to_key(
                        level_ssts,
                        crate::key::KeySlice::from_slice(key),
                    )?
                }
                Bound::Unbounded => SstConcatIterator::create_and_seek_to_first(level_ssts)?,
            };
            if matches!(
                lower,
                Bound::Excluded(key) if level_iter.is_valid() && level_iter.key().raw_ref() == key
            ) {
                level_iter.next()?;
            }
            level_iters.push(Box::new(level_iter));
        }

        let levels_iter = MergeIterator::create(level_iters);
        let inner = TwoMergeIterator::create(memtable_l0_iter, levels_iter)?;
        let lsm_iter = LsmIterator::new(inner, map_bound(upper))?;
        Ok(FusedIterator::new(lsm_iter))
    }
}

fn range_overlaps_sst(lower: Bound<&[u8]>, upper: Bound<&[u8]>, sst: &SsTable) -> bool {
    let after_lower = match lower {
        Bound::Included(key) => sst.last_key().raw_ref() >= key,
        Bound::Excluded(key) => sst.last_key().raw_ref() > key,
        Bound::Unbounded => true,
    };
    let before_upper = match upper {
        Bound::Included(key) => sst.first_key().raw_ref() <= key,
        Bound::Excluded(key) => sst.first_key().raw_ref() < key,
        Bound::Unbounded => true,
    };
    after_lower && before_upper
}
