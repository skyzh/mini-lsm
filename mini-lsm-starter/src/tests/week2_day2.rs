use tempfile::tempdir;

use crate::{
    compact::{CompactionOptions, SimpleLeveledCompactionOptions},
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
