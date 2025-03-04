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
        let begin_key = sst_ids
            .iter()
            .map(|id| snapshot.sstables[id].first_key())
            .cloned()
            .min()
            .unwrap();
        let end_key = sst_ids
            .iter()
            .map(|id| snapshot.sstables[id].last_key())
            .cloned()
            .max()
            .unwrap();
        let mut overlapping_ssts = Vec::new();
        for sst_id in &snapshot.levels[in_level].1 {
            let sstable = &snapshot.sstables[sst_id];
            if !(sstable.first_key() > &end_key || sstable.last_key() < &begin_key) {
                overlapping_ssts.push(*sst_id);
            }
        }
        overlapping_ssts
    }

    pub fn generate_compaction_task(
        &self,
        snapshot: &LsmStorageState,
    ) -> Option<LeveledCompactionTask> {
        // Initialize the Target Size and Real Size Arrays
        let max_levels = self.options.max_levels;
        let mut target_size = vec![0; max_levels];
        let real_size: Vec<u64> = snapshot
            .levels
            .iter()
            .map(|(_, ids)| {
                ids.iter()
                    .map(|id| snapshot.sstables[id].table_size())
                    .sum::<u64>()
                    / (1024 * 1024)
            })
            .collect();

        target_size[max_levels - 1] = real_size[max_levels - 1] as usize;

        (0..max_levels - 1).rev().for_each(|i| {
            if target_size[i + 1] > self.options.base_level_size_mb {
                target_size[i] = target_size[i + 1] / self.options.level_size_multiplier
            } else {
                target_size[i] = 0
            }
        });

        let lower_level = target_size
            .iter()
            .enumerate() // Enumerate to get both index and value
            .find(|(_, &i)| i > 0) // Find the first element where i > 0
            .map(|(index, _)| index) // Map to the index part of the tuple
            .unwrap_or(max_levels - 1);

        if snapshot.l0_sstables.len() >= self.options.level0_file_num_compaction_trigger {
            println!("Compaction Generation");
            println!("L0 sst compaction {:?}", lower_level);
            return Some(LeveledCompactionTask {
                upper_level: None,
                upper_level_sst_ids: snapshot.l0_sstables.clone(),
                lower_level: (lower_level + 1),
                lower_level_sst_ids: self.find_overlapping_ssts(
                    snapshot,
                    &snapshot.l0_sstables.clone(),
                    lower_level,
                ),
                is_lower_level_bottom_level: lower_level == max_levels - 1,
            });
        }

        let mut max_pr: f64 = 0.0;
        let mut max_ind = max_levels;
        for i in lower_level..max_levels - 1 {
            let iter_pr = real_size[i] as f64 / (target_size[i] as f64);
            if iter_pr > 1.0 && max_pr < iter_pr {
                max_pr = iter_pr;
                max_ind = i;
            }
        }

        if max_ind != max_levels {
            // Find the sst that is the oldest in the set
            let oldie = vec![*snapshot.levels[max_ind].1.iter().min().unwrap()];
            println!("Compaction Generation");
            return Some(LeveledCompactionTask {
                upper_level: Some(max_ind + 1),
                lower_level_sst_ids: self.find_overlapping_ssts(snapshot, &oldie, max_ind + 1),
                upper_level_sst_ids: oldie,
                lower_level: max_ind + 2,
                is_lower_level_bottom_level: max_ind == max_levels - 2,
            });
        }

        None
    }

    fn remove_id_from_level(&self, level_data: &mut Vec<usize>, id: usize) {
        level_data
            .iter()
            .position(|&val| val == id)
            .map(|pos| level_data.remove(pos));
    }

    pub fn apply_compaction_result(
        &self,
        snapshot: &LsmStorageState,
        task: &LeveledCompactionTask,
        output: &[usize],
        in_recovery: bool,
    ) -> (LsmStorageState, Vec<usize>) {
        let mut new_snapshot = snapshot.clone();
        let mut rem = Vec::new();
        rem.extend(task.upper_level_sst_ids.clone());
        rem.extend(task.lower_level_sst_ids.clone());
        let lower_level = task.lower_level - 1_usize;
        let upper_level = task.upper_level;

        if let Some(level) = upper_level {
            self.remove_id_from_level(
                &mut new_snapshot.levels[level - 1].1,
                task.upper_level_sst_ids[0],
            );
        } else {
            for ids in task.upper_level_sst_ids.iter() {
                self.remove_id_from_level(&mut new_snapshot.l0_sstables, *ids);
            }
        }

        for ids in task.lower_level_sst_ids.iter() {
            self.remove_id_from_level(&mut new_snapshot.levels[lower_level].1, *ids);
        }

        new_snapshot.levels[lower_level].1.extend(output);

        if !in_recovery {
            new_snapshot.levels[lower_level].1.sort_by(|x, y| {
                new_snapshot.sstables[x]
                    .first_key()
                    .cmp(new_snapshot.sstables[y].first_key())
            });
        }

        (new_snapshot, rem)
    }
}
