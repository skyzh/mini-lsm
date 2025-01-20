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
    lsm_storage::{LsmStorageOptions, MiniLsm},
    tests::harness::check_lsm_iter_result_by_key,
};

#[test]
fn test_txn_integration() {
    let dir = tempdir().unwrap();
    let options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    let storage = MiniLsm::open(&dir, options.clone()).unwrap();
    let txn1 = storage.new_txn().unwrap();
    let txn2 = storage.new_txn().unwrap();
    txn1.put(b"test1", b"233");
    txn2.put(b"test2", b"233");
    check_lsm_iter_result_by_key(
        &mut txn1.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![(Bytes::from("test1"), Bytes::from("233"))],
    );
    check_lsm_iter_result_by_key(
        &mut txn2.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![(Bytes::from("test2"), Bytes::from("233"))],
    );
    let txn3 = storage.new_txn().unwrap();
    check_lsm_iter_result_by_key(
        &mut txn3.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![],
    );
    txn1.commit().unwrap();
    txn2.commit().unwrap();
    check_lsm_iter_result_by_key(
        &mut txn3.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![],
    );
    drop(txn3);
    check_lsm_iter_result_by_key(
        &mut storage.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("test1"), Bytes::from("233")),
            (Bytes::from("test2"), Bytes::from("233")),
        ],
    );
    let txn4 = storage.new_txn().unwrap();
    assert_eq!(txn4.get(b"test1").unwrap(), Some(Bytes::from("233")));
    assert_eq!(txn4.get(b"test2").unwrap(), Some(Bytes::from("233")));
    check_lsm_iter_result_by_key(
        &mut txn4.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("test1"), Bytes::from("233")),
            (Bytes::from("test2"), Bytes::from("233")),
        ],
    );
    txn4.put(b"test2", b"2333");
    assert_eq!(txn4.get(b"test1").unwrap(), Some(Bytes::from("233")));
    assert_eq!(txn4.get(b"test2").unwrap(), Some(Bytes::from("2333")));
    check_lsm_iter_result_by_key(
        &mut txn4.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("test1"), Bytes::from("233")),
            (Bytes::from("test2"), Bytes::from("2333")),
        ],
    );
    txn4.delete(b"test2");
    assert_eq!(txn4.get(b"test1").unwrap(), Some(Bytes::from("233")));
    assert_eq!(txn4.get(b"test2").unwrap(), None);
    check_lsm_iter_result_by_key(
        &mut txn4.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![(Bytes::from("test1"), Bytes::from("233"))],
    );
}
