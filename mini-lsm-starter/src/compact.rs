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

mod leveled;
mod simple_leveled;
mod tiered;

use std::collections::HashSet;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Ok, Result};

pub use leveled::{LeveledCompactionController, LeveledCompactionOptions, LeveledCompactionTask};
use serde::{Deserialize, Serialize};
pub use simple_leveled::{
    SimpleLeveledCompactionController, SimpleLeveledCompactionOptions, SimpleLeveledCompactionTask,
};
pub use tiered::{TieredCompactionController, TieredCompactionOptions, TieredCompactionTask};

use crate::iterators::StorageIterator;
use crate::iterators::concat_iterator::SstConcatIterator;
use crate::iterators::merge_iterator::MergeIterator;
use crate::iterators::two_merge_iterator::TwoMergeIterator;

use crate::key::KeySlice;
use crate::lsm_storage::{LsmStorageInner, LsmStorageState};
use crate::manifest::ManifestRecord;
use crate::table::{SsTable, SsTableBuilder, SsTableIterator};

#[derive(Debug, Serialize, Deserialize)]
pub enum CompactionTask {
    Leveled(LeveledCompactionTask),
    Tiered(TieredCompactionTask),
    Simple(SimpleLeveledCompactionTask),
    /// only used when only have l0, l1 levels, so when used, always compact_to_bottom_level = true by default
    ForceFullCompaction {
        l0_sstables: Vec<usize>,
        l1_sstables: Vec<usize>,
    },
}

impl CompactionTask {
    fn compact_to_bottom_level(&self) -> bool {
        match self {
            CompactionTask::ForceFullCompaction { .. } => true,
            CompactionTask::Leveled(task) => task.is_lower_level_bottom_level,
            CompactionTask::Simple(task) => task.is_lower_level_bottom_level,
            CompactionTask::Tiered(task) => task.bottom_tier_included,
        }
    }
}

pub(crate) enum CompactionController {
    Leveled(LeveledCompactionController),
    Tiered(TieredCompactionController),
    Simple(SimpleLeveledCompactionController),
    NoCompaction,
}

impl CompactionController {
    pub fn generate_compaction_task(&self, snapshot: &LsmStorageState) -> Option<CompactionTask> {
        match self {
            CompactionController::Leveled(ctrl) => ctrl
                .generate_compaction_task(snapshot)
                .map(CompactionTask::Leveled),
            CompactionController::Simple(ctrl) => ctrl
                .generate_compaction_task(snapshot)
                .map(CompactionTask::Simple),
            CompactionController::Tiered(ctrl) => ctrl
                .generate_compaction_task(snapshot)
                .map(CompactionTask::Tiered),
            CompactionController::NoCompaction => unreachable!(),
        }
    }

    pub fn apply_compaction_result(
        &self,
        snapshot: &LsmStorageState,
        task: &CompactionTask,
        new_sst_ids: &[usize],
    ) -> (LsmStorageState, Vec<usize>) {
        match (self, task) {
            (CompactionController::Leveled(ctrl), CompactionTask::Leveled(task)) => {
                ctrl.apply_compaction_result(snapshot, task, new_sst_ids, false)
            }
            (CompactionController::Simple(ctrl), CompactionTask::Simple(task)) => {
                ctrl.apply_compaction_result(snapshot, task, new_sst_ids)
            }
            (CompactionController::Tiered(ctrl), CompactionTask::Tiered(task)) => {
                ctrl.apply_compaction_result(snapshot, task, new_sst_ids)
            }
            _ => unreachable!(),
        }
    }
}

impl CompactionController {
    pub fn flush_to_l0(&self) -> bool {
        matches!(
            self,
            Self::Leveled(_) | Self::Simple(_) | Self::NoCompaction
        )
    }
}

#[derive(Debug, Clone)]
pub enum CompactionOptions {
    /// Leveled compaction with partial compaction + dynamic level support (= RocksDB's Leveled
    /// Compaction)
    Leveled(LeveledCompactionOptions),
    /// Tiered compaction (= RocksDB's universal compaction)
    Tiered(TieredCompactionOptions),
    /// Simple leveled compaction
    Simple(SimpleLeveledCompactionOptions),
    /// In no compaction mode (week 1), always flush to L0
    NoCompaction,
}

