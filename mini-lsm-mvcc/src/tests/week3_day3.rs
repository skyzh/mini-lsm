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

use std::ops::Bound;

use bytes::Bytes;
use tempfile::tempdir;

use crate::{
    compact::CompactionOptions,
    key::KeySlice,
    lsm_storage::{LsmStorageOptions, MiniLsm},
    table::SsTableBuilder,
    tests::harness::check_lsm_iter_result_by_key,
};

#[test]
fn test_task2_memtable_mvcc() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.enable_wal = true;
    let storage = MiniLsm::open(&dir, options.clone()).unwrap();
    storage.put(b"a", b"1").unwrap();
    storage.put(b"b", b"1").unwrap();
    let snapshot1 = storage.new_txn().unwrap();
    storage.put(b"a", b"2").unwrap();
    let snapshot2 = storage.new_txn().unwrap();
    storage.delete(b"b").unwrap();
    storage.put(b"c", b"1").unwrap();
    let snapshot3 = storage.new_txn().unwrap();
    assert_eq!(snapshot1.get(b"a").unwrap(), Some(Bytes::from_static(b"1")));
    assert_eq!(snapshot1.get(b"b").unwrap(), Some(Bytes::from_static(b"1")));
    assert_eq!(snapshot1.get(b"c").unwrap(), None);
    check_lsm_iter_result_by_key(
        &mut snapshot1.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("1")),
            (Bytes::from("b"), Bytes::from("1")),
        ],
    );
    check_lsm_iter_result_by_key(
        &mut snapshot1
            .scan(Bound::Excluded(b"a"), Bound::Excluded(b"b"))
            .unwrap(),
        vec![],
    );
    assert_eq!(snapshot2.get(b"a").unwrap(), Some(Bytes::from_static(b"2")));
    assert_eq!(snapshot2.get(b"b").unwrap(), Some(Bytes::from_static(b"1")));
    assert_eq!(snapshot2.get(b"c").unwrap(), None);
    check_lsm_iter_result_by_key(
        &mut snapshot2.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("2")),
            (Bytes::from("b"), Bytes::from("1")),
        ],
    );
    check_lsm_iter_result_by_key(
        &mut snapshot2
            .scan(Bound::Excluded(b"a"), Bound::Excluded(b"b"))
            .unwrap(),
        vec![],
    );
    assert_eq!(snapshot3.get(b"a").unwrap(), Some(Bytes::from_static(b"2")));
    assert_eq!(snapshot3.get(b"b").unwrap(), None);
    assert_eq!(snapshot3.get(b"c").unwrap(), Some(Bytes::from_static(b"1")));
    check_lsm_iter_result_by_key(
        &mut snapshot3.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("2")),
            (Bytes::from("c"), Bytes::from("1")),
        ],
    );
    check_lsm_iter_result_by_key(
        &mut snapshot3
            .scan(Bound::Excluded(b"a"), Bound::Excluded(b"c"))
            .unwrap(),
        vec![],
    );
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.put(b"a", b"3").unwrap();
    storage.put(b"b", b"3").unwrap();
    let snapshot4 = storage.new_txn().unwrap();
    storage.put(b"a", b"4").unwrap();
    let snapshot5 = storage.new_txn().unwrap();
    storage.delete(b"b").unwrap();
    storage.put(b"c", b"5").unwrap();
    let snapshot6 = storage.new_txn().unwrap();
    assert_eq!(snapshot1.get(b"a").unwrap(), Some(Bytes::from_static(b"1")));
    assert_eq!(snapshot1.get(b"b").unwrap(), Some(Bytes::from_static(b"1")));
    assert_eq!(snapshot1.get(b"c").unwrap(), None);
    check_lsm_iter_result_by_key(
        &mut snapshot1.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("1")),
            (Bytes::from("b"), Bytes::from("1")),
        ],
    );
    check_lsm_iter_result_by_key(
        &mut snapshot1
            .scan(Bound::Excluded(b"a"), Bound::Excluded(b"b"))
            .unwrap(),
        vec![],
    );
    assert_eq!(snapshot2.get(b"a").unwrap(), Some(Bytes::from_static(b"2")));
    assert_eq!(snapshot2.get(b"b").unwrap(), Some(Bytes::from_static(b"1")));
    assert_eq!(snapshot2.get(b"c").unwrap(), None);
    check_lsm_iter_result_by_key(
        &mut snapshot2.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("2")),
            (Bytes::from("b"), Bytes::from("1")),
        ],
    );
    check_lsm_iter_result_by_key(
        &mut snapshot2
            .scan(Bound::Excluded(b"a"), Bound::Excluded(b"b"))
            .unwrap(),
        vec![],
    );
    assert_eq!(snapshot3.get(b"a").unwrap(), Some(Bytes::from_static(b"2")));
    assert_eq!(snapshot3.get(b"b").unwrap(), None);
    assert_eq!(snapshot3.get(b"c").unwrap(), Some(Bytes::from_static(b"1")));
    check_lsm_iter_result_by_key(
        &mut snapshot3.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("2")),
            (Bytes::from("c"), Bytes::from("1")),
        ],
    );
    check_lsm_iter_result_by_key(
        &mut snapshot3
            .scan(Bound::Excluded(b"a"), Bound::Excluded(b"c"))
            .unwrap(),
        vec![],
    );
    assert_eq!(snapshot4.get(b"a").unwrap(), Some(Bytes::from_static(b"3")));
    assert_eq!(snapshot4.get(b"b").unwrap(), Some(Bytes::from_static(b"3")));
    assert_eq!(snapshot4.get(b"c").unwrap(), Some(Bytes::from_static(b"1")));
    check_lsm_iter_result_by_key(
        &mut snapshot4.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("3")),
            (Bytes::from("b"), Bytes::from("3")),
            (Bytes::from("c"), Bytes::from("1")),
        ],
    );
    check_lsm_iter_result_by_key(
        &mut snapshot4
            .scan(Bound::Excluded(b"a"), Bound::Excluded(b"c"))
            .unwrap(),
        vec![(Bytes::from("b"), Bytes::from("3"))],
    );
    assert_eq!(snapshot5.get(b"a").unwrap(), Some(Bytes::from_static(b"4")));
    assert_eq!(snapshot5.get(b"b").unwrap(), Some(Bytes::from_static(b"3")));
    assert_eq!(snapshot5.get(b"c").unwrap(), Some(Bytes::from_static(b"1")));
    check_lsm_iter_result_by_key(
        &mut snapshot5.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("4")),
            (Bytes::from("b"), Bytes::from("3")),
            (Bytes::from("c"), Bytes::from("1")),
        ],
    );
    check_lsm_iter_result_by_key(
        &mut snapshot5
            .scan(Bound::Excluded(b"a"), Bound::Excluded(b"c"))
            .unwrap(),
        vec![(Bytes::from("b"), Bytes::from("3"))],
    );
    assert_eq!(snapshot6.get(b"a").unwrap(), Some(Bytes::from_static(b"4")));
    assert_eq!(snapshot6.get(b"b").unwrap(), None);
    assert_eq!(snapshot6.get(b"c").unwrap(), Some(Bytes::from_static(b"5")));
    check_lsm_iter_result_by_key(
        &mut snapshot6.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("4")),
            (Bytes::from("c"), Bytes::from("5")),
        ],
    );
    check_lsm_iter_result_by_key(
        &mut snapshot6
            .scan(Bound::Excluded(b"a"), Bound::Excluded(b"c"))
            .unwrap(),
        vec![],
    );
}

