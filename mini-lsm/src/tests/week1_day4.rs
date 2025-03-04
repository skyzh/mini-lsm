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
use tempfile::{tempdir, TempDir};

use crate::iterators::StorageIterator;
use crate::key::{KeySlice, KeyVec};
use crate::table::{SsTable, SsTableBuilder, SsTableIterator};

#[test]
fn test_sst_build_single_key() {
    let mut builder = SsTableBuilder::new(16);
    builder.add(KeySlice::for_testing_from_slice_no_ts(b"233"), b"233333");
    let dir = tempdir().unwrap();
    builder.build_for_test(dir.path().join("1.sst")).unwrap();
}

#[test]
fn test_sst_build_two_blocks() {
    let mut builder = SsTableBuilder::new(16);
    builder.add(KeySlice::for_testing_from_slice_no_ts(b"11"), b"11");
    builder.add(KeySlice::for_testing_from_slice_no_ts(b"22"), b"22");
    builder.add(KeySlice::for_testing_from_slice_no_ts(b"33"), b"11");
    builder.add(KeySlice::for_testing_from_slice_no_ts(b"44"), b"22");
    builder.add(KeySlice::for_testing_from_slice_no_ts(b"55"), b"11");
    builder.add(KeySlice::for_testing_from_slice_no_ts(b"66"), b"22");
    assert!(builder.meta.len() >= 2);
    let dir = tempdir().unwrap();
    builder.build_for_test(dir.path().join("1.sst")).unwrap();
}

fn key_of(idx: usize) -> KeyVec {
    KeyVec::for_testing_from_vec_no_ts(format!("key_{:03}", idx * 5).into_bytes())
}

fn value_of(idx: usize) -> Vec<u8> {
    format!("value_{:010}", idx).into_bytes()
}

fn num_of_keys() -> usize {
    100
}

fn generate_sst() -> (TempDir, SsTable) {
    let mut builder = SsTableBuilder::new(128);
    for idx in 0..num_of_keys() {
        let key = key_of(idx);
        let value = value_of(idx);
        builder.add(key.as_key_slice(), &value[..]);
    }
    let dir = tempdir().unwrap();
    let path = dir.path().join("1.sst");
    (dir, builder.build_for_test(path).unwrap())
}

#[test]
fn test_sst_build_all() {
    let (_, sst) = generate_sst();
    assert_eq!(sst.first_key().as_key_slice(), key_of(0).as_key_slice());
    assert_eq!(
        sst.last_key().as_key_slice(),
        key_of(num_of_keys() - 1).as_key_slice()
    )
}

#[test]
fn test_sst_decode() {
    let (_dir, sst) = generate_sst();
    let meta = sst.block_meta.clone();
    let new_sst = SsTable::open_for_test(sst.file).unwrap();
    assert_eq!(new_sst.block_meta, meta);
    assert_eq!(
        new_sst.first_key().for_testing_key_ref(),
        key_of(0).for_testing_key_ref()
    );
    assert_eq!(
        new_sst.last_key().for_testing_key_ref(),
        key_of(num_of_keys() - 1).for_testing_key_ref()
    );
}

fn as_bytes(x: &[u8]) -> Bytes {
    Bytes::copy_from_slice(x)
}

#[test]
fn test_sst_iterator() {
    let (_dir, sst) = generate_sst();
    let sst = Arc::new(sst);
    let mut iter = SsTableIterator::create_and_seek_to_first(sst).unwrap();
    for _ in 0..5 {
        for i in 0..num_of_keys() {
            let key = iter.key();
            let value = iter.value();
            assert_eq!(
                key.for_testing_key_ref(),
                key_of(i).for_testing_key_ref(),
                "expected key: {:?}, actual key: {:?}",
                as_bytes(key_of(i).for_testing_key_ref()),
                as_bytes(key.for_testing_key_ref())
            );
            assert_eq!(
                value,
                value_of(i),
                "expected value: {:?}, actual value: {:?}",
                as_bytes(&value_of(i)),
                as_bytes(value)
            );
            iter.next().unwrap();
        }
        iter.seek_to_first().unwrap();
    }
}

#[test]
fn test_sst_seek_key() {
    let (_dir, sst) = generate_sst();
    let sst = Arc::new(sst);
    let mut iter = SsTableIterator::create_and_seek_to_key(sst, key_of(0).as_key_slice()).unwrap();
    for offset in 1..=5 {
        for i in 0..num_of_keys() {
            let key = iter.key();
            let value = iter.value();
            assert_eq!(
                key.for_testing_key_ref(),
                key_of(i).for_testing_key_ref(),
                "expected key: {:?}, actual key: {:?}",
                as_bytes(key_of(i).for_testing_key_ref()),
                as_bytes(key.for_testing_key_ref())
            );
            assert_eq!(
                value,
                value_of(i),
                "expected value: {:?}, actual value: {:?}",
                as_bytes(&value_of(i)),
                as_bytes(value)
            );
            iter.seek_to_key(KeySlice::for_testing_from_slice_no_ts(
                &format!("key_{:03}", i * 5 + offset).into_bytes(),
            ))
            .unwrap();
        }
        iter.seek_to_key(KeySlice::for_testing_from_slice_no_ts(b"k"))
            .unwrap();
    }
}
