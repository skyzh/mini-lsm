use std::ops::Bound;

use bytes::Bytes;
use tempfile::tempdir;

use self::harness::check_iter_result;

use super::*;
use crate::lsm_storage::{LsmStorageInner, LsmStorageOptions};

fn sync(storage: &LsmStorageInner) {
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    storage.force_flush_next_imm_memtable().unwrap();
}

#[test]
fn test_task3_integration() {
    let dir = tempdir().unwrap();
    let storage = LsmStorageInner::open(&dir, LsmStorageOptions::default_for_week1_test()).unwrap();
    storage.put(b"0", b"2333333").unwrap();
    storage.put(b"00", b"2333333").unwrap();
    storage.put(b"4", b"23").unwrap();
    sync(&storage);

    storage.delete(b"4").unwrap();
    sync(&storage);

    storage.force_full_compaction().unwrap();
    assert!(storage.state.read().l0_sstables.is_empty());
    assert!(!storage.state.read().levels[0].1.is_empty());

    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    sync(&storage);

    storage.put(b"00", b"2333").unwrap();
    storage.put(b"3", b"23333").unwrap();
    storage.delete(b"1").unwrap();
    // sync(&storage);
    // storage.force_full_compaction().unwrap();

    // assert!(storage.state.read().l0_sstables.is_empty());
    // assert!(!storage.state.read().levels[0].1.is_empty());

    check_iter_result(
        &mut storage.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("0"), Bytes::from("2333333")),
            (Bytes::from("00"), Bytes::from("2333")),
            (Bytes::from("2"), Bytes::from("2333")),
            (Bytes::from("3"), Bytes::from("23333")),
        ],
    );

    assert_eq!(
        storage.get(b"0").unwrap(),
        Some(Bytes::from_static(b"2333333"))
    );
    assert_eq!(
        storage.get(b"00").unwrap(),
        Some(Bytes::from_static(b"2333"))
    );
    assert_eq!(
        storage.get(b"2").unwrap(),
        Some(Bytes::from_static(b"2333"))
    );
    assert_eq!(
        storage.get(b"3").unwrap(),
        Some(Bytes::from_static(b"23333"))
    );
    assert_eq!(storage.get(b"4").unwrap(), None);
    assert_eq!(storage.get(b"--").unwrap(), None);
    assert_eq!(storage.get(b"555").unwrap(), None);
}
