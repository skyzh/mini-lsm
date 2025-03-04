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
        let mut sum_rest = 0;

        if snapshot.levels.len() < self.options.num_tiers {
            return None;
        }

        let max_tier = snapshot.levels.len() - 1;

        // Space Amplification trigger
        if max_tier > 0 {
            for i in 0..max_tier {
                sum_rest += snapshot.levels[i].1.len();
            }
            if snapshot.levels[max_tier].1.len() * self.options.max_size_amplification_percent
                <= sum_rest * 100
            {
                println!("Space Amplification {:?}", max_tier);
                return Some({
                    TieredCompactionTask {
                        tiers: snapshot.levels.clone(),
                        bottom_tier_included: true,
                    }
                });
            }
        }

        // Size-ratio trigger
        sum_rest = 0;
        for i in 0..(max_tier + 1) {
            if i > 0
                && (i + 1) >= self.options.min_merge_width
                && sum_rest * (self.options.size_ratio + 100) < snapshot.levels[i].1.len() * 100
            {
                // println!("OPti {:?}", self.options);
                // println!("{:?}", i);
                // println!("Tiers {:?}", snapshot.levels);
                // println!("Len {:?}", snapshot.levels.len());
                return Some(TieredCompactionTask {
                    tiers: snapshot.levels[0..i + 1].to_vec(),
                    bottom_tier_included: i == max_tier,
                });
            }
            sum_rest += snapshot.levels[i].1.len();
        }

        println!("Default");
        Some(TieredCompactionTask {
            tiers: snapshot.levels.to_vec(),
            bottom_tier_included: true,
        })
    }

    pub fn apply_compaction_result(
        &self,
        snapshot: &LsmStorageState,
        task: &TieredCompactionTask,
        output: &[usize],
    ) -> (LsmStorageState, Vec<usize>) {
        let mut snapshot_copy = snapshot.clone();
        // find the first tier whose id matches this and then compress it
        for i in 0..snapshot.levels.len() {
            if task.tiers[0] == snapshot.levels[i] {
                snapshot_copy.levels.clear();
                snapshot_copy
                    .levels
                    .append(&mut snapshot.levels[..i].to_vec());
                snapshot_copy
                    .levels
                    .append(&mut snapshot.levels[i + task.tiers.len()..].to_vec());
                break;
            }
        }

        // Insert the compressed tier into the levels vector
        snapshot_copy.levels.insert(0, (output[0], output.to_vec()));

        let mut removal_vec = Vec::new();
        for tier in task.tiers.iter() {
            for table_id in tier.1.iter() {
                removal_vec.push(*table_id);
            }
        }
        (snapshot_copy, removal_vec)
    }
}
