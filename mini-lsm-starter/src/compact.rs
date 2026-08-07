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

mod leveled;
mod simple_leveled;
mod tiered;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
pub use leveled::{LeveledCompactionController, LeveledCompactionOptions, LeveledCompactionTask};
use serde::{Deserialize, Serialize};
pub use simple_leveled::{
    SimpleLeveledCompactionController, SimpleLeveledCompactionOptions, SimpleLeveledCompactionTask,
};
pub use tiered::{TieredCompactionController, TieredCompactionOptions, TieredCompactionTask};

use crate::iterators::{
    StorageIterator, concat_iterator::SstConcatIterator, merge_iterator::MergeIterator,
    two_merge_iterator::TwoMergeIterator,
};
use crate::key::KeySlice;
use crate::lsm_storage::{LsmStorageInner, LsmStorageState};
use crate::manifest::ManifestRecord;
use crate::table::{SsTable, SsTableBuilder, SsTableIterator};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompactionTask {
    Leveled(LeveledCompactionTask),
    Tiered(TieredCompactionTask),
    Simple(SimpleLeveledCompactionTask),
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
        output: &[usize],
        in_recovery: bool,
    ) -> (LsmStorageState, Vec<usize>) {
        if let CompactionTask::ForceFullCompaction {
            l0_sstables,
            l1_sstables,
        } = task
        {
            let mut snapshot = snapshot.clone();
            let l0_len_before = snapshot.l0_sstables.len();
            snapshot
                .l0_sstables
                .retain(|sst_id| !l0_sstables.contains(sst_id));
            assert_eq!(
                l0_len_before - snapshot.l0_sstables.len(),
                l0_sstables.len()
            );
            assert_eq!(snapshot.levels[0].1, *l1_sstables);
            snapshot.levels[0].1 = output.to_vec();
            let obsolete_sst_ids = l0_sstables.iter().chain(l1_sstables).copied().collect();
            return (snapshot, obsolete_sst_ids);
        }
        match (self, task) {
            (CompactionController::Leveled(ctrl), CompactionTask::Leveled(task)) => {
                ctrl.apply_compaction_result(snapshot, task, output, in_recovery)
            }
            (CompactionController::Simple(ctrl), CompactionTask::Simple(task)) => {
                ctrl.apply_compaction_result(snapshot, task, output)
            }
            (CompactionController::Tiered(ctrl), CompactionTask::Tiered(task)) => {
                ctrl.apply_compaction_result(snapshot, task, output)
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
    fn compact(&self, task: &CompactionTask) -> Result<Vec<Arc<SsTable>>> {
        let snapshot = self.state.read().clone();
        match task {
            CompactionTask::ForceFullCompaction {
                l0_sstables,
                l1_sstables,
            } => {
                let mut input_iters = Vec::with_capacity(l0_sstables.len() + l1_sstables.len());
                for sst_id in l0_sstables.iter().chain(l1_sstables) {
                    let sst = snapshot.sstables.get(sst_id).unwrap().clone();
                    input_iters.push(Box::new(SsTableIterator::create_and_seek_to_first(sst)?));
                }
                let input = MergeIterator::create(input_iters);
                self.compact_generate_sst_from_iter(input, task.compact_to_bottom_level())
            }
            CompactionTask::Leveled(task) => {
                let lower_ssts = task
                    .lower_level_sst_ids
                    .iter()
                    .map(|sst_id| snapshot.sstables.get(sst_id).unwrap().clone())
                    .collect();
                let lower_iter = SstConcatIterator::create_and_seek_to_first(lower_ssts)?;

                if task.upper_level.is_none() {
                    let mut upper_iters = Vec::with_capacity(task.upper_level_sst_ids.len());
                    for sst_id in &task.upper_level_sst_ids {
                        let sst = snapshot.sstables.get(sst_id).unwrap().clone();
                        upper_iters.push(Box::new(SsTableIterator::create_and_seek_to_first(sst)?));
                    }
                    let upper_iter = MergeIterator::create(upper_iters);
                    let input = TwoMergeIterator::create(upper_iter, lower_iter)?;
                    self.compact_generate_sst_from_iter(input, task.is_lower_level_bottom_level)
                } else {
                    assert_eq!(task.upper_level_sst_ids.len(), 1);
                    let upper_sst = snapshot
                        .sstables
                        .get(&task.upper_level_sst_ids[0])
                        .unwrap()
                        .clone();
                    let upper_iter = SsTableIterator::create_and_seek_to_first(upper_sst)?;
                    let input = TwoMergeIterator::create(upper_iter, lower_iter)?;
                    self.compact_generate_sst_from_iter(input, task.is_lower_level_bottom_level)
                }
            }
            CompactionTask::Simple(task) => {
                let lower_ssts = task
                    .lower_level_sst_ids
                    .iter()
                    .map(|sst_id| snapshot.sstables.get(sst_id).unwrap().clone())
                    .collect();
                let lower_iter = SstConcatIterator::create_and_seek_to_first(lower_ssts)?;

                if task.upper_level.is_none() {
                    let mut upper_iters = Vec::with_capacity(task.upper_level_sst_ids.len());
                    for sst_id in &task.upper_level_sst_ids {
                        let sst = snapshot.sstables.get(sst_id).unwrap().clone();
                        upper_iters.push(Box::new(SsTableIterator::create_and_seek_to_first(sst)?));
                    }
                    let upper_iter = MergeIterator::create(upper_iters);
                    let input = TwoMergeIterator::create(upper_iter, lower_iter)?;
                    self.compact_generate_sst_from_iter(input, task.is_lower_level_bottom_level)
                } else {
                    let upper_ssts = task
                        .upper_level_sst_ids
                        .iter()
                        .map(|sst_id| snapshot.sstables.get(sst_id).unwrap().clone())
                        .collect();
                    let upper_iter = SstConcatIterator::create_and_seek_to_first(upper_ssts)?;
                    let input = TwoMergeIterator::create(upper_iter, lower_iter)?;
                    self.compact_generate_sst_from_iter(input, task.is_lower_level_bottom_level)
                }
            }
            CompactionTask::Tiered(task) => {
                let mut tier_iters = Vec::with_capacity(task.tiers.len());
                for (_, tier_sst_ids) in &task.tiers {
                    let tier_ssts = tier_sst_ids
                        .iter()
                        .map(|sst_id| snapshot.sstables.get(sst_id).unwrap().clone())
                        .collect();
                    tier_iters.push(Box::new(SstConcatIterator::create_and_seek_to_first(
                        tier_ssts,
                    )?));
                }
                let input = MergeIterator::create(tier_iters);
                self.compact_generate_sst_from_iter(input, task.bottom_tier_included)
            }
        }
    }

    fn compact_generate_sst_from_iter<I>(
        &self,
        mut input: I,
        compact_to_bottom_level: bool,
    ) -> Result<Vec<Arc<SsTable>>>
    where
        I: for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>,
    {
        let mut output = Vec::new();
        let mut builder = SsTableBuilder::new(self.options.block_size);
        let mut builder_has_entries = false;
        let mut last_user_key = Vec::new();
        let mut current_user_key = Vec::new();
        let mut kept_version_at_or_below_watermark = false;
        let mut filter_selected = false;
        let watermark = self.mvcc().watermark();
        let filters = self.compaction_filters.lock().clone();

        while input.is_valid() {
            if current_user_key.as_slice() != input.key().key_ref() {
                kept_version_at_or_below_watermark = false;
                filter_selected = false;
                current_user_key.clear();
                current_user_key.extend_from_slice(input.key().key_ref());
            }

            let mut matches_filter = false;
            for filter in &filters {
                match filter {
                    crate::lsm_storage::CompactionFilter::Prefix(prefix)
                        if input.key().key_ref().starts_with(prefix) =>
                    {
                        matches_filter = true
                    }
                    _ => {}
                }
            }
            let keep = if filter_selected {
                false
            } else if matches_filter && input.key().ts() <= watermark {
                filter_selected = true;
                false
            } else if input.key().ts() > watermark {
                true
            } else if kept_version_at_or_below_watermark {
                false
            } else {
                kept_version_at_or_below_watermark = true;
                !(compact_to_bottom_level && input.value().is_empty())
            };

            if keep {
                if builder_has_entries
                    && builder.estimated_size() >= self.options.target_sst_size
                    && last_user_key.as_slice() != input.key().key_ref()
                {
                    let sst_id = self.next_sst_id();
                    let sst = builder.build(
                        sst_id,
                        Some(self.block_cache.clone()),
                        self.path_of_sst(sst_id),
                    )?;
                    output.push(Arc::new(sst));
                    builder = SsTableBuilder::new(self.options.block_size);
                }
                builder.add(input.key(), input.value());
                builder_has_entries = true;
                last_user_key.clear();
                last_user_key.extend_from_slice(input.key().key_ref());
            }
            input.next()?;
        }

        if builder_has_entries {
            let sst_id = self.next_sst_id();
            let sst = builder.build(
                sst_id,
                Some(self.block_cache.clone()),
                self.path_of_sst(sst_id),
            )?;
            output.push(Arc::new(sst));
        }
        self.sync_dir()?;
        Ok(output)
    }

    pub fn force_full_compaction(&self) -> Result<()> {
        let task = {
            let snapshot = self.state.read();
            CompactionTask::ForceFullCompaction {
                l0_sstables: snapshot.l0_sstables.clone(),
                l1_sstables: snapshot.levels[0].1.clone(),
            }
        };
        let new_ssts = self.compact(&task)?;
        let new_sst_ids = new_ssts.iter().map(|sst| sst.sst_id()).collect::<Vec<_>>();

        let obsolete_sst_ids;
        {
            let state_lock = self.state_lock.lock();
            self.manifest.as_ref().unwrap().add_record(
                &state_lock,
                ManifestRecord::Compaction(task.clone(), new_sst_ids.clone()),
            )?;
            let mut state_guard = self.state.write();
            let mut current_snapshot = state_guard.as_ref().clone();
            for sst in &new_ssts {
                current_snapshot.sstables.insert(sst.sst_id(), sst.clone());
            }
            let (mut snapshot, obsolete) = self.compaction_controller.apply_compaction_result(
                &current_snapshot,
                &task,
                &new_sst_ids,
                false,
            );
            for sst_id in &obsolete {
                snapshot.sstables.remove(sst_id);
            }
            *state_guard = Arc::new(snapshot);
            obsolete_sst_ids = obsolete;
        }

        for sst_id in obsolete_sst_ids {
            std::fs::remove_file(self.path_of_sst(sst_id))?;
        }
        self.sync_dir()?;
        Ok(())
    }

    fn trigger_compaction(&self) -> Result<()> {
        let task = {
            let snapshot = self.state.read();
            let Some(task) = self
                .compaction_controller
                .generate_compaction_task(&snapshot)
            else {
                return Ok(());
            };
            task
        };

        let new_ssts = self.compact(&task)?;
        let new_sst_ids = new_ssts.iter().map(|sst| sst.sst_id()).collect::<Vec<_>>();
        let obsolete_sst_ids;
        {
            let state_lock = self.state_lock.lock();
            self.manifest.as_ref().unwrap().add_record(
                &state_lock,
                ManifestRecord::Compaction(task.clone(), new_sst_ids.clone()),
            )?;
            let mut state_guard = self.state.write();
            let mut current_snapshot = state_guard.as_ref().clone();
            for sst in &new_ssts {
                current_snapshot.sstables.insert(sst.sst_id(), sst.clone());
            }
            let (mut snapshot, obsolete) = self.compaction_controller.apply_compaction_result(
                &current_snapshot,
                &task,
                &new_sst_ids,
                false,
            );
            for sst_id in &obsolete {
                snapshot.sstables.remove(sst_id);
            }
            *state_guard = Arc::new(snapshot);
            obsolete_sst_ids = obsolete;
        }

        for sst_id in obsolete_sst_ids {
            std::fs::remove_file(self.path_of_sst(sst_id))?;
        }
        self.sync_dir()?;
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
                            eprintln!("compaction failed: {}", e);
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
        let should_flush = self.state.read().imm_memtables.len() >= self.options.num_memtable_limit;
        if should_flush {
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
                        eprintln!("flush failed: {}", e);
                    },
                    recv(rx) -> _ => return
                }
            }
        });
        Ok(Some(handle))
    }
}