impl LsmStorageInner {
    fn compact_from_iter(
        &self,
        mut iter: impl for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>> + 'static,
        compact_to_bottom_level: bool,
    ) -> Result<(Vec<Arc<SsTable>>, Vec<u32>)> {
        let mut ret = vec![];
        let mut all_vlog_ids = vec![];
        let mut builder = SsTableBuilder::new(self.options.block_size);

        while iter.is_valid() {
            if builder.estimated_size() >= self.options.target_sst_size {
                all_vlog_ids.extend_from_slice(builder.vlog_file_ids());
                let sst_id = self.next_sst_id();
                let sst = builder.build(
                    sst_id,
                    Some(self.block_cache.clone()),
                    self.path_of_sst(sst_id),
                )?;
                ret.push(Arc::new(sst));
                builder = SsTableBuilder::new(self.options.block_size);
            }

            // Detect tombstones from raw value: tombstone = [KvKind::Inline:1] only (1 byte)
            let raw = iter.raw_value();
            let is_tombstone = raw.len() == 1 && raw[0] == crate::vlog::KvKind::Inline as u8;
            if !is_tombstone || !compact_to_bottom_level {
                builder.add_raw(iter.key(), iter.raw_value())?;
            }
            iter.next()?;
        }

        if !builder.is_empty() {
            all_vlog_ids.extend_from_slice(builder.vlog_file_ids());
            let sst_id = self.next_sst_id();
            let sst = builder.build(
                sst_id,
                Some(self.block_cache.clone()),
                self.path_of_sst(sst_id),
            )?;
            ret.push(Arc::new(sst));
        }

        all_vlog_ids.sort_unstable();
        all_vlog_ids.dedup();
        Ok((ret, all_vlog_ids))
    }

    fn compact_from_iters(
        &self,
        upper_level_iter: impl for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>> + 'static,
        lower_level_iter: impl for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>> + 'static,
        compact_to_bottom_level: bool,
    ) -> Result<(Vec<Arc<SsTable>>, Vec<u32>)> {
        let s_it = TwoMergeIterator::create(upper_level_iter, lower_level_iter)?;

        self.compact_from_iter(s_it, compact_to_bottom_level)
    }

    fn compact_from_l0_l1(
        &self,
        l0_sst_ids: Vec<usize>,
        l1_sst_ids: Vec<usize>,
        compact_to_bottom_level: bool,
    ) -> Result<(Vec<Arc<SsTable>>, Vec<u32>)> {
        let state = self.state.read().clone();
        let mut m_it = vec![];
        for i in l0_sst_ids.iter() {
            let t = state.sstables[i].clone();
            let s = SsTableIterator::create_and_seek_to_first(t.clone())?;
            m_it.push(Box::new(s));
        }
        let upper_level_iter = MergeIterator::create(m_it);

        let mut s_lower = vec![];
        for i in l1_sst_ids.iter() {
            let t = state.sstables[i].clone();
            s_lower.push(t);
        }
        let lower_level_iter = SstConcatIterator::create_and_seek_to_first(s_lower)?;

        self.compact_from_iters(upper_level_iter, lower_level_iter, compact_to_bottom_level)
    }

