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

use std::{ops::Bound, path::Path, sync::Arc};

use self::harness::{check_iter_result_by_key, check_lsm_iter_result_by_key, sync};
use bytes::Bytes;
use tempfile::tempdir;
use week2_day1::harness::construct_merge_iterator_over_storage;

use super::*;
use crate::{
    iterators::{concat_iterator::SstConcatIterator, StorageIterator},
    key::{KeySlice, TS_ENABLED},
    lsm_storage::{LsmStorageInner, LsmStorageOptions},
    table::{SsTable, SsTableBuilder},
};

#[test]
fn test_task1_full_compaction() {
    // We do not use LSM iterator in this test because it's implemented as part of task 3
    let dir = tempdir().unwrap();
    let storage =
        Arc::new(LsmStorageInner::open(&dir, LsmStorageOptions::default_for_week1_test()).unwrap());
    #[allow(clippy::let_unit_value)]
    let _txn = storage.new_txn().unwrap();
    storage.put(b"0", b"v1").unwrap();
    sync(&storage);
    storage.put(b"0", b"v2").unwrap();
    storage.put(b"1", b"v2").unwrap();
    storage.put(b"2", b"v2").unwrap();
    sync(&storage);
    storage.delete(b"0").unwrap();
    storage.delete(b"2").unwrap();
    sync(&storage);
    assert_eq!(storage.state.read().l0_sstables.len(), 3);
    let mut iter = construct_merge_iterator_over_storage(&storage.state.read());
    if TS_ENABLED {
        check_iter_result_by_key(
            &mut iter,
            vec![
                (Bytes::from_static(b"0"), Bytes::from_static(b"")),
                (Bytes::from_static(b"0"), Bytes::from_static(b"v2")),
                (Bytes::from_static(b"0"), Bytes::from_static(b"v1")),
                (Bytes::from_static(b"1"), Bytes::from_static(b"v2")),
                (Bytes::from_static(b"2"), Bytes::from_static(b"")),
                (Bytes::from_static(b"2"), Bytes::from_static(b"v2")),
            ],
        );
    } else {
        check_iter_result_by_key(
            &mut iter,
            vec![
                (Bytes::from_static(b"0"), Bytes::from_static(b"")),
                (Bytes::from_static(b"1"), Bytes::from_static(b"v2")),
                (Bytes::from_static(b"2"), Bytes::from_static(b"")),
            ],
        );
    }
    storage.force_full_compaction().unwrap();
    assert!(storage.state.read().l0_sstables.is_empty());
    let mut iter = construct_merge_iterator_over_storage(&storage.state.read());
    if TS_ENABLED {
        check_iter_result_by_key(
            &mut iter,
            vec![
                (Bytes::from_static(b"0"), Bytes::from_static(b"")),
                (Bytes::from_static(b"0"), Bytes::from_static(b"v2")),
                (Bytes::from_static(b"0"), Bytes::from_static(b"v1")),
                (Bytes::from_static(b"1"), Bytes::from_static(b"v2")),
                (Bytes::from_static(b"2"), Bytes::from_static(b"")),
                (Bytes::from_static(b"2"), Bytes::from_static(b"v2")),
            ],
        );
    } else {
        check_iter_result_by_key(
            &mut iter,
            vec![(Bytes::from_static(b"1"), Bytes::from_static(b"v2"))],
        );
    }
    storage.put(b"0", b"v3").unwrap();
    storage.put(b"2", b"v3").unwrap();
    sync(&storage);
    storage.delete(b"1").unwrap();
    sync(&storage);
    let mut iter = construct_merge_iterator_over_storage(&storage.state.read());
    if TS_ENABLED {
        check_iter_result_by_key(
            &mut iter,
            vec![
                (Bytes::from_static(b"0"), Bytes::from_static(b"v3")),
                (Bytes::from_static(b"0"), Bytes::from_static(b"")),
                (Bytes::from_static(b"0"), Bytes::from_static(b"v2")),
                (Bytes::from_static(b"0"), Bytes::from_static(b"v1")),
                (Bytes::from_static(b"1"), Bytes::from_static(b"")),
                (Bytes::from_static(b"1"), Bytes::from_static(b"v2")),
                (Bytes::from_static(b"2"), Bytes::from_static(b"v3")),
                (Bytes::from_static(b"2"), Bytes::from_static(b"")),
                (Bytes::from_static(b"2"), Bytes::from_static(b"v2")),
            ],
        );
    } else {
        check_iter_result_by_key(
            &mut iter,
            vec![
                (Bytes::from_static(b"0"), Bytes::from_static(b"v3")),
                (Bytes::from_static(b"1"), Bytes::from_static(b"")),
                (Bytes::from_static(b"2"), Bytes::from_static(b"v3")),
            ],
        );
    }
    storage.force_full_compaction().unwrap();
    assert!(storage.state.read().l0_sstables.is_empty());
    let mut iter = construct_merge_iterator_over_storage(&storage.state.read());
    if TS_ENABLED {
        check_iter_result_by_key(
            &mut iter,
            vec![
                (Bytes::from_static(b"0"), Bytes::from_static(b"v3")),
                (Bytes::from_static(b"0"), Bytes::from_static(b"")),
                (Bytes::from_static(b"0"), Bytes::from_static(b"v2")),
                (Bytes::from_static(b"0"), Bytes::from_static(b"v1")),
                (Bytes::from_static(b"1"), Bytes::from_static(b"")),
                (Bytes::from_static(b"1"), Bytes::from_static(b"v2")),
                (Bytes::from_static(b"2"), Bytes::from_static(b"v3")),
                (Bytes::from_static(b"2"), Bytes::from_static(b"")),
                (Bytes::from_static(b"2"), Bytes::from_static(b"v2")),
            ],
        );
    } else {
        check_iter_result_by_key(
            &mut iter,
            vec![
                (Bytes::from_static(b"0"), Bytes::from_static(b"v3")),
                (Bytes::from_static(b"2"), Bytes::from_static(b"v3")),
            ],
        );
    }
}

