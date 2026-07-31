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

use serde::{Deserialize, Serialize};

use crate::lsm_storage::LsmStorageState;

#[derive(Debug, Clone)]
pub struct SimpleLeveledCompactionOptions {
    pub size_ratio_percent: usize,
    pub level0_file_num_compaction_trigger: usize,
    pub max_levels: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        snapshot: &LsmStorageState,
    ) -> Option<SimpleLeveledCompactionTask> {
        assert_eq!(snapshot.levels.len(), self.options.max_levels);

        if !snapshot.l0_sstables.is_empty()
            && snapshot.l0_sstables.len() >= self.options.level0_file_num_compaction_trigger
        {
            return Some(SimpleLeveledCompactionTask {
                upper_level: None,
                upper_level_sst_ids: snapshot.l0_sstables.clone(),
                lower_level: 1,
                lower_level_sst_ids: snapshot.levels[0].1.clone(),
                is_lower_level_bottom_level: self.options.max_levels == 1,
            });
        }

        for upper_level_idx in 0..snapshot.levels.len().saturating_sub(1) {
            let (upper_level, upper_level_sst_ids) = &snapshot.levels[upper_level_idx];
            if upper_level_sst_ids.is_empty() {
                continue;
            }
            let (lower_level, lower_level_sst_ids) = &snapshot.levels[upper_level_idx + 1];
            let ratio_below_target = (lower_level_sst_ids.len() as u128) * 100
                < (upper_level_sst_ids.len() as u128) * (self.options.size_ratio_percent as u128);
            if ratio_below_target {
                return Some(SimpleLeveledCompactionTask {
                    upper_level: Some(*upper_level),
                    upper_level_sst_ids: upper_level_sst_ids.clone(),
                    lower_level: *lower_level,
                    lower_level_sst_ids: lower_level_sst_ids.clone(),
                    is_lower_level_bottom_level: *lower_level == self.options.max_levels,
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
        snapshot: &LsmStorageState,
        task: &SimpleLeveledCompactionTask,
        output: &[usize],
    ) -> (LsmStorageState, Vec<usize>) {
        let mut snapshot = snapshot.clone();
        if let Some(upper_level) = task.upper_level {
            assert_eq!(task.lower_level, upper_level + 1);
            let (current_level, current_sst_ids) = &mut snapshot.levels[upper_level - 1];
            assert_eq!(*current_level, upper_level);
            assert_eq!(*current_sst_ids, task.upper_level_sst_ids);
            current_sst_ids.clear();
        } else {
            assert_eq!(task.lower_level, 1);
            let l0_len_before = snapshot.l0_sstables.len();
            snapshot
                .l0_sstables
                .retain(|sst_id| !task.upper_level_sst_ids.contains(sst_id));
            assert_eq!(
                l0_len_before - snapshot.l0_sstables.len(),
                task.upper_level_sst_ids.len()
            );
        }

        let (current_lower_level, current_lower_sst_ids) =
            &mut snapshot.levels[task.lower_level - 1];
        assert_eq!(*current_lower_level, task.lower_level);
        assert_eq!(*current_lower_sst_ids, task.lower_level_sst_ids);
        current_lower_sst_ids.clear();
        current_lower_sst_ids.extend_from_slice(output);

        let obsolete_sst_ids = task
            .upper_level_sst_ids
            .iter()
            .chain(&task.lower_level_sst_ids)
            .copied()
            .collect();
        (snapshot, obsolete_sst_ids)
    }
}
