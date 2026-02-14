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
        snapshot: &LsmStorageState,
        sst_ids: &[usize],
        in_level: usize,
    ) -> Vec<usize> {
        let first_key = sst_ids
            .iter()
            .map(|id| snapshot.sstables[id].first_key())
            .min()
            .unwrap();
        let last_key = sst_ids
            .iter()
            .map(|id| snapshot.sstables[id].last_key())
            .max()
            .unwrap();

        let mut ret = vec![];
        for sst_id in &snapshot.levels[in_level - 1].1 {
            let sst = &snapshot.sstables[sst_id];
            if !(sst.first_key() > last_key || sst.last_key() < first_key) {
                ret.push(*sst_id);
            }
        }

        ret
    }

    pub fn generate_compaction_task(
        &self,
        snapshot: &LsmStorageState,
    ) -> Option<LeveledCompactionTask> {
        // build target level size
        let mut target_sizes = vec![0; snapshot.levels.len()];
        let mut bottom_size = 0;
        snapshot.levels[snapshot.levels.len() - 1]
            .1
            .iter()
            .for_each(|i| bottom_size += snapshot.sstables[i].table_size());
        target_sizes[snapshot.levels.len() - 1] = bottom_size as usize;
        for i in (0..snapshot.levels.len() - 1).rev() {
            if target_sizes[i + 1] >= self.options.base_level_size_mb * (1 << 20) {
                target_sizes[i] = target_sizes[i + 1] / self.options.level_size_multiplier;
            }
        }
        // l0 first
        if snapshot.l0_sstables.len() >= self.options.level0_file_num_compaction_trigger {
            let mut lower_level = 0;
            for (i, item) in target_sizes.iter().enumerate() {
                if *item > 0 {
                    lower_level = i;
                    break;
                }
            }

            let lower_level_sst_ids =
                self.find_overlapping_ssts(snapshot, &snapshot.l0_sstables, lower_level + 1);
            return Some(LeveledCompactionTask {
                upper_level: None,
                upper_level_sst_ids: snapshot.l0_sstables.clone(),
                lower_level: lower_level + 1,
                lower_level_sst_ids,
                is_lower_level_bottom_level: lower_level + 1 == self.options.max_levels,
            });
        }

        // find max ratio
        let mut ratio_max = (0.0, 0_usize);
        for (i, &t) in target_sizes.iter().enumerate() {
            let mut size = 0;
            snapshot.levels[i]
                .1
                .iter()
                .for_each(|i| size += snapshot.sstables[i].table_size());
            let ratio = size as f64 / t as f64;
            if ratio > ratio_max.0 {
                ratio_max = (ratio, i);
            }
        }
        if ratio_max.0 <= 1.0 {
            return None;
        }

        let upper_level = ratio_max.1;
        // oldest sst in upper level
        let upper_sstid = *snapshot.levels[upper_level].1.iter().min().unwrap();

        Some(LeveledCompactionTask {
            upper_level: Some(upper_level + 1),
            upper_level_sst_ids: vec![upper_sstid],
            lower_level: upper_level + 1 + 1,
            lower_level_sst_ids: self.find_overlapping_ssts(
                snapshot,
                &[upper_sstid],
                upper_level + 1 + 1,
            ),
            is_lower_level_bottom_level: upper_level + 1 + 1 == self.options.max_levels,
        })
    }

    pub fn apply_compaction_result(
        &self,
        snapshot: &LsmStorageState,
        task: &LeveledCompactionTask,
        new_sst_ids: &[usize],
        _in_recovery: bool,
    ) -> (LsmStorageState, Vec<usize>) {
        let mut snapshot = snapshot.clone();

        match task.upper_level {
            // might have new l0 insert into snashot.l0_sstables during compaction
            None => snapshot
                .l0_sstables
                .retain(|x| !task.upper_level_sst_ids.contains(x)),
            Some(u) => snapshot.levels[u - 1]
                .1
                .retain(|x| !task.upper_level_sst_ids.contains(x)),
        };
        snapshot.levels[task.lower_level - 1]
            .1
            .retain(|x| !task.lower_level_sst_ids.contains(x));
        snapshot.levels[task.lower_level - 1].1.extend(new_sst_ids);
        snapshot.levels[task.lower_level - 1]
            .1
            .sort_by_key(|x| snapshot.sstables[x].first_key());

        let mut rm_ids = task.upper_level_sst_ids.clone();
        rm_ids.extend(task.lower_level_sst_ids.clone());

        (snapshot, rm_ids)
    }
}
