use std::ops::Bound;
use std::sync::Arc;

use self::harness::{check_iter_result_by_key, MockIterator};
use self::harness::{check_lsm_iter_result_by_key, generate_sst};
use bytes::Bytes;
use tempfile::tempdir;

use super::*;
use crate::{
    iterators::two_merge_iterator::TwoMergeIterator,
    lsm_storage::{LsmStorageInner, LsmStorageOptions},
};

#[test]
fn test_task1_merge_1() {
    let i1 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.1")),
        (Bytes::from("b"), Bytes::from("2.1")),
        (Bytes::from("c"), Bytes::from("3.1")),
    ]);
    let i2 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.2")),
        (Bytes::from("b"), Bytes::from("2.2")),
        (Bytes::from("c"), Bytes::from("3.2")),
        (Bytes::from("d"), Bytes::from("4.2")),
    ]);
    let mut iter = TwoMergeIterator::create(i1, i2).unwrap();
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("a"), Bytes::from("1.1")),
            (Bytes::from("b"), Bytes::from("2.1")),
            (Bytes::from("c"), Bytes::from("3.1")),
            (Bytes::from("d"), Bytes::from("4.2")),
        ],
    )
}

#[test]
fn test_task1_merge_2() {
    let i2 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.1")),
        (Bytes::from("b"), Bytes::from("2.1")),
        (Bytes::from("c"), Bytes::from("3.1")),
    ]);
    let i1 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.2")),
        (Bytes::from("b"), Bytes::from("2.2")),
        (Bytes::from("c"), Bytes::from("3.2")),
        (Bytes::from("d"), Bytes::from("4.2")),
    ]);
    let mut iter = TwoMergeIterator::create(i1, i2).unwrap();
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("a"), Bytes::from("1.2")),
            (Bytes::from("b"), Bytes::from("2.2")),
            (Bytes::from("c"), Bytes::from("3.2")),
            (Bytes::from("d"), Bytes::from("4.2")),
        ],
    )
}

#[test]
fn test_task1_merge_3() {
    let i2 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.1")),
        (Bytes::from("b"), Bytes::from("2.1")),
        (Bytes::from("c"), Bytes::from("3.1")),
    ]);
    let i1 = MockIterator::new(vec![
        (Bytes::from("b"), Bytes::from("2.2")),
        (Bytes::from("c"), Bytes::from("3.2")),
        (Bytes::from("d"), Bytes::from("4.2")),
    ]);
    let mut iter = TwoMergeIterator::create(i1, i2).unwrap();
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("a"), Bytes::from("1.1")),
            (Bytes::from("b"), Bytes::from("2.2")),
            (Bytes::from("c"), Bytes::from("3.2")),
            (Bytes::from("d"), Bytes::from("4.2")),
        ],
    )
}

#[test]
fn test_task1_merge_4() {
    let i2 = MockIterator::new(vec![]);
    let i1 = MockIterator::new(vec![
        (Bytes::from("b"), Bytes::from("2.2")),
        (Bytes::from("c"), Bytes::from("3.2")),
        (Bytes::from("d"), Bytes::from("4.2")),
    ]);
    let mut iter = TwoMergeIterator::create(i1, i2).unwrap();
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("b"), Bytes::from("2.2")),
            (Bytes::from("c"), Bytes::from("3.2")),
            (Bytes::from("d"), Bytes::from("4.2")),
        ],
    );
    let i1 = MockIterator::new(vec![]);
    let i2 = MockIterator::new(vec![
        (Bytes::from("b"), Bytes::from("2.2")),
        (Bytes::from("c"), Bytes::from("3.2")),
        (Bytes::from("d"), Bytes::from("4.2")),
    ]);
    let mut iter = TwoMergeIterator::create(i1, i2).unwrap();
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("b"), Bytes::from("2.2")),
            (Bytes::from("c"), Bytes::from("3.2")),
            (Bytes::from("d"), Bytes::from("4.2")),
        ],
    );
}

