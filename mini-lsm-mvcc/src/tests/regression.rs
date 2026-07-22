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

use bytes::Bytes;
use tempfile::tempdir;

use crate::{
    compact::CompactionOptions,
    lsm_storage::{LsmStorageOptions, MiniLsm, WriteBatchRecord},
};

#[test]
fn test_read_only_txn_does_not_advance_commit_ts() {
    let dir = tempdir().unwrap();
    let options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    let storage = MiniLsm::open(&dir, options).unwrap();
    storage.put(b"key", b"value").unwrap();
    let commit_ts = storage.inner.mvcc().latest_commit_ts();

    let txn = storage.new_txn().unwrap();
    assert_eq!(txn.get(b"key").unwrap(), Some(Bytes::from("value")));
    txn.commit().unwrap();

    assert_eq!(storage.inner.mvcc().latest_commit_ts(), commit_ts);

    let empty_batch: &[WriteBatchRecord<&[u8]>] = &[];
    storage.write_batch(empty_batch).unwrap();
    assert_eq!(storage.inner.mvcc().latest_commit_ts(), commit_ts);
}
