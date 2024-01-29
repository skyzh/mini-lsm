use std::time::Duration;

use tempfile::tempdir;

use crate::{
    compact::CompactionOptions,
    lsm_storage::{LsmStorageOptions, MiniLsm},
    tests::harness::dump_files_in_dir,
};

#[test]
fn test_task3_compaction_integration() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.enable_wal = true;
    let storage = MiniLsm::open(&dir, options.clone()).unwrap();
    let _txn = storage.new_txn().unwrap();
    for i in 0..=20000 {
        storage
            .put(b"0", format!("{:02000}", i).as_bytes())
            .unwrap();
    }
    std::thread::sleep(Duration::from_secs(1)); // wait until all memtables flush
    while {
        let snapshot = storage.inner.state.read();
        !snapshot.imm_memtables.is_empty()
    } {
        storage.inner.force_flush_next_imm_memtable().unwrap();
    }
    assert!(storage.inner.state.read().l0_sstables.len() > 1);
    storage.force_full_compaction().unwrap();
    storage.dump_structure();
    dump_files_in_dir(&dir);
    assert!(storage.inner.state.read().l0_sstables.is_empty());
    assert_eq!(storage.inner.state.read().levels.len(), 1);
    // same key in the same SST
    assert_eq!(storage.inner.state.read().levels[0].1.len(), 1);
    for i in 0..=100 {
        storage
            .put(b"1", format!("{:02000}", i).as_bytes())
            .unwrap();
    }
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    std::thread::sleep(Duration::from_secs(1)); // wait until all memtables flush
    while {
        let snapshot = storage.inner.state.read();
        !snapshot.imm_memtables.is_empty()
    } {
        storage.inner.force_flush_next_imm_memtable().unwrap();
    }
    storage.force_full_compaction().unwrap();
    storage.dump_structure();
    dump_files_in_dir(&dir);
    assert!(storage.inner.state.read().l0_sstables.is_empty());
    assert_eq!(storage.inner.state.read().levels.len(), 1);
    // same key in the same SST, now we should split two
    assert_eq!(storage.inner.state.read().levels[0].1.len(), 2);
}