#[test]
fn test_task1_merge_5() {
    let i2 = MockIterator::new(vec![]);
    let i1 = MockIterator::new(vec![]);
    let mut iter = TwoMergeIterator::create(i1, i2).unwrap();
    check_iter_result_by_key(&mut iter, vec![])
}

#[test]
fn test_task2_storage_scan() {
    let dir = tempdir().unwrap();
    let storage =
        Arc::new(LsmStorageInner::open(&dir, LsmStorageOptions::default_for_week1_test()).unwrap());
    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    storage.put(b"00", b"2333").unwrap();
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    storage.put(b"3", b"23333").unwrap();
    storage.delete(b"1").unwrap();
    let sst1 = generate_sst(
        10,
        dir.path().join("10.sst"),
        vec![
            (Bytes::from_static(b"0"), Bytes::from_static(b"2333333")),
            (Bytes::from_static(b"00"), Bytes::from_static(b"2333333")),
            (Bytes::from_static(b"4"), Bytes::from_static(b"23")),
        ],
        Some(storage.block_cache.clone()),
    );
    let sst2 = generate_sst(
        11,
        dir.path().join("11.sst"),
        vec![(Bytes::from_static(b"4"), Bytes::from_static(b""))],
        Some(storage.block_cache.clone()),
    );
    {
        let mut state = storage.state.write();
        let mut snapshot = state.as_ref().clone();
        snapshot.l0_sstables.push(sst2.sst_id()); // this is the latest SST
        snapshot.l0_sstables.push(sst1.sst_id());
        snapshot.sstables.insert(sst2.sst_id(), sst2.into());
        snapshot.sstables.insert(sst1.sst_id(), sst1.into());
        *state = snapshot.into();
    }
    check_lsm_iter_result_by_key(
        &mut storage.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("0"), Bytes::from("2333333")),
            (Bytes::from("00"), Bytes::from("2333")),
            (Bytes::from("2"), Bytes::from("2333")),
            (Bytes::from("3"), Bytes::from("23333")),
        ],
    );
    check_lsm_iter_result_by_key(
        &mut storage
            .scan(Bound::Included(b"1"), Bound::Included(b"2"))
            .unwrap(),
        vec![(Bytes::from("2"), Bytes::from("2333"))],
    );
    check_lsm_iter_result_by_key(
        &mut storage
            .scan(Bound::Excluded(b"1"), Bound::Excluded(b"3"))
            .unwrap(),
        vec![(Bytes::from("2"), Bytes::from("2333"))],
    );
}

#[test]
fn test_task3_storage_get() {
    let dir = tempdir().unwrap();
    let storage =
        Arc::new(LsmStorageInner::open(&dir, LsmStorageOptions::default_for_week1_test()).unwrap());
    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    storage.put(b"00", b"2333").unwrap();
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    storage.put(b"3", b"23333").unwrap();
    storage.delete(b"1").unwrap();
    let sst1 = generate_sst(
        10,
        dir.path().join("10.sst"),
        vec![
            (Bytes::from_static(b"0"), Bytes::from_static(b"2333333")),
            (Bytes::from_static(b"00"), Bytes::from_static(b"2333333")),
            (Bytes::from_static(b"4"), Bytes::from_static(b"23")),
        ],
        Some(storage.block_cache.clone()),
    );
    let sst2 = generate_sst(
        11,
        dir.path().join("11.sst"),
        vec![(Bytes::from_static(b"4"), Bytes::from_static(b""))],
        Some(storage.block_cache.clone()),
    );
    {
        let mut state = storage.state.write();
        let mut snapshot = state.as_ref().clone();
        snapshot.l0_sstables.push(sst2.sst_id()); // this is the latest SST
        snapshot.l0_sstables.push(sst1.sst_id());
        snapshot.sstables.insert(sst2.sst_id(), sst2.into());
        snapshot.sstables.insert(sst1.sst_id(), sst1.into());
        *state = snapshot.into();
    }
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
