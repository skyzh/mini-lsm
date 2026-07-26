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

use tempfile::tempdir;

use crate::{
    compact::{
        CompactionOptions, SimpleLeveledCompactionController, SimpleLeveledCompactionOptions,
        SimpleLeveledCompactionTask,
    },
    lsm_storage::{LsmStorageOptions, MiniLsm},
};

use super::harness::{check_compaction_ratio, compaction_bench};

#[test]
fn test_integration() {
    let dir = tempdir().unwrap();
    let storage = MiniLsm::open(
        &dir,
        LsmStorageOptions::default_for_week2_test(CompactionOptions::Simple(
            SimpleLeveledCompactionOptions {
                level0_file_num_compaction_trigger: 2,
                max_levels: 3,
                size_ratio_percent: 200,
            },
        )),
    )
    .unwrap();

    compaction_bench(storage.clone());
    check_compaction_ratio(storage.clone());
}

#[test]
fn test_l0_compaction_preserves_newer_ssts_in_order() {
    let options = SimpleLeveledCompactionOptions {
        level0_file_num_compaction_trigger: 2,
        max_levels: 3,
        size_ratio_percent: 200,
    };
    let controller = SimpleLeveledCompactionController::new(options.clone());
    let dir = tempdir().unwrap();
    let storage = MiniLsm::open(
        &dir,
        LsmStorageOptions::default_for_week2_test(CompactionOptions::Simple(options)),
    )
    .unwrap();
    let mut snapshot = storage.inner.state.read().as_ref().clone();
    snapshot.l0_sstables = vec![7, 6, 5, 4];
    let task = SimpleLeveledCompactionTask {
        upper_level: None,
        upper_level_sst_ids: vec![5, 4],
        lower_level: 1,
        lower_level_sst_ids: vec![],
        is_lower_level_bottom_level: false,
    };

    let (result, removed) = controller.apply_compaction_result(&snapshot, &task, &[]);
    assert_eq!(result.l0_sstables, vec![7, 6]);
    assert_eq!(removed, vec![5, 4]);
}