    fn compact(&self, task: &CompactionTask) -> Result<(Vec<Arc<SsTable>>, Vec<u32>)> {
        let state = self.state.read().clone();
        match task {
            CompactionTask::Simple(SimpleLeveledCompactionTask {
                upper_level,
                upper_level_sst_ids,
                lower_level: _,
                lower_level_sst_ids,
                ..
            })
            | CompactionTask::Leveled(LeveledCompactionTask {
                upper_level,
                upper_level_sst_ids,
                lower_level: _,
                lower_level_sst_ids,
                ..
            }) => {
                if upper_level.is_none() {
                    self.compact_from_l0_l1(
                        upper_level_sst_ids.clone(),
                        lower_level_sst_ids.clone(),
                        task.compact_to_bottom_level(),
                    )
                } else {
                    let mut s_upper = vec![];
                    for i in upper_level_sst_ids.iter() {
                        let t = state.sstables[i].clone();
                        s_upper.push(t);
                    }
                    let upper_level_iter = SstConcatIterator::create_and_seek_to_first(s_upper)?;

                    let mut s_lower = vec![];
                    for i in lower_level_sst_ids.iter() {
                        let t = state.sstables[i].clone();
                        s_lower.push(t);
                    }
                    let lower_level_iter = SstConcatIterator::create_and_seek_to_first(s_lower)?;

                    self.compact_from_iters(
                        upper_level_iter,
                        lower_level_iter,
                        task.compact_to_bottom_level(),
                    )
                }
            }
            CompactionTask::ForceFullCompaction {
                l0_sstables,
                l1_sstables,
            } => self.compact_from_l0_l1(
                l0_sstables.clone(),
                l1_sstables.clone(),
                task.compact_to_bottom_level(),
            ),
            CompactionTask::Tiered(t) => {
                let mut s_iters = vec![];
                for tier in &t.tiers {
                    let mut sstables = vec![];
                    for i in tier.1.iter() {
                        let t = state.sstables[i].clone();
                        sstables.push(t);
                    }
                    s_iters.push(Box::new(SstConcatIterator::create_and_seek_to_first(
                        sstables,
                    )?));
                }
                let m_iter = MergeIterator::create(s_iters);
                self.compact_from_iter(m_iter, task.compact_to_bottom_level())
            }
        }
    }

    pub fn force_full_compaction(&self) -> Result<()> {
        let state = self.state.read().clone();
        let ssts_to_compact = (&state.l0_sstables, &state.levels[0].1);
        let task = CompactionTask::ForceFullCompaction {
            l0_sstables: ssts_to_compact.0.clone(),
            l1_sstables: ssts_to_compact.1.clone(),
        };

        let (new_ssts, compact_vlog_ids) = self.compact(&task)?;

        {
            let _state_lock = self.state_lock.lock();
            let mut guard = self.state.write();
            let mut snashot = guard.as_ref().clone();

            // Unregister vLog references for old SSTs
            if let Some(ref vlog) = self.vlog {
                for id in ssts_to_compact.0.iter().chain(ssts_to_compact.1) {
                    vlog.unregister_sst_references(*id);
                }
                // Register vLog references for new SSTs
                for sst in &new_ssts {
                    let sst_vlog_ids = compact_vlog_ids.iter().copied().collect::<Vec<_>>();
                    vlog.register_sst_references(sst.sst_id(), &sst_vlog_ids);
                }
            }

            ssts_to_compact
                .0
                .iter()
                .chain(ssts_to_compact.1)
                .for_each(|id| {
                    snashot.sstables.remove(id);
                });
            let new_sst_ids: Vec<_> = new_ssts.iter().map(|t| t.sst_id()).collect();
            snashot.levels[0].1.clone_from(&new_sst_ids);
            new_ssts.iter().for_each(|id| {
                snashot.sstables.insert(id.sst_id(), id.clone());
            });
            let l0_rm = ssts_to_compact.0.iter().collect::<HashSet<_>>();
            // might have new l0 insert into snashot.l0_sstables during compact
            snashot.l0_sstables.retain(|id| !l0_rm.contains(id));

            *guard = Arc::new(snashot);
            drop(guard);

            self.sync_dir()?;
            // Use CompactionV2 if vLog references exist
            let manifest_record = if self.vlog.is_some() {
                ManifestRecord::CompactionV2(task, new_sst_ids.clone(), compact_vlog_ids)
            } else {
                ManifestRecord::Compaction(task, new_sst_ids.clone())
            };
            self.manifest
                .as_ref()
                .unwrap()
                .add_record(&_state_lock, manifest_record)?;
        }

        for id in ssts_to_compact.0.iter().chain(ssts_to_compact.1) {
            std::fs::remove_file(self.path_of_sst(*id))?;
        }

        Ok(())
    }