fn generate_concat_sst(
    start_key: usize,
    end_key: usize,
    dir: impl AsRef<Path>,
    id: usize,
) -> SsTable {
    let mut builder = SsTableBuilder::new(128);
    for idx in start_key..end_key {
        let key = format!("{:05}", idx);
        builder.add(
            KeySlice::for_testing_from_slice_no_ts(key.as_bytes()),
            b"test",
        );
    }
    let path = dir.as_ref().join(format!("{id}.sst"));
    builder.build_for_test(path).unwrap()
}

#[test]
fn test_task2_concat_iterator() {
    let dir = tempdir().unwrap();
    let mut sstables = Vec::new();
    for i in 1..=10 {
        sstables.push(Arc::new(generate_concat_sst(
            i * 10,
            (i + 1) * 10,
            dir.path(),
            i,
        )));
    }
    for key in 0..120 {
        let iter = SstConcatIterator::create_and_seek_to_key(
            sstables.clone(),
            KeySlice::for_testing_from_slice_no_ts(format!("{:05}", key).as_bytes()),
        )
        .unwrap();
        if key < 10 {
            assert!(iter.is_valid());
            assert_eq!(iter.key().for_testing_key_ref(), b"00010");
        } else if key >= 110 {
            assert!(!iter.is_valid());
        } else {
            assert!(iter.is_valid());
            assert_eq!(
                iter.key().for_testing_key_ref(),
                format!("{:05}", key).as_bytes()
            );
        }
    }
    let iter = SstConcatIterator::create_and_seek_to_first(sstables.clone()).unwrap();
    assert!(iter.is_valid());
    assert_eq!(iter.key().for_testing_key_ref(), b"00010");
}

#[test]
fn test_task3_integration() {
    let dir = tempdir().unwrap();
    let storage =
        Arc::new(LsmStorageInner::open(&dir, LsmStorageOptions::default_for_week1_test()).unwrap());
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
    sync(&storage);
    storage.force_full_compaction().unwrap();

    assert!(storage.state.read().l0_sstables.is_empty());
    assert!(!storage.state.read().levels[0].1.is_empty());

    check_lsm_iter_result_by_key(
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
