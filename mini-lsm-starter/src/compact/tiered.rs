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

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::lsm_storage::LsmStorageState;

#[derive(Debug, Serialize, Deserialize)]
pub struct TieredCompactionTask {
    pub tiers: Vec<(usize, Vec<usize>)>,
    pub bottom_tier_included: bool,
}

#[derive(Debug, Clone)]
pub struct TieredCompactionOptions {
    pub num_tiers: usize,
    pub max_size_amplification_percent: usize,
    pub size_ratio: usize,
    pub min_merge_width: usize,
    pub max_merge_width: Option<usize>,
}

pub struct TieredCompactionController {
    options: TieredCompactionOptions,
}

impl TieredCompactionController {
    pub fn new(options: TieredCompactionOptions) -> Self {
        Self { options }
    }

    pub fn generate_compaction_task(
        &self,
        snapshot: &LsmStorageState,
    ) -> Option<TieredCompactionTask> {
        // only trigger tasks when the number of tiers (sorted runs) is larger than num_tiers
        if snapshot.levels.len() < self.options.num_tiers {
            return None;
        }

        // Triggered by Space Amplification Ratio
        let mut all_leves_but_last_size = 0;
        for l in 0..snapshot.levels.len() - 1 {
            all_leves_but_last_size += &snapshot.levels[l].1.len();
        }
        let last_size = snapshot.levels.last().unwrap().1.len();

        if all_leves_but_last_size as f64 / last_size as f64
            >= self.options.max_size_amplification_percent as f64 / 100_f64
        {
            return Some(TieredCompactionTask {
                tiers: snapshot.levels.clone(),
                bottom_tier_included: true,
            });
        }

        // Triggered by Size Ratio
        let mut pre_size = 0;
        for l in 0..snapshot.levels.len() - 1 {
            pre_size += &snapshot.levels[l].1.len();
            let ratio = pre_size as f64 / snapshot.levels[l + 1].1.len() as f64;

            if l + 2 >= self.options.min_merge_width
                && ratio >= (100_f64 + self.options.size_ratio as f64) / 100_f64
            {
                return Some(TieredCompactionTask {
                    tiers: snapshot.levels[0..l + 1 + 1].to_vec(),
                    bottom_tier_included: l + 2 >= snapshot.levels.len(),
                });
            };
        }
        // Reduce Sorted Runs
        let top_most = snapshot.levels.len() - self.options.num_tiers + 1;

        Some(TieredCompactionTask {
            tiers: snapshot.levels[0..top_most + 1].to_vec(),
            bottom_tier_included: top_most + 1 >= snapshot.levels.len(),
        })
    }

    pub fn apply_compaction_result(
        &self,
        snapshot: &LsmStorageState,
        task: &TieredCompactionTask,
        output: &[usize],
    ) -> (LsmStorageState, Vec<usize>) {
        assert!(
            snapshot.l0_sstables.is_empty(),
            "should not add l0 ssts in tiered compaction"
        );
        let mut snapshot = snapshot.clone();
        let mut tier_to_remove = task
            .tiers
            .iter()
            .map(|(x, y)| (*x, y))
            .collect::<HashMap<_, _>>();
        let mut levels = Vec::new();
        let mut new_tier_added = false;
        let mut files_to_remove = Vec::new();

        for (tier_id, sstids) in &snapshot.levels {
            // might have new files added to sstables.levels when flush_immtables, so rm by id
            if let Some(f) = tier_to_remove.remove(tier_id) {
                // the tier should be removed
                assert_eq!(f, sstids, "file changed after issuing compaction task");
                files_to_remove.extend(f.iter());
            } else {
                // retain the tier
                levels.push((*tier_id, sstids.clone()));
            }
            if tier_to_remove.is_empty() && !new_tier_added {
                // tricky one, insert the new generated tier to the LSM tree, this may be in the middle of the LSM tree
                new_tier_added = true;
                // use the first output SST id as the level/tier id for new sorted run
                levels.push((output[0], output.to_vec()));
            }
        }
        if !tier_to_remove.is_empty() {
            unreachable!("some tiers not found??");
        }
        snapshot.levels = levels;

        (snapshot, files_to_remove)
    }
}