    fn trigger_compaction(&self) -> Result<()> {
        let task = self
            .compaction_controller
            .generate_compaction_task(self.state.read().clone().as_ref());
        if task.is_none() {
            return Ok(());
        }

        let t = task.as_ref().unwrap();
        let (new_ssts, compact_vlog_ids) = self.compact(t)?;
        let new_sst_ids = new_ssts.iter().map(|x| x.sst_id()).collect::<Vec<_>>();

        let rm_sst_ids = {
            let _state_lock = self.state_lock.lock();
            let mut snapshot = self.state.read().as_ref().clone();
            // specific for leveled compaction, need the sstables in the snapshot
            for s in &new_ssts {
                snapshot.sstables.insert(s.sst_id(), s.clone());
            }
            let (snapshot_partial, rm_sst_ids) = self
                .compaction_controller
                .apply_compaction_result(&snapshot, t, new_sst_ids.as_slice());

            // Unregister vLog references for removed SSTs
            if let Some(ref vlog) = self.vlog {
                for id in &rm_sst_ids {
                    vlog.unregister_sst_references(*id);
                }
                // Register vLog references for new SSTs
                for sst in &new_ssts {
                    vlog.register_sst_references(sst.sst_id(), &compact_vlog_ids);
                }
            }

            let mut guard = self.state.write();
            let mut snapshot = guard.as_ref().clone();
            // specific for leveled compaction
            snapshot.sstables = snapshot_partial.sstables;
            for s in &rm_sst_ids {
                snapshot.sstables.remove(s);
            }
            snapshot.l0_sstables = snapshot_partial.l0_sstables;
            snapshot.levels = snapshot_partial.levels;
            *guard = Arc::new(snapshot);
            drop(guard);

            self.sync_dir()?;
            // Use CompactionV2 if vLog is enabled
            let manifest_record = if self.vlog.is_some() {
                ManifestRecord::CompactionV2(task.unwrap(), new_sst_ids, compact_vlog_ids)
            } else {
                ManifestRecord::Compaction(task.unwrap(), new_sst_ids)
            };
            self.manifest
                .as_ref()
                .unwrap()
                .add_record(&_state_lock, manifest_record)?;

            rm_sst_ids
        };

        for id in &rm_sst_ids {
            std::fs::remove_file(self.path_of_sst(*id))?;
        }

        Ok(())
    }

    pub(crate) fn spawn_compaction_thread(
        self: &Arc<Self>,
        rx: crossbeam_channel::Receiver<()>,
    ) -> Result<Option<std::thread::JoinHandle<()>>> {
        if let CompactionOptions::Leveled(_)
        | CompactionOptions::Simple(_)
        | CompactionOptions::Tiered(_) = self.options.compaction_options
        {
            let this = self.clone();
            let handle = std::thread::spawn(move || {
                let ticker = crossbeam_channel::tick(Duration::from_millis(50));
                loop {
                    crossbeam_channel::select! {
                        recv(ticker) -> _ => if let Err(e) = this.trigger_compaction() {
                            eprintln!("compaction failed: {e}");
                        },
                        recv(rx) -> _ => return
                    }
                }
            });
            return Ok(Some(handle));
        }
        Ok(None)
    }

    fn trigger_flush(&self) -> Result<()> {
        let state = self.state.read().clone();
        if state.imm_memtables.len() + 1 > self.options.num_memtable_limit {
            self.force_flush_next_imm_memtable()?;
        }

        Ok(())
    }

    pub(crate) fn spawn_flush_thread(
        self: &Arc<Self>,
        rx: crossbeam_channel::Receiver<()>,
    ) -> Result<Option<std::thread::JoinHandle<()>>> {
        let this = self.clone();
        let handle = std::thread::spawn(move || {
            let ticker = crossbeam_channel::tick(Duration::from_millis(50));
            loop {
                crossbeam_channel::select! {
                    recv(ticker) -> _ => if let Err(e) = this.trigger_flush() {
                        eprintln!("flush failed: {e}");
                    },
                    recv(rx) -> _ => return
                }
            }
        });
        Ok(Some(handle))
    }
}
