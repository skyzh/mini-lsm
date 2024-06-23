use serde::{Deserialize, Serialize};

use crate::lsm_storage::LsmStorageState;

#[derive(Debug, Serialize, Deserialize)]
pub struct LeveledCompactionTask {
    // if upper_level is `None`, then it is L0 compaction
    pub upper_level: Option<usize>,
    pub upper_level_sst_ids: Vec<usize>,
    pub lower_level: usize,
    pub lower_level_sst_ids: Vec<usize>,
    pub is_lower_level_bottom_level: bool,
}

#[derive(Debug, Clone)]
pub struct LeveledCompactionOptions {
    pub level_size_multiplier: usize,
    pub level0_file_num_compaction_trigger: usize,
    pub max_levels: usize,
    pub base_level_size_mb: usize,
}

pub struct LeveledCompactionController {
    options: LeveledCompactionOptions,
}

impl LeveledCompactionController {
    pub fn new(options: LeveledCompactionOptions) -> Self {
        Self { options }
    }

    fn find_overlapping_ssts(
        &self,
        _snapshot: &LsmStorageState,
        _sst_ids: &[usize],
        _in_level: usize,
    ) -> Vec<usize> {
        unimplemented!()
    }

    pub fn generate_compaction_task(
        &self,
        _snapshot: &LsmStorageState,
    ) -> Option<LeveledCompactionTask> {
        unimplemented!()
    }

    pub fn apply_compaction_result(
        &self,
        _snapshot: &LsmStorageState,
        _task: &LeveledCompactionTask,
        _output: &[usize],
        _in_recovery: bool,
    ) -> (LsmStorageState, Vec<usize>) {
        unimplemented!()
    }
}
