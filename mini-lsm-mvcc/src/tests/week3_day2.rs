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

use std::ops::Bound;

use bytes::Bytes;
use tempfile::tempdir;

use crate::{
    compact::CompactionOptions,
    lsm_storage::{LsmStorageOptions, MiniLsm, WriteBatchRecord},
};

use super::harness::{
    check_iter_result_by_key_and_ts, check_lsm_iter_result_by_key,
    construct_merge_iterator_over_storage,
};

#[test]
fn test_timestamped_batches_and_latest_reads() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.enable_wal = true;
    let storage = MiniLsm::open(&dir, options).unwrap();
    storage
        .write_batch(&[
            WriteBatchRecord::Put(b"a", b"1"),
            WriteBatchRecord::Put(b"b", b"1"),
        ])
        .unwrap();
    storage
        .write_batch(&[
            WriteBatchRecord::Put(b"a", b"2"),
            WriteBatchRecord::Del(b"b"),
        ])
        .unwrap();
    storage.force_flush().unwrap();

    let mut raw_iter = construct_merge_iterator_over_storage(&storage.inner.state.read());
    check_iter_result_by_key_and_ts(
        &mut raw_iter,
        vec![
            ((Bytes::from("a"), 2), Bytes::from("2")),
            ((Bytes::from("a"), 1), Bytes::from("1")),
            ((Bytes::from("b"), 2), Bytes::new()),
            ((Bytes::from("b"), 1), Bytes::from("1")),
        ],
    );
    assert_eq!(storage.get(b"a").unwrap(), Some(Bytes::from("2")));
    assert_eq!(storage.get(b"b").unwrap(), None);
    check_lsm_iter_result_by_key(
        &mut storage.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![(Bytes::from("a"), Bytes::from("2"))],
    );
}