#[test]
fn test_task2_lsm_iterator_mvcc() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.enable_wal = true;
    let storage = MiniLsm::open(&dir, options.clone()).unwrap();
    storage.put(b"a", b"1").unwrap();
    storage.put(b"b", b"1").unwrap();
    let snapshot1 = storage.new_txn().unwrap();
    storage.put(b"a", b"2").unwrap();
    let snapshot2 = storage.new_txn().unwrap();
    storage.delete(b"b").unwrap();
    storage.put(b"c", b"1").unwrap();
    let snapshot3 = storage.new_txn().unwrap();
    storage.force_flush().unwrap();
    assert_eq!(snapshot1.get(b"a").unwrap(), Some(Bytes::from_static(b"1")));
    assert_eq!(snapshot1.get(b"b").unwrap(), Some(Bytes::from_static(b"1")));
    assert_eq!(snapshot1.get(b"c").unwrap(), None);
    check_lsm_iter_result_by_key(
        &mut snapshot1.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("1")),
            (Bytes::from("b"), Bytes::from("1")),
        ],
    );
    assert_eq!(snapshot2.get(b"a").unwrap(), Some(Bytes::from_static(b"2")));
    assert_eq!(snapshot2.get(b"b").unwrap(), Some(Bytes::from_static(b"1")));
    assert_eq!(snapshot2.get(b"c").unwrap(), None);
    check_lsm_iter_result_by_key(
        &mut snapshot2.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("2")),
            (Bytes::from("b"), Bytes::from("1")),
        ],
    );
    assert_eq!(snapshot3.get(b"a").unwrap(), Some(Bytes::from_static(b"2")));
    assert_eq!(snapshot3.get(b"b").unwrap(), None);
    assert_eq!(snapshot3.get(b"c").unwrap(), Some(Bytes::from_static(b"1")));
    check_lsm_iter_result_by_key(
        &mut snapshot3.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("2")),
            (Bytes::from("c"), Bytes::from("1")),
        ],
    );
    storage.put(b"a", b"3").unwrap();
    storage.put(b"b", b"3").unwrap();
    let snapshot4 = storage.new_txn().unwrap();
    storage.put(b"a", b"4").unwrap();
    let snapshot5 = storage.new_txn().unwrap();
    storage.delete(b"b").unwrap();
    storage.put(b"c", b"5").unwrap();
    let snapshot6 = storage.new_txn().unwrap();
    storage.force_flush().unwrap();
    assert_eq!(snapshot1.get(b"a").unwrap(), Some(Bytes::from_static(b"1")));
    assert_eq!(snapshot1.get(b"b").unwrap(), Some(Bytes::from_static(b"1")));
    assert_eq!(snapshot1.get(b"c").unwrap(), None);
    check_lsm_iter_result_by_key(
        &mut snapshot1.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("1")),
            (Bytes::from("b"), Bytes::from("1")),
        ],
    );
    assert_eq!(snapshot2.get(b"a").unwrap(), Some(Bytes::from_static(b"2")));
    assert_eq!(snapshot2.get(b"b").unwrap(), Some(Bytes::from_static(b"1")));
    assert_eq!(snapshot2.get(b"c").unwrap(), None);
    check_lsm_iter_result_by_key(
        &mut snapshot2.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("2")),
            (Bytes::from("b"), Bytes::from("1")),
        ],
    );
    assert_eq!(snapshot3.get(b"a").unwrap(), Some(Bytes::from_static(b"2")));
    assert_eq!(snapshot3.get(b"b").unwrap(), None);
    assert_eq!(snapshot3.get(b"c").unwrap(), Some(Bytes::from_static(b"1")));
    check_lsm_iter_result_by_key(
        &mut snapshot3.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("2")),
            (Bytes::from("c"), Bytes::from("1")),
        ],
    );
    assert_eq!(snapshot4.get(b"a").unwrap(), Some(Bytes::from_static(b"3")));
    assert_eq!(snapshot4.get(b"b").unwrap(), Some(Bytes::from_static(b"3")));
    assert_eq!(snapshot4.get(b"c").unwrap(), Some(Bytes::from_static(b"1")));
    check_lsm_iter_result_by_key(
        &mut snapshot4.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("3")),
            (Bytes::from("b"), Bytes::from("3")),
            (Bytes::from("c"), Bytes::from("1")),
        ],
    );
    assert_eq!(snapshot5.get(b"a").unwrap(), Some(Bytes::from_static(b"4")));
    assert_eq!(snapshot5.get(b"b").unwrap(), Some(Bytes::from_static(b"3")));
    assert_eq!(snapshot5.get(b"c").unwrap(), Some(Bytes::from_static(b"1")));
    check_lsm_iter_result_by_key(
        &mut snapshot5.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("4")),
            (Bytes::from("b"), Bytes::from("3")),
            (Bytes::from("c"), Bytes::from("1")),
        ],
    );
    assert_eq!(snapshot6.get(b"a").unwrap(), Some(Bytes::from_static(b"4")));
    assert_eq!(snapshot6.get(b"b").unwrap(), None);
    assert_eq!(snapshot6.get(b"c").unwrap(), Some(Bytes::from_static(b"5")));
    check_lsm_iter_result_by_key(
        &mut snapshot6.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("a"), Bytes::from("4")),
            (Bytes::from("c"), Bytes::from("5")),
        ],
    );
    check_lsm_iter_result_by_key(
        &mut snapshot6
            .scan(Bound::Included(b"a"), Bound::Included(b"a"))
            .unwrap(),
        vec![(Bytes::from("a"), Bytes::from("4"))],
    );
    check_lsm_iter_result_by_key(
        &mut snapshot6
            .scan(Bound::Excluded(b"a"), Bound::Excluded(b"c"))
            .unwrap(),
        vec![],
    );
}

#[test]
fn test_task3_sst_ts() {
    let mut builder = SsTableBuilder::new(16);
    builder.add(KeySlice::for_testing_from_slice_with_ts(b"11", 1), b"11");
    builder.add(KeySlice::for_testing_from_slice_with_ts(b"22", 2), b"22");
    builder.add(KeySlice::for_testing_from_slice_with_ts(b"33", 3), b"11");
    builder.add(KeySlice::for_testing_from_slice_with_ts(b"44", 4), b"22");
    builder.add(KeySlice::for_testing_from_slice_with_ts(b"55", 5), b"11");
    builder.add(KeySlice::for_testing_from_slice_with_ts(b"66", 6), b"22");
    let dir = tempdir().unwrap();
    let sst = builder.build_for_test(dir.path().join("1.sst")).unwrap();
    assert_eq!(sst.max_ts(), 6);
}
