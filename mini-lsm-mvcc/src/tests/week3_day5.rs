// Copyright (c) 2022-2026 Alex Chi Z
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

use std::{ops::Bound, sync::Arc};

use bytes::Bytes;
use crossbeam_skiplist::SkipMap;
use tempfile::tempdir;

use crate::{
    compact::CompactionOptions,
    key::{KeyBytes, KeySlice},
    lsm_storage::{LsmStorageInner, LsmStorageOptions, MiniLsm},
    tests::harness::check_lsm_iter_result_by_key,
    wal::Wal,
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

#[test]
fn test_task4_batch_uses_one_memtable_and_timestamp() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.enable_wal = true;
    options.target_sst_size = 1;
    let storage = Arc::new(LsmStorageInner::open(&dir, options).unwrap());

    let txn = storage.new_txn().unwrap();
    txn.put(b"a", b"1");
    txn.put(b"b", b"2");
    txn.commit().unwrap();

    let commit_ts = storage.mvcc().latest_commit_ts();
    let state = storage.state.read();
    assert_eq!(state.imm_memtables.len(), 1);
    assert_eq!(
        state.imm_memtables[0].get(KeySlice::from_slice(b"a", commit_ts)),
        Some(Bytes::from_static(b"1"))
    );
    assert_eq!(
        state.imm_memtables[0].get(KeySlice::from_slice(b"b", commit_ts)),
        Some(Bytes::from_static(b"2"))
    );
}

#[test]
fn test_task4_wal_batch_round_trip() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");
    let wal = Wal::create(&path).unwrap();
    wal.put_batch(&[
        (KeySlice::from_slice(b"a", 2), b"1"),
        (KeySlice::from_slice(b"b", 2), b""),
    ])
    .unwrap();
    wal.sync().unwrap();
    drop(wal);

    let map = SkipMap::<KeyBytes, Bytes>::new();
    let recovered = Wal::recover(&path, &map).unwrap();
    drop(recovered);
    assert_eq!(map.len(), 2);
    assert_eq!(
        map.get(&KeyBytes::from_bytes_with_ts(Bytes::from_static(b"a"), 2))
            .unwrap()
            .value(),
        &Bytes::from_static(b"1")
    );
    assert!(
        map.get(&KeyBytes::from_bytes_with_ts(Bytes::from_static(b"b"), 2))
            .unwrap()
            .value()
            .is_empty()
    );
}

#[test]
fn test_task4_wal_rejects_truncated_or_corrupt_batch_without_applying_it() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");
    let wal = Wal::create(&path).unwrap();
    wal.put_batch(&[
        (KeySlice::from_slice(b"a", 2), b"1"),
        (KeySlice::from_slice(b"b", 2), b"2"),
    ])
    .unwrap();
    wal.sync().unwrap();
    drop(wal);

    let encoded = std::fs::read(&path).unwrap();
    for (idx, cutoff) in [1, encoded.len() / 2, encoded.len() - 1]
        .into_iter()
        .enumerate()
    {
        let truncated_path = dir.path().join(format!("truncated-{idx}.wal"));
        std::fs::write(&truncated_path, &encoded[..cutoff]).unwrap();
        let map = SkipMap::<KeyBytes, Bytes>::new();
        assert!(Wal::recover(&truncated_path, &map).is_err());
        assert!(map.is_empty());
    }

    let corrupt_path = dir.path().join("corrupt.wal");
    let mut corrupt = encoded;
    let checksum_byte = corrupt.last_mut().unwrap();
    *checksum_byte ^= 0xff;
    std::fs::write(&corrupt_path, corrupt).unwrap();
    let map = SkipMap::<KeyBytes, Bytes>::new();
    assert!(Wal::recover(&corrupt_path, &map).is_err());
    assert!(map.is_empty());
}

#[test]
fn test_task4_wal_rejects_oversized_fields() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.wal");
    let wal = Wal::create(&path).unwrap();
    let oversized_value = vec![0; usize::from(u16::MAX) + 1];
    assert!(
        wal.put_batch(&[(KeySlice::from_slice(b"key", 1), oversized_value.as_slice(),)])
            .is_err()
    );
}
