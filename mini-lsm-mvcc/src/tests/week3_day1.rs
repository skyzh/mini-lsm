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

use std::sync::Arc;

use bytes::Bytes;
use tempfile::tempdir;

use crate::key::KeySlice;
use crate::table::{FileObject, SsTable, SsTableBuilder, SsTableIterator};

use super::harness::{check_iter_result_by_key_and_ts, generate_sst_with_ts};

#[test]
fn test_sst_build_multi_version_simple() {
    let mut builder = SsTableBuilder::new(16);
    builder.add(
        KeySlice::for_testing_from_slice_with_ts(b"233", 233),
        b"233333",
    );
    builder.add(
        KeySlice::for_testing_from_slice_with_ts(b"233", 0),
        b"2333333",
    );
    let dir = tempdir().unwrap();
    builder.build_for_test(dir.path().join("1.sst")).unwrap();
}

fn generate_test_data() -> Vec<((Bytes, u64), Bytes)> {
    (0..100)
        .map(|id| {
            (
                (Bytes::from(format!("key{:05}", id / 5)), 5 - (id % 5)),
                Bytes::from(format!("value{:05}", id)),
            )
        })
        .collect()
}

#[test]
fn test_sst_build_multi_version_hard() {
    let dir = tempdir().unwrap();
    let data = generate_test_data();
    generate_sst_with_ts(1, dir.path().join("1.sst"), data.clone(), None);
    let sst = Arc::new(
        SsTable::open(
            1,
            None,
            FileObject::open(&dir.path().join("1.sst")).unwrap(),
        )
        .unwrap(),
    );
    check_iter_result_by_key_and_ts(
        &mut SsTableIterator::create_and_seek_to_first(sst).unwrap(),
        data,
    );
}
