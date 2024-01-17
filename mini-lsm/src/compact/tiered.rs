use std::collections::HashMap;

use crate::lsm_storage::LsmStorageInner;

pub struct TieredCompactionTask {
    pub tiers: Vec<(usize, Vec<usize>)>,
}

pub struct TieredCompactionOptions {
    pub level0_file_num_compaction_trigger: usize,
    pub max_size_amplification_percent: usize,
    pub size_ratio: usize,
    pub min_merge_width: usize,
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
        snapshot: &LsmStorageInner,
    ) -> Option<TieredCompactionTask> {
        assert!(
            snapshot.l0_sstables.is_empty(),
            "should not add l0 ssts in tiered compaction"
        );
        if snapshot.levels.len() < self.options.level0_file_num_compaction_trigger {
            return None;
        }
        // compaction triggered by space amplification ratio
        let mut size = 0;
        for id in 0..(snapshot.levels.len() - 1) {
            size += snapshot.levels[id].1.len();
        }
        let space_amp_ratio =
            (size as f64) / (snapshot.levels.last().unwrap().1.len() as f64) * 100.0;
        if space_amp_ratio >= self.options.max_size_amplification_percent as f64 {
            println!(
                "compaction triggered by space amplification ratio: {}",
                space_amp_ratio
            );
            return Some(TieredCompactionTask {
                tiers: snapshot.levels.clone(),
            });
        }
        let size_ratio_trigger = (100.0 + self.options.size_ratio as f64) / 100.0;
        // compaction triggered by size ratio
        let mut size = 0;
        for id in 0..(snapshot.levels.len() - 1) {
            size += snapshot.levels[id].1.len();
            let next_level_size = snapshot.levels[id + 1].1.len();
            let current_size_ratio = size as f64 / next_level_size as f64;
            if current_size_ratio >= size_ratio_trigger && id + 2 >= self.options.min_merge_width {
                println!(
                    "compaction triggered by size ratio: {}",
                    current_size_ratio * 100.0
                );
                return Some(TieredCompactionTask {
                    tiers: snapshot
                        .levels
                        .iter()
                        .take(id + 2)
                        .cloned()
                        .collect::<Vec<_>>(),
                });
            }
        }
        // trying to reduce sorted runs without respecting size ratio
        let num_tiers_to_take =
            snapshot.levels.len() - self.options.level0_file_num_compaction_trigger + 1;
        println!("compaction triggered by reducing sorted runs");
        return Some(TieredCompactionTask {
            tiers: snapshot
                .levels
                .iter()
                .take(num_tiers_to_take)
                .cloned()
                .collect::<Vec<_>>(),
        });
    }

    pub fn apply_compaction_result(
        &self,
        snapshot: &LsmStorageInner,
        task: &TieredCompactionTask,
        output: &[usize],
    ) -> (LsmStorageInner, Vec<usize>) {
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
        for (tier_id, files) in &snapshot.levels {
            if let Some(ffiles) = tier_to_remove.remove(tier_id) {
                // the tier should be removed
                assert_eq!(ffiles, files, "file changed after issuing compaction task");
                files_to_remove.extend(ffiles.iter().copied());
            } else {
                // retain the tier
                levels.push((*tier_id, files.clone()));
            }
            if tier_to_remove.is_empty() && !new_tier_added {
                // add the compacted tier to the LSM tree
                new_tier_added = true;
                levels.push((output[0], output.to_vec()));
            }
        }
        snapshot.levels = levels;
        (snapshot, files_to_remove)
    }
}
