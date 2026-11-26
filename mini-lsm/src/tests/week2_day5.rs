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

use std::time::Duration;

use bytes::BufMut;
use tempfile::tempdir;

use crate::{
    compact::{
        CompactionOptions, LeveledCompactionOptions, SimpleLeveledCompactionOptions,
        TieredCompactionOptions,
    },
    lsm_storage::{LsmStorageInner, LsmStorageOptions, MiniLsm},
    tests::harness::dump_files_in_dir,
};

#[test]
fn test_integration_leveled() {
    test_integration(CompactionOptions::Leveled(LeveledCompactionOptions {
        level_size_multiplier: 2,
        level0_file_num_compaction_trigger: 2,
        max_levels: 3,
        base_level_size_mb: 1,
    }))
}

#[test]
fn test_integration_tiered() {
    test_integration(CompactionOptions::Tiered(TieredCompactionOptions {
        num_tiers: 3,
        max_size_amplification_percent: 200,
        size_ratio: 1,
        min_merge_width: 3,
        max_merge_width: None,
    }))
}

#[test]
fn test_integration_simple() {
    test_integration(CompactionOptions::Simple(SimpleLeveledCompactionOptions {
        size_ratio_percent: 200,
        level0_file_num_compaction_trigger: 2,
        max_levels: 3,
    }));
}

/// Provision the storage such that base_level contains 2 SST files (target size is 2MB and each SST is 1MB).
/// This configuration has the effect that compaction will generate a new lower-level containing more than 1 SST files,
/// and leveled compaction should handle this situation correctly: These files might not be sorted by first-key and
/// should NOT be sorted inside the `apply_compaction_result` function, because we don't have any actual SST loaded at the
/// point where this function is called during manifest recovery.
#[test]
fn test_multiple_compacted_ssts_leveled() {
    let compaction_options = CompactionOptions::Leveled(LeveledCompactionOptions {
        level_size_multiplier: 4,
        level0_file_num_compaction_trigger: 2,
        max_levels: 2,
        base_level_size_mb: 2,
    });

    let lsm_storage_options = LsmStorageOptions::default_for_week2_test(compaction_options.clone());

    let dir = tempdir().unwrap();
    let storage = MiniLsm::open(&dir, lsm_storage_options).unwrap();

    // Insert approximately 10MB of data to ensure that at least one compaction is triggered by priority.
    // Insert 500 key-value pairs where each pair is 2KB
    for i in 0..500 {
        let (key, val) = key_value_pair_with_target_size(i, 20 * 1024);
        storage.put(&key, &val).unwrap();
    }

    let mut prev_snapshot = storage.inner.state.read().clone();
    while {
        std::thread::sleep(Duration::from_secs(1));
        let snapshot = storage.inner.state.read().clone();
        let to_cont = prev_snapshot.levels != snapshot.levels
            || prev_snapshot.l0_sstables != snapshot.l0_sstables;
        prev_snapshot = snapshot;
        to_cont
    } {
        println!("waiting for compaction to converge");
    }

    storage.close().unwrap();
    assert!(storage.inner.state.read().memtable.is_empty());
    assert!(storage.inner.state.read().imm_memtables.is_empty());

    storage.dump_structure();
    drop(storage);
    dump_files_in_dir(&dir);

    let storage = MiniLsm::open(
        &dir,
        LsmStorageOptions::default_for_week2_test(compaction_options.clone()),
    )
    .unwrap();

    for i in 0..500 {
        let (key, val) = key_value_pair_with_target_size(i, 20 * 1024);
        assert_eq!(&storage.get(&key).unwrap().unwrap()[..], &val);
    }
}

