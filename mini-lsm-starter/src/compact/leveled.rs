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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        snapshot: &LsmStorageState,
        sst_ids: &[usize],
        in_level: usize,
    ) -> Vec<usize> {
        assert!(!sst_ids.is_empty());
        let first_sst = snapshot.sstables.get(&sst_ids[0]).unwrap();
        let mut first_key = first_sst.first_key().raw_ref();
        let mut last_key = first_sst.last_key().raw_ref();
        for sst_id in &sst_ids[1..] {
            let sst = snapshot.sstables.get(sst_id).unwrap();
            first_key = first_key.min(sst.first_key().raw_ref());
            last_key = last_key.max(sst.last_key().raw_ref());
        }

        let (level, level_sst_ids) = &snapshot.levels[in_level - 1];
        assert_eq!(*level, in_level);
        level_sst_ids
            .iter()
            .filter(|sst_id| {
                let sst = snapshot.sstables.get(sst_id).unwrap();
                sst.first_key().raw_ref() <= last_key && sst.last_key().raw_ref() >= first_key
            })
            .copied()
            .collect()
    }

    fn target_level_sizes(&self, snapshot: &LsmStorageState) -> Vec<u64> {
        assert_eq!(snapshot.levels.len(), self.options.max_levels);
        assert!(self.options.max_levels > 0);
        assert!(self.options.level_size_multiplier > 1);

        let base_level_size = self.options.base_level_size_mb as u64 * 1024 * 1024;
        let bottom_level_size = snapshot
            .levels
            .last()
            .unwrap()
            .1
            .iter()
            .fold(0u64, |size, sst_id| {
                size + snapshot.sstables.get(sst_id).unwrap().table_size()
            });
        let mut current_target = bottom_level_size.max(base_level_size);
        let mut target_sizes = vec![0; self.options.max_levels];
        target_sizes[self.options.max_levels - 1] = current_target;

        for level_idx in (0..self.options.max_levels - 1).rev() {
            if current_target <= base_level_size {
                break;
            }
            current_target /= self.options.level_size_multiplier as u64;
            if current_target == 0 {
                break;
            }
            target_sizes[level_idx] = current_target;
        }
        target_sizes
    }

    pub fn generate_compaction_task(
        &self,
        snapshot: &LsmStorageState,
    ) -> Option<LeveledCompactionTask> {
        let target_sizes = self.target_level_sizes(snapshot);
        let base_level = target_sizes.iter().position(|size| *size > 0).unwrap() + 1;

        if snapshot.l0_sstables.len() >= self.options.level0_file_num_compaction_trigger
            && !snapshot.l0_sstables.is_empty()
        {
            return Some(LeveledCompactionTask {
                upper_level: None,
                upper_level_sst_ids: snapshot.l0_sstables.clone(),
                lower_level: base_level,
                lower_level_sst_ids: self.find_overlapping_ssts(
                    snapshot,
                    &snapshot.l0_sstables,
                    base_level,
                ),
                is_lower_level_bottom_level: base_level == self.options.max_levels,
            });
        }

        let mut selected_level = None;
        let mut selected_size = 0u64;
        let mut selected_target = 1u64;
        for (level_idx, target_size) in target_sizes
            .iter()
            .enumerate()
            .take(self.options.max_levels - 1)
        {
            if *target_size == 0 {
                continue;
            }
            let current_size = snapshot.levels[level_idx]
                .1
                .iter()
                .map(|sst_id| snapshot.sstables.get(sst_id).unwrap().table_size())
                .sum::<u64>();
            if current_size <= *target_size {
                continue;
            }
            if selected_level.is_none()
                || (current_size as u128) * (selected_target as u128)
                    > (selected_size as u128) * (*target_size as u128)
            {
                selected_level = Some(level_idx);
                selected_size = current_size;
                selected_target = *target_size;
            }
        }

        let upper_level_idx = selected_level?;
        let upper_level = upper_level_idx + 1;
        let lower_level = upper_level + 1;
        let upper_sst_id = *snapshot.levels[upper_level_idx].1.iter().min().unwrap();
        let upper_level_sst_ids = vec![upper_sst_id];
        Some(LeveledCompactionTask {
            upper_level: Some(upper_level),
            lower_level_sst_ids: self.find_overlapping_ssts(
                snapshot,
                &upper_level_sst_ids,
                lower_level,
            ),
            upper_level_sst_ids,
            lower_level,
            is_lower_level_bottom_level: lower_level == self.options.max_levels,
        })
    }

    pub fn apply_compaction_result(
        &self,
        snapshot: &LsmStorageState,
        task: &LeveledCompactionTask,
        output: &[usize],
        in_recovery: bool,
    ) -> (LsmStorageState, Vec<usize>) {
        let mut snapshot = snapshot.clone();
        if let Some(upper_level) = task.upper_level {
            assert_eq!(task.lower_level, upper_level + 1);
            let (current_level, current_sst_ids) = &mut snapshot.levels[upper_level - 1];
            assert_eq!(*current_level, upper_level);
            let old_len = current_sst_ids.len();
            current_sst_ids.retain(|sst_id| !task.upper_level_sst_ids.contains(sst_id));
            assert_eq!(
                old_len - current_sst_ids.len(),
                task.upper_level_sst_ids.len()
            );
        } else {
            let old_len = snapshot.l0_sstables.len();
            snapshot
                .l0_sstables
                .retain(|sst_id| !task.upper_level_sst_ids.contains(sst_id));
            assert_eq!(
                old_len - snapshot.l0_sstables.len(),
                task.upper_level_sst_ids.len()
            );
        }

        let (current_lower_level, current_lower_sst_ids) =
            &mut snapshot.levels[task.lower_level - 1];
        assert_eq!(*current_lower_level, task.lower_level);
        let old_len = current_lower_sst_ids.len();
        current_lower_sst_ids.retain(|sst_id| !task.lower_level_sst_ids.contains(sst_id));
        assert_eq!(
            old_len - current_lower_sst_ids.len(),
            task.lower_level_sst_ids.len()
        );
        current_lower_sst_ids.extend_from_slice(output);
        if !in_recovery {
            current_lower_sst_ids.sort_by(|left, right| {
                snapshot
                    .sstables
                    .get(left)
                    .unwrap()
                    .first_key()
                    .cmp(snapshot.sstables.get(right).unwrap().first_key())
            });
        }

        let obsolete_sst_ids = task
            .upper_level_sst_ids
            .iter()
            .chain(&task.lower_level_sst_ids)
            .copied()
            .collect();
        (snapshot, obsolete_sst_ids)
    }
}
