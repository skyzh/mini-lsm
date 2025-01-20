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
    iterators::StorageIterator,
    lsm_storage::{LsmStorageOptions, MiniLsm},
};

#[test]
fn test_serializable_1() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.serializable = true;
    let storage = MiniLsm::open(&dir, options.clone()).unwrap();
    storage.put(b"key1", b"1").unwrap();
    storage.put(b"key2", b"2").unwrap();
    let txn1 = storage.new_txn().unwrap();
    let txn2 = storage.new_txn().unwrap();
    txn1.put(b"key1", &txn1.get(b"key2").unwrap().unwrap());
    txn2.put(b"key2", &txn2.get(b"key1").unwrap().unwrap());
    txn1.commit().unwrap();
    assert!(txn2.commit().is_err());
    drop(txn2);
    assert_eq!(storage.get(b"key1").unwrap(), Some(Bytes::from("2")));
    assert_eq!(storage.get(b"key2").unwrap(), Some(Bytes::from("2")));
}

#[test]
fn test_serializable_2() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.serializable = true;
    let storage = MiniLsm::open(&dir, options.clone()).unwrap();
    let txn1 = storage.new_txn().unwrap();
    let txn2 = storage.new_txn().unwrap();
    txn1.put(b"key1", b"1");
    txn2.put(b"key1", b"2");
    txn1.commit().unwrap();
    txn2.commit().unwrap();
    assert_eq!(storage.get(b"key1").unwrap(), Some(Bytes::from("2")));
}

#[test]
fn test_serializable_3_ts_range() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.serializable = true;
    let storage = MiniLsm::open(&dir, options.clone()).unwrap();
    storage.put(b"key1", b"1").unwrap();
    storage.put(b"key2", b"2").unwrap();
    let txn1 = storage.new_txn().unwrap();
    txn1.put(b"key1", &txn1.get(b"key2").unwrap().unwrap());
    txn1.commit().unwrap();
    let txn2 = storage.new_txn().unwrap();
    txn2.put(b"key2", &txn2.get(b"key1").unwrap().unwrap());
    txn2.commit().unwrap();
    drop(txn2);
    assert_eq!(storage.get(b"key1").unwrap(), Some(Bytes::from("2")));
    assert_eq!(storage.get(b"key2").unwrap(), Some(Bytes::from("2")));
}

#[test]
fn test_serializable_4_scan() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.serializable = true;
    let storage = MiniLsm::open(&dir, options.clone()).unwrap();
    storage.put(b"key1", b"1").unwrap();
    storage.put(b"key2", b"2").unwrap();
    let txn1 = storage.new_txn().unwrap();
    let txn2 = storage.new_txn().unwrap();
    txn1.put(b"key1", &txn1.get(b"key2").unwrap().unwrap());
    txn1.commit().unwrap();
    let mut iter = txn2.scan(Bound::Unbounded, Bound::Unbounded).unwrap();
    while iter.is_valid() {
        iter.next().unwrap();
    }
    txn2.put(b"key2", b"1");
    assert!(txn2.commit().is_err());
    drop(txn2);
    assert_eq!(storage.get(b"key1").unwrap(), Some(Bytes::from("2")));
    assert_eq!(storage.get(b"key2").unwrap(), Some(Bytes::from("2")));
}

#[test]
fn test_serializable_5_read_only() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.serializable = true;
    let storage = MiniLsm::open(&dir, options.clone()).unwrap();
    storage.put(b"key1", b"1").unwrap();
    storage.put(b"key2", b"2").unwrap();
    let txn1 = storage.new_txn().unwrap();
    txn1.put(b"key1", &txn1.get(b"key2").unwrap().unwrap());
    txn1.commit().unwrap();
    let txn2 = storage.new_txn().unwrap();
    txn2.get(b"key1").unwrap().unwrap();
    let mut iter = txn2.scan(Bound::Unbounded, Bound::Unbounded).unwrap();
    while iter.is_valid() {
        iter.next().unwrap();
    }
    txn2.commit().unwrap();
    assert_eq!(storage.get(b"key1").unwrap(), Some(Bytes::from("2")));
    assert_eq!(storage.get(b"key2").unwrap(), Some(Bytes::from("2")));
}
