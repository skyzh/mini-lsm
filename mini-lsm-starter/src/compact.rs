mod leveled;
mod simple_leveled;
mod tiered;

use crate::iterators::merge_iterator::MergeIterator;
use crate::iterators::StorageIterator;
use crate::table::SsTableBuilder;
use crate::table::SsTableIterator;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use std::vec;

use anyhow::Result;
pub use leveled::{LeveledCompactionController, LeveledCompactionOptions, LeveledCompactionTask};
use serde::{Deserialize, Serialize};
pub use simple_leveled::{
    SimpleLeveledCompactionController, SimpleLeveledCompactionOptions, SimpleLeveledCompactionTask,
};
pub use tiered::{TieredCompactionController, TieredCompactionOptions, TieredCompactionTask};

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
    ) -> (LsmStorageState, Vec<usize>) {
        match (self, task) {
            (CompactionController::Leveled(ctrl), CompactionTask::Leveled(task)) => {
                ctrl.apply_compaction_result(snapshot, task, output)
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
        // 1.use margeiterator to merge all sstables
        let mut sst_iters: Vec<Box<SsTableIterator>> = Vec::new();
        let state = self.state.read();
        match task {
            CompactionTask::ForceFullCompaction {
                l0_sstables,
                l1_sstables,
            } => {
                for l0_sst in l0_sstables {
                    let sst = state.sstables.get(l0_sst).unwrap().clone();
                    sst_iters.push(Box::new(
                        SsTableIterator::create_and_seek_to_first(sst).unwrap(),
                    ));
                }
                for l1_sst in l1_sstables {
                    let sst = state.sstables.get(l1_sst).unwrap().clone();
                    sst_iters.push(Box::new(
                        SsTableIterator::create_and_seek_to_first(sst).unwrap(),
                    ));
                }
            }
            _ => {}
        }

        let mut merge_iter = MergeIterator::create(sst_iters);

        // 2.write into new sstable by sst builder
        let mut sst_builder = SsTableBuilder::new(self.options.block_size);
        while merge_iter.is_valid() {
            if !merge_iter.value().is_empty() {
                sst_builder.add(merge_iter.key(), merge_iter.value());
            }
            merge_iter.next()?;
        }
        let sst_id = self.next_sst_id();
        let new_sst = sst_builder.build(
            sst_id,
            Some(self.block_cache.clone()),
            self.path_of_sst(sst_id),
        );

        Ok(vec![Arc::new(new_sst.unwrap())])
    }

    pub fn force_full_compaction(&self) -> Result<()> {
        let mut new_sst_l1 = vec![];
        let mut new_ssts = vec![];
        // 1. compact all sstables to new sstable
        {
            let state = self.state.read();
            new_ssts = self.compact(&CompactionTask::ForceFullCompaction {
                l0_sstables: state.l0_sstables.clone(),
                l1_sstables: state.levels[0].1.clone(),
            })?;
        }
        {
            let state_lock = self.state_lock.lock();
            let mut state_guard = self.state.write();
            let state = Arc::make_mut(&mut state_guard);

            // 2.clear old sstables
            for sst_id in state.l0_sstables.iter() {
                state.sstables.remove(sst_id);
            }
            for sst_id in state.levels[0].1.iter() {
                state.sstables.remove(sst_id);
            }
            state.l0_sstables.clear();

            // 3.replace  by new sstables
            for sst in new_ssts {
                new_sst_l1.push(sst.sst_id());
                state.sstables.insert(sst.sst_id(), sst);
            }
            state.levels[0] = (0, new_sst_l1); // new SSTs added to L1
        };
        Ok(())
    }

    fn trigger_compaction(&self) -> Result<()> {
        unimplemented!()
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
        if {
            let state = self.state.read();
            state.imm_memtables.len() >= self.options.num_memtable_limit
        } {
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
