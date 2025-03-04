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

use crate::iterators::concat_iterator::SstConcatIterator;
use crate::iterators::two_merge_iterator::TwoMergeIterator;
use crate::iterators::StorageIterator;

use crate::key::KeySlice;
use crate::manifest::ManifestRecord;
use crate::table::{SsTableBuilder, SsTableIterator};
use anyhow::Result;
use serde::{Deserialize, Serialize};

use std::sync::Arc;
use std::time::Duration;

pub use leveled::{LeveledCompactionController, LeveledCompactionOptions, LeveledCompactionTask};
pub use simple_leveled::{
    SimpleLeveledCompactionController, SimpleLeveledCompactionOptions, SimpleLeveledCompactionTask,
};
pub use tiered::{TieredCompactionController, TieredCompactionOptions, TieredCompactionTask};

use crate::iterators::merge_iterator::MergeIterator;
use crate::lsm_storage::{LsmStorageInner, LsmStorageState};
use crate::table::SsTable;

#[derive(Debug, Serialize, Deserialize)]
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
    fn merger<I: 'static + for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>>(
        &self,
        mut merge_iter: I,
    ) -> Result<Vec<Arc<SsTable>>> {
        let mut result_sst = Vec::with_capacity(10); // Preallocate some space
        let mut builder = SsTableBuilder::new(self.options.block_size);
        let mut prev_key: Option<Vec<u8>> = None;

        while merge_iter.is_valid() {
            let current_key = merge_iter.key().key_ref();

            if builder.estimated_size() >= self.options.target_sst_size
                && prev_key.as_deref() != Some(current_key)
            {
                let id = self.next_sst_id();
                result_sst.push(Arc::new(builder.build(
                    id,
                    Some(self.block_cache.clone()),
                    self.path_of_sst(id),
                )?));
                builder = SsTableBuilder::new(self.options.block_size);
            }

            builder.add(merge_iter.key(), merge_iter.value());
            prev_key = Some(current_key.to_vec());
            merge_iter.next()?;
        }

        if prev_key.is_some() {
            let id = self.next_sst_id();
            result_sst.push(Arc::new(builder.build(
                id,
                Some(self.block_cache.clone()),
                self.path_of_sst(id),
            )?));
        }

        Ok(result_sst)
    }

    fn merge_compact_l0_l1(
        &self,
        l0_sstables: &[usize],
        l1_sstables: &[usize],
    ) -> Result<Vec<Arc<SsTable>>> {
        let guard = {
            let temp_guard = self.state.read();
            temp_guard.clone()
        };

        let sst_iters_l0 = l0_sstables
            .iter()
            .map(|sst_id| {
                SsTableIterator::create_and_seek_to_first(guard.sstables[sst_id].clone())
                    .map(Box::new)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let ss_tables_l1 = l1_sstables
            .iter()
            .map(|sst_id| guard.sstables[sst_id].clone())
            .collect();

        let merge_iter = TwoMergeIterator::create(
            MergeIterator::create(sst_iters_l0),
            SstConcatIterator::create_and_seek_to_first(ss_tables_l1)?,
        )?;
        self.merger(merge_iter)
    }

    fn merge_compact(
        &self,
        table_upper: &[usize],
        table_lower: &[usize],
    ) -> Result<Vec<Arc<SsTable>>> {
        // Level 0 is highest
        let guard = { self.state.read().clone() };

        let ss_tables_upper = table_upper
            .iter()
            .map(|sst_id| guard.sstables[sst_id].clone())
            .collect();

        let ss_tables_lower = table_lower
            .iter()
            .map(|sst_id| guard.sstables[sst_id].clone())
            .collect();

        let merge_iter = TwoMergeIterator::create(
            SstConcatIterator::create_and_seek_to_first(ss_tables_upper)?,
            SstConcatIterator::create_and_seek_to_first(ss_tables_lower)?,
        )?;
        self.merger(merge_iter)
    }

    fn compact(&self, task: &CompactionTask) -> Result<Vec<Arc<SsTable>>> {
        match task {
            CompactionTask::ForceFullCompaction {
                l0_sstables,
                l1_sstables,
            } => self.merge_compact_l0_l1(l0_sstables, l1_sstables),
            CompactionTask::Simple(simple_task) => {
                if simple_task.upper_level.is_none() {
                    self.merge_compact_l0_l1(
                        &simple_task.upper_level_sst_ids,
                        &simple_task.lower_level_sst_ids,
                    )
                } else {
                    self.merge_compact(
                        &simple_task.upper_level_sst_ids,
                        &simple_task.lower_level_sst_ids,
                    )
                }
            }

            CompactionTask::Tiered(tiered_task) => {
                println!("Tiered Task {:?}", tiered_task);
                let guard = {
                    let temp_guard = self.state.read();
                    temp_guard.clone()
                };

                let iters_set = tiered_task
                    .tiers
                    .iter()
                    .map(|(_, tier)| {
                        let tier_ssts = tier
                            .iter()
                            .map(|sst_id| guard.sstables[sst_id].clone())
                            .collect();
                        SstConcatIterator::create_and_seek_to_first(tier_ssts).map(Box::new)
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                self.merger(MergeIterator::create(iters_set))
            }

            CompactionTask::Leveled(leveled_task) => {
                if leveled_task.upper_level.is_none() {
                    self.merge_compact_l0_l1(
                        &leveled_task.upper_level_sst_ids,
                        &leveled_task.lower_level_sst_ids,
                    )
                } else {
                    self.merge_compact(
                        &leveled_task.upper_level_sst_ids,
                        &leveled_task.lower_level_sst_ids,
                    )
                }
            }
        }
    }

    pub fn force_full_compaction(&self) -> Result<()> {
        let (sst_tables_l0, sst_tables_l1) = {
            let guard = self.state.read();
            (guard.l0_sstables.clone(), guard.levels[0].1.clone())
        };

        // Compacted SSTs
        let new_sst = self.compact(&CompactionTask::ForceFullCompaction {
            l0_sstables: (sst_tables_l0.clone()),
            l1_sstables: (sst_tables_l1.clone()),
        })?;

        {
            let _state_lock = self.state_lock.lock();
            let mut snapshot = self.state.read().as_ref().clone();

            // Remove only those L0 SSTables that were compacted
            snapshot.l0_sstables.retain(|x| !sst_tables_l0.contains(x));

            // Add the new SSTables to L1
            snapshot.levels[0].1 = new_sst.iter().map(|x| x.sst_id()).collect();

            // Remove the Old and Add the New SST ids
            for sst_ids in sst_tables_l0.iter().chain(sst_tables_l1.iter()) {
                snapshot.sstables.remove(sst_ids);
            }

            for (k, v) in snapshot.levels[0].1.iter().enumerate() {
                snapshot.sstables.insert(*v, new_sst[k].clone());
            }

            *self.state.write() = Arc::new(snapshot);
        }
        Ok(())
    }

    pub fn del_tables(
        sst_ids: Vec<usize>,
        snapshot: &mut LsmStorageState,
    ) -> Result<Vec<Arc<SsTable>>> {
        let mut deleted_tables = Vec::new();
        for sst_id in sst_ids.iter() {
            let res = snapshot.sstables.remove(sst_id);
            if res.is_none() {
                panic!("!Deletion Id not found in the table")
            }
            deleted_tables.push(res.unwrap());
        }
        Ok(deleted_tables)
    }

    fn trigger_compaction(&self) -> Result<()> {
        let snapshot_copy = {
            let guard = self.state.read();
            Arc::clone(&guard)
        };
        let task = self
            .compaction_controller
            .generate_compaction_task(&snapshot_copy);
        if task.is_none() {
            return Ok(());
        }

        let output = self.compact(task.as_ref().unwrap())?;
        let output_ids_vec: Vec<usize> = output.iter().map(|x| x.as_ref().sst_id()).collect();
        let output_ids: &[usize] = &output_ids_vec;
        let ssts_to_remove = {
            let state_lock = self.state_lock.lock();
            let mut snapshot_new = self.state.read().as_ref().clone();
            for i in 0..output.len() {
                snapshot_new
                    .sstables
                    .insert(output_ids[i], output[i].clone());
            }

            let (mut snapshot_new, del) = self.compaction_controller.apply_compaction_result(
                &snapshot_new,
                task.as_ref().unwrap(),
                output_ids,
                false,
            );
            let ssts_to_remove = LsmStorageInner::del_tables(del, &mut snapshot_new)?;

            let mut state = self.state.write();
            *state = Arc::new(snapshot_new);
            drop(state);

            self.manifest.as_ref().unwrap().add_record(
                &state_lock,
                ManifestRecord::Compaction(task.unwrap(), output_ids_vec.clone()),
            )?;

            ssts_to_remove
        };
        for table_id in ssts_to_remove {
            std::fs::remove_file(self.path_of_sst(table_id.sst_id()))?;
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
        let res = {
            let guard = self.state.read();
            guard.imm_memtables.len() >= self.options.num_memtable_limit
        };
        if res {
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
