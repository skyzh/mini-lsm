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
        assert!(
            snapshot.l0_sstables.is_empty(),
            "should not add l0 ssts in tiered compaction"
        );
        if snapshot.levels.len() < self.options.num_tiers {
            return None;
        }

        let bottom_size = snapshot.levels.last().unwrap().1.len();
        assert!(bottom_size > 0, "tier must contain at least one SST");
        let newer_size = snapshot.levels[..snapshot.levels.len() - 1]
            .iter()
            .map(|(_, sst_ids)| sst_ids.len())
            .sum::<usize>();
        if (newer_size as u128) * 100
            >= (bottom_size as u128) * (self.options.max_size_amplification_percent as u128)
        {
            return Some(TieredCompactionTask {
                tiers: snapshot.levels.clone(),
                bottom_tier_included: true,
            });
        }

        let mut newer_prefix_size = 0usize;
        for (tier_idx, (_, sst_ids)) in snapshot.levels.iter().enumerate() {
            if tier_idx > 0
                && tier_idx >= self.options.min_merge_width
                && (sst_ids.len() as u128) * 100
                    > (newer_prefix_size as u128) * ((100 + self.options.size_ratio) as u128)
            {
                return Some(TieredCompactionTask {
                    tiers: snapshot.levels[..tier_idx].to_vec(),
                    bottom_tier_included: false,
                });
            }
            newer_prefix_size += sst_ids.len();
        }

        let merge_width = self
            .options
            .max_merge_width
            .unwrap_or(snapshot.levels.len())
            .min(snapshot.levels.len());
        if merge_width < 2 {
            return None;
        }
        Some(TieredCompactionTask {
            tiers: snapshot.levels[..merge_width].to_vec(),
            bottom_tier_included: merge_width == snapshot.levels.len(),
        })
    }

    pub fn apply_compaction_result(
        &self,
        snapshot: &LsmStorageState,
        task: &TieredCompactionTask,
        output: &[usize],
    ) -> (LsmStorageState, Vec<usize>) {
        assert!(!task.tiers.is_empty());
        let mut snapshot = snapshot.clone();
        let first_tier_id = task.tiers[0].0;
        let first_tier_idx = snapshot
            .levels
            .iter()
            .position(|(tier_id, _)| *tier_id == first_tier_id)
            .expect("compaction input tier must still be present");
        let task_end = first_tier_idx + task.tiers.len();
        assert!(task_end <= snapshot.levels.len());
        assert_eq!(snapshot.levels[first_tier_idx..task_end], task.tiers);

        let obsolete_sst_ids = task
            .tiers
            .iter()
            .flat_map(|(_, sst_ids)| sst_ids.iter())
            .copied()
            .collect();
        snapshot.levels.drain(first_tier_idx..task_end);
        if let Some(first_output_sst_id) = output.first() {
            snapshot
                .levels
                .insert(first_tier_idx, (*first_output_sst_id, output.to_vec()));
        }
        (snapshot, obsolete_sst_ids)
    }
}