/// A crash can happen after the compaction record is written to the manifest but before the
/// compacted-away SSTs are deleted, so recovery must remove them.
#[test]
fn test_recovery_removes_obsolete_ssts() {
    let dir = tempdir().unwrap();
    // flush two L0 SSTs without compaction running, so that their ids are stable
    let storage = MiniLsm::open(
        &dir,
        LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction),
    )
    .unwrap();
    storage.put(b"0", b"v1").unwrap();
    storage.force_flush().unwrap();
    storage.put(b"1", b"v1").unwrap();
    storage.force_flush().unwrap();
    let obsolete_sst_ids = storage.inner.state.read().l0_sstables.clone();
    assert_eq!(obsolete_sst_ids.len(), 2);
    storage.close().unwrap();
    drop(storage);

    // reopen with compaction enabled so that the two L0 SSTs get compacted away
    let options = LsmStorageOptions::default_for_week2_test(CompactionOptions::Simple(
        SimpleLeveledCompactionOptions {
            size_ratio_percent: 200,
            level0_file_num_compaction_trigger: 2,
            max_levels: 3,
        },
    ));
    let storage = MiniLsm::open(&dir, options.clone()).unwrap();
    let mut prev_snapshot = storage.inner.state.read().clone();
    while {
        std::thread::sleep(Duration::from_secs(1));
        let snapshot = storage.inner.state.read().clone();
        let to_cont = prev_snapshot.levels != snapshot.levels
            || prev_snapshot.l0_sstables != snapshot.l0_sstables;
        prev_snapshot = snapshot;
        to_cont
    } {
        println!("waiting for compaction to converge");
    }
    assert!(storage.inner.state.read().l0_sstables.is_empty());
    storage.close().unwrap();
    drop(storage);

    // simulate a crash between the manifest write and the file removal
    for id in &obsolete_sst_ids {
        let path = LsmStorageInner::path_of_sst_static(&dir, *id);
        std::fs::write(&path, b"obsolete").unwrap();
    }
    dump_files_in_dir(&dir);

    let storage = MiniLsm::open(&dir, options).unwrap();
    for id in &obsolete_sst_ids {
        let path = LsmStorageInner::path_of_sst_static(&dir, *id);
        assert!(
            !path.exists(),
            "obsolete SST {} was not removed during recovery",
            id
        );
    }
    assert_eq!(&storage.get(b"0").unwrap().unwrap()[..], b"v1".as_slice());
    assert_eq!(&storage.get(b"1").unwrap().unwrap()[..], b"v1".as_slice());
}

fn test_integration(compaction_options: CompactionOptions) {
    let dir = tempdir().unwrap();
    let storage = MiniLsm::open(
        &dir,
        LsmStorageOptions::default_for_week2_test(compaction_options.clone()),
    )
    .unwrap();
    for i in 0..=20 {
        storage.put(b"0", format!("v{}", i).as_bytes()).unwrap();
        if i % 2 == 0 {
            storage.put(b"1", format!("v{}", i).as_bytes()).unwrap();
        } else {
            storage.delete(b"1").unwrap();
        }
        if i % 2 == 1 {
            storage.put(b"2", format!("v{}", i).as_bytes()).unwrap();
        } else {
            storage.delete(b"2").unwrap();
        }
        storage
            .inner
            .force_freeze_memtable(&storage.inner.state_lock.lock())
            .unwrap();
    }
    storage.close().unwrap();
    // ensure all SSTs are flushed
    assert!(storage.inner.state.read().memtable.is_empty());
    assert!(storage.inner.state.read().imm_memtables.is_empty());
    storage.dump_structure();
    drop(storage);
    dump_files_in_dir(&dir);

    let storage = MiniLsm::open(
        &dir,
        LsmStorageOptions::default_for_week2_test(compaction_options.clone()),
    )
    .unwrap();
    assert_eq!(&storage.get(b"0").unwrap().unwrap()[..], b"v20".as_slice());
    assert_eq!(&storage.get(b"1").unwrap().unwrap()[..], b"v20".as_slice());
    assert_eq!(storage.get(b"2").unwrap(), None);
}

/// Create a key value pair where key and value are of target size in bytes
fn key_value_pair_with_target_size(seed: i32, target_size_byte: usize) -> (Vec<u8>, Vec<u8>) {
    let mut key = vec![0; target_size_byte - 4];
    key.put_i32(seed);

    let mut val = vec![0; target_size_byte - 4];
    val.put_i32(seed);

    (key, val)
}
