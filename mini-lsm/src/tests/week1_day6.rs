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

use std::{ops::Bound, sync::Arc, time::Duration};

use bytes::Bytes;
use tempfile::tempdir;

use self::harness::{check_lsm_iter_result_by_key, sync};

use super::*;
use crate::{
    iterators::StorageIterator,
    lsm_storage::{LsmStorageInner, LsmStorageOptions, MiniLsm},
};

#[test]
fn test_task1_storage_scan() {
    let dir = tempdir().unwrap();
    let storage =
        Arc::new(LsmStorageInner::open(&dir, LsmStorageOptions::default_for_week1_test()).unwrap());
    storage.put(b"0", b"2333333").unwrap();
    storage.put(b"00", b"2333333").unwrap();
    storage.put(b"4", b"23").unwrap();
    sync(&storage);

    storage.delete(b"4").unwrap();
    sync(&storage);

    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    storage.put(b"00", b"2333").unwrap();
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    storage.put(b"3", b"23333").unwrap();
    storage.delete(b"1").unwrap();

    {
        let state = storage.state.read();
        assert_eq!(state.l0_sstables.len(), 2);
        assert_eq!(state.imm_memtables.len(), 2);
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
fn test_task1_storage_get() {
    let dir = tempdir().unwrap();
    let storage =
        Arc::new(LsmStorageInner::open(&dir, LsmStorageOptions::default_for_week1_test()).unwrap());
    storage.put(b"0", b"2333333").unwrap();
    storage.put(b"00", b"2333333").unwrap();
    storage.put(b"4", b"23").unwrap();
    sync(&storage);

    storage.delete(b"4").unwrap();
    sync(&storage);

    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    storage.put(b"00", b"2333").unwrap();
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    storage.put(b"3", b"23333").unwrap();
    storage.delete(b"1").unwrap();

    {
        let state = storage.state.read();
        assert_eq!(state.l0_sstables.len(), 2);
        assert_eq!(state.imm_memtables.len(), 2);
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

#[test]
fn test_task2_auto_flush() {
    let dir = tempdir().unwrap();
    let storage = MiniLsm::open(&dir, LsmStorageOptions::default_for_week1_day6_test()).unwrap();

    let value = "1".repeat(1024); // 1KB

    // approximately 6MB
    for i in 0..6000 {
        storage
            .put(format!("{i}").as_bytes(), value.as_bytes())
            .unwrap();
    }

    std::thread::sleep(Duration::from_millis(500));

    assert!(!storage.inner.state.read().l0_sstables.is_empty());
}

#[test]
fn test_task3_sst_filter() {
    let dir = tempdir().unwrap();
    let storage =
        Arc::new(LsmStorageInner::open(&dir, LsmStorageOptions::default_for_week1_test()).unwrap());

    for i in 1..=10000 {
        if i % 1000 == 0 {
            sync(&storage);
        }
        storage
            .put(format!("{:05}", i).as_bytes(), b"2333333")
            .unwrap();
    }

    let iter = storage.scan(Bound::Unbounded, Bound::Unbounded).unwrap();
    assert!(
        iter.num_active_iterators() >= 10,
        "did you implement num_active_iterators? current active iterators = {}",
        iter.num_active_iterators()
    );
    let max_num = iter.num_active_iterators();
    let iter = storage
        .scan(
            Bound::Excluded(format!("{:05}", 10000).as_bytes()),
            Bound::Unbounded,
        )
        .unwrap();
    assert!(iter.num_active_iterators() < max_num);
    let min_num = iter.num_active_iterators();
    let iter = storage
        .scan(
            Bound::Unbounded,
            Bound::Excluded(format!("{:05}", 1).as_bytes()),
        )
        .unwrap();
    assert_eq!(iter.num_active_iterators(), min_num);
    let iter = storage
        .scan(
            Bound::Unbounded,
            Bound::Included(format!("{:05}", 0).as_bytes()),
        )
        .unwrap();
    assert_eq!(iter.num_active_iterators(), min_num);
    let iter = storage
        .scan(
            Bound::Included(format!("{:05}", 10001).as_bytes()),
            Bound::Unbounded,
        )
        .unwrap();
    assert_eq!(iter.num_active_iterators(), min_num);
    let iter = storage
        .scan(
            Bound::Included(format!("{:05}", 5000).as_bytes()),
            Bound::Excluded(format!("{:05}", 6000).as_bytes()),
        )
        .unwrap();
    assert!(min_num <= iter.num_active_iterators() && iter.num_active_iterators() < max_num);
}
