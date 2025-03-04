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

use serde::{Deserialize, Serialize};

use crate::lsm_storage::LsmStorageState;

#[derive(Debug, Clone)]
pub struct SimpleLeveledCompactionOptions {
    pub size_ratio_percent: usize,
    pub level0_file_num_compaction_trigger: usize,
    pub max_levels: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimpleLeveledCompactionTask {
    // if upper_level is `None`, then it is L0 compaction
    pub upper_level: Option<usize>,
    pub upper_level_sst_ids: Vec<usize>,
    pub lower_level: usize,
    pub lower_level_sst_ids: Vec<usize>,
    pub is_lower_level_bottom_level: bool,
}

pub struct SimpleLeveledCompactionController {
    options: SimpleLeveledCompactionOptions,
}

impl SimpleLeveledCompactionController {
    pub fn new(options: SimpleLeveledCompactionOptions) -> Self {
        Self { options }
    }

    /// Generates a compaction task.
    ///
    /// Returns `None` if no compaction needs to be scheduled. The order of SSTs in the compaction task id vector matters.
    pub fn generate_compaction_task(
        &self,
        _snapshot: &LsmStorageState,
    ) -> Option<SimpleLeveledCompactionTask> {
        if _snapshot.l0_sstables.len() >= self.options.level0_file_num_compaction_trigger
            && self.options.size_ratio_percent * _snapshot.l0_sstables.len()
                > 100 * _snapshot.levels[0].1.len()
        {
            return Some(SimpleLeveledCompactionTask {
                upper_level: None,
                upper_level_sst_ids: _snapshot.l0_sstables.clone(),
                lower_level: 1,
                lower_level_sst_ids: _snapshot.levels[0].1.clone(),
                is_lower_level_bottom_level: _snapshot.levels.len() == 1,
            });
        }
        for i in 0..(self.options.max_levels - 1) {
            if !_snapshot.levels[i].1.is_empty()
                && self.options.size_ratio_percent * _snapshot.levels[i].1.len()
                    > 100 * _snapshot.levels[i + 1].1.len()
            {
                return Some(SimpleLeveledCompactionTask {
                    upper_level: Some(i),
                    upper_level_sst_ids: _snapshot.levels[i].1.clone(),
                    lower_level: i + 1,
                    lower_level_sst_ids: _snapshot.levels[i + 1].1.clone(),
                    is_lower_level_bottom_level: (_snapshot.levels.len() == i + 1),
                });
            }
        }
        None
    }

    /// Apply the compaction result.
    ///
    /// The compactor will call this function with the compaction task and the list of SST ids generated. This function applies the
    /// result and generates a new LSM state. The functions should only change `l0_sstables` and `levels` without changing memtables
    /// and `sstables` hash map. Though there should only be one thread running compaction jobs, you should think about the case
    /// where an L0 SST gets flushed while the compactor generates new SSTs, and with that in mind, you should do some sanity checks
    /// in your implementation.
    pub fn apply_compaction_result(
        &self,
        _snapshot: &LsmStorageState,
        _task: &SimpleLeveledCompactionTask,
        _output: &[usize],
    ) -> (LsmStorageState, Vec<usize>) {
        let mut snapshot_copy = _snapshot.clone();
        if _task.upper_level.is_none() {
            snapshot_copy
                .l0_sstables
                .retain(|sst| !_task.upper_level_sst_ids.contains(sst));
            snapshot_copy.levels[0].1.clear();
            snapshot_copy.levels[0].1.extend(_output.to_vec());
        } else {
            snapshot_copy.levels[_task.upper_level.unwrap()]
                .1
                .retain(|sst| !_task.upper_level_sst_ids.contains(sst));
            snapshot_copy.levels[_task.upper_level.unwrap() + 1]
                .1
                .clear();
            snapshot_copy.levels[_task.upper_level.unwrap() + 1]
                .1
                .extend(_output.to_vec());
        }
        let removal_list = _task
            .upper_level_sst_ids
            .iter()
            .cloned()
            .chain(_task.lower_level_sst_ids.iter().cloned())
            .collect::<Vec<_>>();
        (snapshot_copy, removal_list)
    }
}
